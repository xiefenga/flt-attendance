use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use calamine::{Data, ExcelDateTime, ExcelDateTimeType, Reader, open_workbook_auto};
use serde::Serialize;
use thiserror::Error;

use crate::model::{
    AnnualLeaveRecord, AttendanceDataset, AttendancePeriod, CalendarDate, DailyRecord,
    EmploymentRecord, InvalidPunch, MonthlyRecord, PunchKind, PunchSlot,
};

const REQUIRED_SHEETS: [&str; 4] = ["打卡时间", "原始记录", "月度汇总", "每日统计"];
const EMPLOYMENT_SHEETS: [&str; 2] = ["入职名单", "离职名单"];
const ANNUAL_LEAVE_SHEET: &str = "年假明细";

#[derive(Debug, Error)]
pub enum DingtalkError {
    #[error("无法读取钉钉工作簿：{0}")]
    Open(#[from] calamine::Error),
    #[error("钉钉工作簿缺少工作表：{0}")]
    MissingSheet(String),
    #[error("工作表 {sheet} 缺少必需字段：{missing}")]
    MissingHeaders { sheet: String, missing: String },
    #[error("每日统计表中没有有效的考勤日期")]
    MissingPeriod,
    #[error("每日统计表包含多个考勤月份：{0}")]
    MixedPeriod(String),
    #[error("工作表 {sheet} 第 {row} 行的{field}无效")]
    InvalidEmploymentDate {
        sheet: String,
        row: usize,
        field: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorksheetSummary {
    pub name: String,
    pub rows: usize,
    pub columns: usize,
    pub data_rows: usize,
    pub unique_employees: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkbookSummary {
    pub period: AttendancePeriod,
    pub sheets: Vec<WorksheetSummary>,
    pub employees: Vec<EmployeeIdentity>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EmployeeIdentity {
    pub employee_no: String,
    pub name: String,
}

impl WorkbookSummary {
    pub fn sheet(&self, name: &str) -> Option<&WorksheetSummary> {
        self.sheets.iter().find(|sheet| sheet.name == name)
    }
}

pub fn inspect_dingtalk(path: impl AsRef<Path>) -> Result<WorkbookSummary, DingtalkError> {
    let mut workbook = open_workbook_auto(path)?;
    let available: BTreeSet<String> = workbook.sheet_names().iter().cloned().collect();

    for required in REQUIRED_SHEETS {
        if !available.contains(required) {
            return Err(DingtalkError::MissingSheet(required.to_owned()));
        }
    }

    let inspected_sheets = REQUIRED_SHEETS
        .into_iter()
        .chain(
            EMPLOYMENT_SHEETS
                .into_iter()
                .filter(|name| available.contains(*name)),
        )
        .chain(std::iter::once(ANNUAL_LEAVE_SHEET).filter(|name| available.contains(*name)))
        .collect::<Vec<_>>();
    let mut sheets = Vec::with_capacity(inspected_sheets.len());
    let mut employee_list = Vec::new();
    let mut period = None;
    for sheet_name in inspected_sheets {
        let range = workbook.worksheet_range(sheet_name)?;
        validate_headers(sheet_name, &range)?;
        if sheet_name == "每日统计" {
            period = Some(read_period(&range)?);
        } else if sheet_name == "月度汇总" {
            employee_list = range
                .rows()
                .skip(4)
                .filter_map(|row| {
                    let name = cell_text(row.first());
                    (!name.is_empty()).then(|| EmployeeIdentity {
                        employee_no: cell_text(row.get(3)),
                        name,
                    })
                })
                .collect();
        }

        let header_rows = if sheet_name == ANNUAL_LEAVE_SHEET {
            3
        } else {
            match sheet_name {
                "入职名单" | "离职名单" => 1,
                "打卡时间" | "月度汇总" | "每日统计" => 4,
                _ => 3,
            }
        };
        let mut employees = HashSet::new();
        let mut data_rows = 0;

        for row in range.rows().skip(header_rows) {
            let (employee_no, employee_name) = if sheet_name == ANNUAL_LEAVE_SHEET {
                (cell_text(row.first()), cell_text(row.get(1)))
            } else {
                (cell_text(row.get(3)), cell_text(row.first()))
            };
            if employee_name.is_empty() {
                continue;
            }
            data_rows += 1;
            employees.insert((employee_no, employee_name));
        }

        sheets.push(WorksheetSummary {
            name: sheet_name.to_owned(),
            rows: range.height(),
            columns: range.width(),
            data_rows,
            unique_employees: employees.len(),
        });
    }

    Ok(WorkbookSummary {
        period: period.ok_or(DingtalkError::MissingPeriod)?,
        sheets,
        employees: employee_list,
    })
}

pub fn load_dingtalk(path: impl AsRef<Path>) -> Result<AttendanceDataset, DingtalkError> {
    let mut workbook = open_workbook_auto(path)?;
    validate_required_sheets(&workbook)?;

    let monthly_range = workbook.worksheet_range("月度汇总")?;
    validate_headers("月度汇总", &monthly_range)?;
    let monthly = parse_monthly(&monthly_range);

    let daily_range = workbook.worksheet_range("每日统计")?;
    validate_headers("每日统计", &daily_range)?;
    let period = read_period(&daily_range)?;
    let daily = parse_daily(&daily_range);

    let raw_range = workbook.worksheet_range("原始记录")?;
    validate_headers("原始记录", &raw_range)?;
    let invalid_punches = parse_invalid_punches(&raw_range);
    let employment_records = parse_employment_records(&mut workbook)?;
    let annual_leave_records = parse_annual_leave_records(&mut workbook)?;

    Ok(AttendanceDataset {
        period,
        monthly,
        daily,
        invalid_punches,
        employment_records,
        annual_leave_records,
    })
}

fn parse_annual_leave_records<RS>(
    workbook: &mut calamine::Sheets<RS>,
) -> Result<Vec<AnnualLeaveRecord>, DingtalkError>
where
    RS: std::io::Read + std::io::Seek,
{
    if !workbook
        .sheet_names()
        .iter()
        .any(|name| name == ANNUAL_LEAVE_SHEET)
    {
        return Ok(Vec::new());
    }

    let range = workbook.worksheet_range(ANNUAL_LEAVE_SHEET)?;
    validate_headers(ANNUAL_LEAVE_SHEET, &range)?;
    Ok(range
        .rows()
        .skip(3)
        .filter_map(|row| {
            let name = cell_text(row.get(1));
            let balance_before_month_hours = cell_optional_number(row.get(9))?;
            (!name.is_empty()).then(|| AnnualLeaveRecord {
                employee_no: cell_text(row.first()),
                name,
                company: cell_text(row.get(2)),
                balance_before_month_hours,
            })
        })
        .collect())
}

fn parse_employment_records<RS>(
    workbook: &mut calamine::Sheets<RS>,
) -> Result<Vec<EmploymentRecord>, DingtalkError>
where
    RS: std::io::Read + std::io::Seek,
{
    let available: HashSet<String> = workbook.sheet_names().iter().cloned().collect();
    let mut records = std::collections::BTreeMap::<String, EmploymentRecord>::new();

    for (sheet_name, is_hire) in [("入职名单", true), ("离职名单", false)] {
        if !available.contains(sheet_name) {
            continue;
        }
        let range = workbook.worksheet_range(sheet_name)?;
        validate_headers(sheet_name, &range)?;
        for (index, row) in range.rows().skip(1).enumerate() {
            let name = cell_text(row.first());
            if name.is_empty() {
                continue;
            }
            let employee_no = cell_text(row.get(2));
            let date =
                cell_date(row.get(3)).ok_or_else(|| DingtalkError::InvalidEmploymentDate {
                    sheet: sheet_name.to_owned(),
                    row: index + 2,
                    field: if is_hire {
                        "入职日期"
                    } else {
                        "离职日期"
                    }
                    .to_owned(),
                })?;
            let key = if employee_no.is_empty() {
                format!("name:{name}")
            } else {
                format!("employee:{employee_no}")
            };
            let record = records.entry(key).or_insert_with(|| EmploymentRecord {
                employee_no: employee_no.clone(),
                name: name.clone(),
                company: cell_text(row.get(1)),
                hire_date: None,
                termination_date: None,
            });
            if is_hire {
                record.hire_date = Some(date);
            } else {
                record.termination_date = Some(date);
            }
        }
    }

    Ok(records.into_values().collect())
}

pub fn load_company_history(
    path: impl AsRef<Path>,
) -> Result<HashMap<String, String>, DingtalkError> {
    let mut workbook = open_workbook_auto(path)?;
    let range = workbook
        .worksheet_range("考勤明细")
        .map_err(|_| DingtalkError::MissingSheet("考勤明细".to_owned()))?;
    let mut companies = HashMap::new();
    for row in range.rows().skip(4) {
        let name = cell_text(row.get(2));
        let company = cell_text(row.get(1));
        if !name.is_empty() && !company.is_empty() {
            companies.insert(name, company);
        }
    }
    Ok(companies)
}

fn read_period(range: &calamine::Range<Data>) -> Result<AttendancePeriod, DingtalkError> {
    let periods: BTreeSet<(u16, u8)> = range
        .rows()
        .skip(4)
        .filter_map(|row| cell_period(row.get(6)))
        .collect();

    if periods.is_empty() {
        Err(DingtalkError::MissingPeriod)
    } else if periods.len() == 1 {
        let (year, month) = periods.iter().next().expect("period set is not empty");
        Ok(AttendancePeriod {
            year: *year,
            month: *month,
        })
    } else {
        Err(DingtalkError::MixedPeriod(
            periods
                .iter()
                .map(|(year, month)| format!("{year}年{month}月"))
                .collect::<Vec<_>>()
                .join("、"),
        ))
    }
}

fn cell_period(cell: Option<&Data>) -> Option<(u16, u8)> {
    match cell? {
        Data::DateTime(value) if value.is_datetime() => {
            let (year, month, ..) = value.to_ymd_hms_milli();
            valid_period(year, month)
        }
        Data::Float(value) => period_from_excel_serial(*value),
        Data::Int(value) => period_from_excel_serial(*value as f64),
        Data::DateTimeIso(value) | Data::String(value) => period_from_text(value),
        _ => None,
    }
}

fn period_from_excel_serial(value: f64) -> Option<(u16, u8)> {
    if !(36_526.0..73_416.0).contains(&value) {
        return None;
    }
    let datetime = ExcelDateTime::new(value, ExcelDateTimeType::DateTime, false);
    let (year, month, ..) = datetime.to_ymd_hms_milli();
    valid_period(year, month)
}

fn period_from_text(value: &str) -> Option<(u16, u8)> {
    let parts: Vec<&str> = value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 2 || !matches!(parts[0].len(), 2 | 4) {
        return None;
    }
    if parts[0].len() == 2 && parts.get(2).is_none_or(|day| day.len() > 2) {
        return None;
    }
    let raw_year: u16 = parts[0].parse().ok()?;
    let year = if parts[0].len() == 2 {
        2000 + raw_year
    } else {
        raw_year
    };
    valid_period(year, parts[1].parse().ok()?)
}

fn cell_date(cell: Option<&Data>) -> Option<CalendarDate> {
    match cell? {
        Data::DateTime(value) if value.is_datetime() => {
            let (year, month, day, ..) = value.to_ymd_hms_milli();
            valid_date(year, month, day)
        }
        Data::Float(value) => date_from_excel_serial(*value),
        Data::Int(value) => date_from_excel_serial(*value as f64),
        Data::DateTimeIso(value) | Data::String(value) => date_from_text(value),
        _ => None,
    }
}

fn date_from_excel_serial(value: f64) -> Option<CalendarDate> {
    if !(36_526.0..73_416.0).contains(&value) {
        return None;
    }
    let datetime = ExcelDateTime::new(value, ExcelDateTimeType::DateTime, false);
    let (year, month, day, ..) = datetime.to_ymd_hms_milli();
    valid_date(year, month, day)
}

fn date_from_text(value: &str) -> Option<CalendarDate> {
    let parts = value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let raw_year: u16 = parts[0].parse().ok()?;
    let year = match parts[0].len() {
        2 => 2000 + raw_year,
        4 => raw_year,
        _ => return None,
    };
    valid_date(year, parts[1].parse().ok()?, parts[2].parse().ok()?)
}

fn valid_date(year: u16, month: u8, day: u8) -> Option<CalendarDate> {
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => return None,
    };
    ((2000..=2100).contains(&year) && (1..=max_day).contains(&day)).then_some(CalendarDate {
        year,
        month,
        day,
    })
}

fn valid_period(year: u16, month: u8) -> Option<(u16, u8)> {
    (2000..=2100)
        .contains(&year)
        .then_some(())
        .and_then(|_| (1..=12).contains(&month).then_some((year, month)))
}

fn validate_required_sheets<RS>(workbook: &calamine::Sheets<RS>) -> Result<(), DingtalkError>
where
    RS: std::io::Read + std::io::Seek,
{
    let available: BTreeSet<String> = workbook.sheet_names().iter().cloned().collect();
    for required in REQUIRED_SHEETS {
        if !available.contains(required) {
            return Err(DingtalkError::MissingSheet(required.to_owned()));
        }
    }
    Ok(())
}

fn parse_monthly(range: &calamine::Range<Data>) -> Vec<MonthlyRecord> {
    range
        .rows()
        .skip(4)
        .filter(|row| !cell_text(row.first()).is_empty())
        .map(|row| {
            let employee_no = cell_text(row.get(3));
            let user_id = cell_text(row.get(5));
            MonthlyRecord {
                employee_key: employee_key(&employee_no, &user_id),
                name: cell_text(row.first()),
                attendance_group: cell_text(row.get(1)),
                department: cell_text(row.get(2)),
                employee_no,
                position: cell_text(row.get(4)),
                user_id,
                attendance_days: cell_number(row.get(6)),
                weekday_overtime_hours: cell_number(row.get(7)),
                weekend_overtime_hours: cell_number(row.get(8)),
                holiday_overtime_hours: cell_number(row.get(9)),
                personal_leave_hours: cell_number(row.get(10)),
                compensatory_leave_hours: cell_number(row.get(11)),
                sick_leave_hours: cell_number(row.get(12)),
                annual_leave_hours: cell_number(row.get(13)),
                maternity_leave_days: cell_number(row.get(14)),
                paternity_leave_days: cell_number(row.get(15)),
                marriage_leave_days: cell_number(row.get(16)),
                menstrual_leave_days: cell_number(row.get(17)),
                bereavement_leave_days: cell_number(row.get(18)),
                breastfeeding_leave_hours: cell_number(row.get(19)),
                daily_results: row
                    .iter()
                    .skip(21)
                    .map(|cell| cell_text(Some(cell)))
                    .collect(),
            }
        })
        .collect()
}

fn parse_daily(range: &calamine::Range<Data>) -> Vec<DailyRecord> {
    range
        .rows()
        .skip(4)
        .filter(|row| !cell_text(row.first()).is_empty())
        .map(|row| {
            let employee_no = cell_text(row.get(3));
            let user_id = cell_text(row.get(5));
            let mut punch_slots = Vec::with_capacity(6);
            for (kind, time_column, result_column) in [
                (PunchKind::In, 9, 10),
                (PunchKind::Out, 11, 12),
                (PunchKind::In, 13, 14),
                (PunchKind::Out, 15, 16),
                (PunchKind::In, 17, 18),
                (PunchKind::Out, 19, 20),
            ] {
                let time = cell_text(row.get(time_column));
                let result = cell_text(row.get(result_column));
                if !time.is_empty() || !result.is_empty() {
                    punch_slots.push(PunchSlot { kind, time, result });
                }
            }

            DailyRecord {
                employee_key: employee_key(&employee_no, &user_id),
                employee_no,
                name: cell_text(row.first()),
                date: cell_text(row.get(6)),
                shift: cell_text(row.get(8)),
                overtime_hours: cell_number(row.get(40)),
                punch_slots,
                late_count: cell_number(row.get(23)),
                severe_late_count: cell_number(row.get(24)),
                absent_late_days: cell_number(row.get(25)),
                early_count: cell_number(row.get(26)),
                missing_in_count: cell_number(row.get(27)),
                missing_out_count: cell_number(row.get(28)),
                absent_days: cell_number(row.get(29)),
            }
        })
        .collect()
}

fn parse_invalid_punches(range: &calamine::Range<Data>) -> Vec<InvalidPunch> {
    range
        .rows()
        .skip(3)
        .filter_map(|row| {
            let result = cell_text(row.get(9));
            if !result.contains("当前不在可打卡的时间范围") {
                return None;
            }
            let employee_no = cell_text(row.get(3));
            let user_id = cell_text(row.get(5));
            Some(InvalidPunch {
                employee_key: employee_key(&employee_no, &user_id),
                employee_no,
                name: cell_text(row.first()),
                attendance_date: cell_text(row.get(6)),
                punch_time: cell_text(row.get(8)),
                result,
            })
        })
        .collect()
}

fn validate_headers(sheet_name: &str, range: &calamine::Range<Data>) -> Result<(), DingtalkError> {
    let required = match sheet_name {
        "打卡时间" => &["姓名", "工号", "UserId", "打卡时间"][..],
        "原始记录" => &["姓名", "工号", "考勤日期", "打卡时间", "打卡结果"][..],
        "月度汇总" => &[
            "姓名",
            "工号",
            "工作日加班",
            "休息日加班",
            "节假日加班",
            "加班总时长",
        ][..],
        "每日统计" => &[
            "姓名",
            "工号",
            "日期",
            "班次",
            "上班1打卡时间",
            "下班1打卡时间",
            "加班总时长",
        ][..],
        "入职名单" => &["姓名", "工号", "入职日期"][..],
        "离职名单" => &["姓名", "工号", "离职日期"][..],
        ANNUAL_LEAVE_SHEET => &["工号", "姓名", "公司", "截止当前月剩余（小时）"][..],
        _ => &[][..],
    };

    let header_rows = range.rows().take(4);
    let present: HashSet<String> = header_rows
        .flat_map(|row| row.iter())
        .map(|cell| normalized_header(&cell.to_string()))
        .collect();
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|header| !present.contains(&normalized_header(header)))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(DingtalkError::MissingHeaders {
            sheet: sheet_name.to_owned(),
            missing: missing.join("、"),
        })
    }
}

fn normalized_header(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn cell_text(cell: Option<&Data>) -> String {
    match cell {
        None | Some(Data::Empty) => String::new(),
        Some(value) => value.to_string().trim().to_owned(),
    }
}

fn cell_number(cell: Option<&Data>) -> f64 {
    match cell {
        Some(Data::Float(value)) => *value,
        Some(Data::Int(value)) => *value as f64,
        Some(Data::String(value)) => value.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn cell_optional_number(cell: Option<&Data>) -> Option<f64> {
    match cell {
        Some(Data::Float(value)) => Some(*value),
        Some(Data::Int(value)) => Some(*value as f64),
        Some(Data::String(value)) => value.trim().parse().ok(),
        _ => None,
    }
}

fn employee_key(employee_no: &str, user_id: &str) -> String {
    if employee_no.is_empty() {
        format!("user:{user_id}")
    } else {
        format!("employee:{employee_no}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;

    #[test]
    fn missing_sheet_error_is_readable() {
        let error = DingtalkError::MissingSheet("每日统计".to_owned());
        assert_eq!(error.to_string(), "钉钉工作簿缺少工作表：每日统计");
    }

    #[test]
    fn reads_period_from_common_date_text() {
        assert_eq!(period_from_text("2026-07-01"), Some((2026, 7)));
        assert_eq!(period_from_text("2026/7/31 00:00:00"), Some((2026, 7)));
        assert_eq!(period_from_text("2026年07月01日"), Some((2026, 7)));
        assert_eq!(period_from_text("26-07-01 星期三"), Some((2026, 7)));
        assert_eq!(period_from_text("07-01-2026"), None);
    }

    #[test]
    fn reads_employment_dates_from_serial_and_text() {
        assert_eq!(
            date_from_excel_serial(46_234.0),
            Some(CalendarDate {
                year: 2026,
                month: 7,
                day: 31,
            })
        );
        assert_eq!(
            date_from_text("2026/7/31"),
            Some(CalendarDate {
                year: 2026,
                month: 7,
                day: 31,
            })
        );
        assert_eq!(date_from_text("2026/2/30"), None);
    }

    #[test]
    fn reads_only_annual_leave_detail_sheet() {
        let temp_dir = std::env::temp_dir();
        let detail_path = temp_dir.join(format!(
            "flt-attendance-annual-detail-{}.xlsx",
            std::process::id()
        ));
        let legacy_path = temp_dir.join(format!(
            "flt-attendance-annual-legacy-{}.xlsx",
            std::process::id()
        ));

        write_annual_leave_workbook(&detail_path, "年假明细");
        let mut detail_workbook = open_workbook_auto(&detail_path).unwrap();
        let records = parse_annual_leave_records(&mut detail_workbook).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].employee_no, "24154");
        assert_eq!(records[0].name, "秦红");
        assert_eq!(records[0].balance_before_month_hours, 10.5);

        write_annual_leave_workbook(&legacy_path, "年假信息");
        let mut legacy_workbook = open_workbook_auto(&legacy_path).unwrap();
        assert!(
            parse_annual_leave_records(&mut legacy_workbook)
                .unwrap()
                .is_empty()
        );

        std::fs::remove_file(detail_path).unwrap();
        std::fs::remove_file(legacy_path).unwrap();
    }

    fn write_annual_leave_workbook(path: &Path, sheet_name: &str) {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name(sheet_name).unwrap();
        sheet.write_string(0, 0, "年假信息").unwrap();
        sheet.write_string(1, 0, "统计截止当前月").unwrap();
        for (column, header) in [
            "工号",
            "姓名",
            "公司",
            "本年总额",
            "已使用",
            "冻结",
            "待生效",
            "其他",
            "调整",
            "截止当前月剩余（小时）",
        ]
        .into_iter()
        .enumerate()
        {
            sheet.write_string(2, column as u16, header).unwrap();
        }
        sheet.write_string(3, 0, "24154").unwrap();
        sheet.write_string(3, 1, "秦红").unwrap();
        sheet.write_string(3, 2, "江苏福拉特").unwrap();
        sheet.write_number(3, 9, 10.5).unwrap();
        workbook.save(path).unwrap();
    }
}
