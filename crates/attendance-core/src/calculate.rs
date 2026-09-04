use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::config::AttendanceConfig;
use crate::holiday;
use crate::model::{
    AnnualLeaveRecord, AttendanceDataset, CalendarDate, DailyRecord, EmploymentRecord, PunchKind,
};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AttendanceReport {
    pub detail_rows: Vec<DetailRow>,
    pub summary_rows: Vec<SummaryRow>,
    pub exception_rows: Vec<ExceptionRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetailRow {
    pub employee_no: String,
    pub name: String,
    pub company: String,
    pub company_from_employment: bool,
    pub works_saturdays: bool,
    pub days: Vec<DailyAttendance>,
    pub summary: SummaryRow,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyAttendance {
    pub attendance: String,
    pub overtime_hours: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SummaryRow {
    pub employee_no: String,
    pub name: String,
    pub attendance_meal_count: f64,
    pub overtime_meal_count: f64,
    pub meal_allowance_count: Option<f64>,
    pub travel_days: f64,
    pub weekday_overtime_hours: f64,
    pub weekend_overtime_hours: f64,
    pub holiday_overtime_hours: f64,
    pub expected_attendance_hours: f64,
    pub actual_attendance_hours: Option<f64>,
    pub annual_leave_hours: f64,
    pub annual_leave_balance_hours: Option<f64>,
    pub sick_leave_hours: f64,
    pub personal_leave_hours: f64,
    pub breastfeeding_leave_hours: f64,
    pub marriage_leave_hours: f64,
    pub maternity_leave_hours: f64,
    pub bereavement_leave_hours: f64,
    pub childcare_leave_hours: f64,
    pub absent_hours: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExceptionRow {
    pub employee_no: String,
    pub name: String,
    pub missing_in: u32,
    pub missing_out: u32,
    pub late_or_early_under_10: u32,
    pub late_or_early_11_to_30: u32,
    pub late_or_early_30_to_120_minutes: u32,
    pub out_of_range: u32,
    pub score: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AnnualLeaveMatch {
    Matched(f64),
    Missing,
    Ambiguous,
}

pub fn calculate_attendance(dataset: &AttendanceDataset) -> AttendanceReport {
    calculate_attendance_with_config(dataset, &AttendanceConfig::default())
}

fn payable_overtime_hours(hours: f64) -> f64 {
    if hours < 1.0 {
        0.0
    } else {
        (hours * 2.0).floor() / 2.0
    }
}

fn cap_overtime_by_effective_time(
    hours: f64,
    daily: &DailyRecord,
    applications: Option<&[(u32, u32)]>,
) -> f64 {
    if hours <= 0.0 {
        return hours;
    }
    let (Some(applications), Some((first_in, last_out))) = (applications, punch_range(daily))
    else {
        return hours;
    };
    let mut minutes = 0;
    for &(start, end) in applications {
        let lower = start.max(first_in);
        let upper = end.min(last_out);
        let mut effective = upper.saturating_sub(lower);
        for day in lower / 1440..=upper / 1440 {
            let offset = day * 1440;
            // Subtract breaks from the time-based cap, never from declared hours.
            effective = effective.saturating_sub(
                upper
                    .min(offset + 13 * 60)
                    .saturating_sub(lower.max(offset + 12 * 60)),
            );
            if start <= offset + 17 * 60 + 30 && last_out >= offset + 19 * 60 + 30 {
                effective = effective.saturating_sub(
                    upper
                        .min(offset + 18 * 60)
                        .saturating_sub(lower.max(offset + 17 * 60 + 30)),
                );
            }
        }
        minutes += effective;
    }
    hours.min(minutes as f64 / 60.0)
}

fn normalize_overtime_categories(mut hours: [f64; 3], total: f64) -> (f64, f64, f64) {
    let difference = total - hours.iter().sum::<f64>();
    if difference > 0.001 {
        let largest = hours
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(index, _)| index)
            .unwrap_or(0);
        hours[largest] += difference;
    } else if difference < -0.001 {
        let mut excess = -difference;
        while excess > 0.001 {
            let largest = hours
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.total_cmp(right))
                .map(|(index, _)| index)
                .unwrap_or(0);
            let reduction = hours[largest].min(excess);
            hours[largest] -= reduction;
            excess -= reduction;
        }
    }

    let mut normalized = hours.map(|value| (value * 2.0).floor() / 2.0);
    let mut remaining_units = ((total - normalized.iter().sum::<f64>()) * 2.0)
        .round()
        .max(0.0) as usize;
    let mut remainders = [
        hours[0] - normalized[0],
        hours[1] - normalized[1],
        hours[2] - normalized[2],
    ];
    while remaining_units > 0 {
        let largest = remainders
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(index, _)| index)
            .unwrap_or(0);
        normalized[largest] += 0.5;
        remainders[largest] = -1.0;
        remaining_units -= 1;
    }
    (normalized[0], normalized[1], normalized[2])
}

fn overtime_summary(
    days: &[DailyAttendance],
    year: u16,
    month: u8,
    config: &AttendanceConfig,
) -> (f64, f64, f64) {
    let mut weekday = 0.0;
    let mut weekend = 0.0;
    let mut holiday = 0.0;
    for (index, day) in days.iter().enumerate() {
        let calendar_day = (index + 1) as u8;
        if config.is_statutory_holiday(year, month, calendar_day) {
            holiday += day.overtime_hours;
        } else if crate::holiday::is_workday(year, month, calendar_day) {
            weekday += day.overtime_hours;
        } else {
            weekend += day.overtime_hours;
        }
    }
    (weekday, weekend, holiday)
}

fn normalized_employee_name(name: &str) -> &str {
    name.trim()
        .strip_suffix("（离职）")
        .or_else(|| name.trim().strip_suffix("(离职)"))
        .unwrap_or_else(|| name.trim())
}

fn annual_leave_balance_before_month(
    records: &[AnnualLeaveRecord],
    employee_no: &str,
    name: &str,
    company: &str,
) -> AnnualLeaveMatch {
    if !employee_no.trim().is_empty() {
        let employee_matches = records
            .iter()
            .filter(|record| {
                !record.employee_no.trim().is_empty()
                    && record.employee_no.trim() == employee_no.trim()
            })
            .collect::<Vec<_>>();
        match employee_matches.as_slice() {
            [record] => {
                return AnnualLeaveMatch::Matched(record.balance_before_month_hours);
            }
            [_, _, ..] => return AnnualLeaveMatch::Ambiguous,
            [] => {}
        }
    }

    let normalized_name = normalized_employee_name(name);
    let name_matches = records
        .iter()
        .filter(|record| normalized_employee_name(&record.name) == normalized_name)
        .collect::<Vec<_>>();
    match name_matches.as_slice() {
        [] => AnnualLeaveMatch::Missing,
        [record] => AnnualLeaveMatch::Matched(record.balance_before_month_hours),
        candidates => {
            let company_matches = candidates
                .iter()
                .copied()
                .filter(|record| {
                    !company.trim().is_empty() && record.company.trim() == company.trim()
                })
                .collect::<Vec<_>>();
            match company_matches.as_slice() {
                [record] => AnnualLeaveMatch::Matched(record.balance_before_month_hours),
                _ => AnnualLeaveMatch::Ambiguous,
            }
        }
    }
}

pub fn calculate_attendance_with_config(
    dataset: &AttendanceDataset,
    config: &AttendanceConfig,
) -> AttendanceReport {
    let mut daily_by_employee: HashMap<&str, Vec<&DailyRecord>> = HashMap::new();
    for daily in &dataset.daily {
        daily_by_employee
            .entry(&daily.employee_key)
            .or_default()
            .push(daily);
    }

    let mut invalid_by_employee: HashMap<&str, Vec<&crate::model::InvalidPunch>> = HashMap::new();
    for invalid in &dataset.invalid_punches {
        invalid_by_employee
            .entry(&invalid.employee_key)
            .or_default()
            .push(invalid);
    }

    let mut detail_rows = Vec::with_capacity(dataset.monthly.len());
    let mut summary_rows = Vec::with_capacity(dataset.monthly.len());
    let mut exception_rows = Vec::new();
    let mut unclassified_late_events = 0_u32;
    let mut annual_leave_missing_employees = 0_usize;
    let mut annual_leave_ambiguous_employees = 0_usize;
    let mut leave_fallback_categories = BTreeMap::<&'static str, usize>::new();
    let has_statutory_holiday_override = config.has_statutory_holiday_override(dataset.period.year);
    let mut statutory_holiday_override_missing_daily_data = false;

    for monthly in dataset
        .monthly
        .iter()
        .filter(|row| !config.excludes_employee(&row.employee_no, &row.name))
    {
        let excludes_overtime = config
            .special_personnel
            .excludes_overtime(&monthly.employee_no, &monthly.name);
        let keeps_only_statutory_holiday_overtime = config
            .special_personnel
            .keeps_only_statutory_holiday_overtime(&monthly.employee_no, &monthly.name);
        let uses_flexible_arrival_shift = config
            .special_personnel
            .uses_flexible_arrival_shift(&monthly.employee_no, &monthly.name);
        let six_day_daily_hours = config
            .special_personnel
            .six_day_daily_hours(&monthly.employee_no, &monthly.name);
        let uses_six_day_schedule = six_day_daily_hours.is_some();
        let meal_policy = meal_policy(config, &monthly.employee_no, &monthly.name);
        let daily_records = daily_by_employee
            .get(monthly.employee_key.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let all_invalid_punches = invalid_by_employee
            .get(monthly.employee_key.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let employment = employment_record(
            &dataset.employment_records,
            &monthly.employee_no,
            &monthly.name,
        );
        let employment_company = employment.filter(|record| !record.company.trim().is_empty());
        let company = employment_company
            .map(|record| record.company.clone())
            .unwrap_or_else(|| company_from_attendance_group(&monthly.attendance_group));
        let active_daily_records = daily_records
            .iter()
            .copied()
            .filter(|daily| {
                day_from_date(&daily.date).is_some_and(|day| {
                    is_active_day(employment, dataset.period.year, dataset.period.month, day)
                })
            })
            .collect::<Vec<_>>();
        let active_invalid_punches = all_invalid_punches
            .iter()
            .copied()
            .filter(|invalid| {
                day_from_date(&invalid.attendance_date).is_some_and(|day| {
                    is_active_day(employment, dataset.period.year, dataset.period.month, day)
                })
            })
            .collect::<Vec<_>>();

        let expected_days = if uses_six_day_schedule {
            (1..=days_in_month(dataset.period.year, dataset.period.month))
                .filter(|day| {
                    is_active_day(employment, dataset.period.year, dataset.period.month, *day)
                        && holiday::is_six_day_workday(
                            dataset.period.year,
                            dataset.period.month,
                            *day as u8,
                        )
                })
                .count() as f64
        } else if meal_policy == MealPolicy::ScheduledWithoutPunch {
            calendar_workdays_in_employment(dataset.period.year, dataset.period.month, employment)
                as f64
        } else {
            active_daily_records
                .iter()
                .filter(|daily| is_scheduled_shift(&daily.shift))
                .count() as f64
        };
        let standard_daily_hours = six_day_daily_hours.unwrap_or(8.0);
        let travel_days = monthly
            .daily_results
            .iter()
            .enumerate()
            .filter(|(index, result)| {
                result.contains("出差")
                    && is_active_day(
                        employment,
                        dataset.period.year,
                        dataset.period.month,
                        index + 1,
                    )
            })
            .count() as f64;
        let process_hours = |process_name| {
            unique_process_amount_for_month(
                &monthly.daily_results,
                process_name,
                daily_records,
                dataset.period.year,
                dataset.period.month,
                standard_daily_hours,
                uses_six_day_schedule,
                employment,
            )
        };
        let childcare_leave_hours = process_hours("育儿假");
        let prenatal_leave_hours = process_hours("产检假");
        let annual_leave_hours = process_hours("年假");
        let absent_days: f64 = active_daily_records
            .iter()
            .map(|daily| daily.absent_days)
            .sum();
        let personal_process_hours = process_hours("事假");
        let compensatory_process_hours = process_hours("调休");
        let sick_process_hours = process_hours("病假");
        let breastfeeding_process_hours = process_hours("哺乳假");
        let marriage_process_hours = process_hours("婚假");
        let maternity_process_hours = process_hours("产假");
        let paternity_process_hours = process_hours("陪产假");
        let bereavement_process_hours = process_hours("丧假");
        let menstrual_process_hours = process_hours("例假");
        let personal_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.personal,
            monthly.personal_leave_hours,
            personal_process_hours,
        );
        let compensatory_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.compensatory,
            monthly.compensatory_leave_hours,
            compensatory_process_hours,
        );
        let sick_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.sick,
            monthly.sick_leave_hours,
            sick_process_hours,
        );
        let breastfeeding_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.breastfeeding,
            monthly.breastfeeding_leave_hours,
            breastfeeding_process_hours,
        );
        let marriage_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.marriage,
            monthly.marriage_leave_days * standard_daily_hours,
            marriage_process_hours,
        );
        let maternity_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.maternity,
            monthly.maternity_leave_days * standard_daily_hours,
            maternity_process_hours,
        ) + summary_or_process_hours(
            monthly.leave_summary_present.paternity,
            monthly.paternity_leave_days * standard_daily_hours,
            paternity_process_hours,
        );
        let bereavement_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.bereavement,
            monthly.bereavement_leave_days * standard_daily_hours,
            bereavement_process_hours,
        );
        let menstrual_leave_hours = summary_or_process_hours(
            monthly.leave_summary_present.menstrual,
            monthly.menstrual_leave_days * standard_daily_hours,
            menstrual_process_hours,
        );
        for (label, used_fallback) in [
            (
                "事假",
                !monthly.leave_summary_present.personal && personal_process_hours > 0.001,
            ),
            (
                "调休",
                !monthly.leave_summary_present.compensatory && compensatory_process_hours > 0.001,
            ),
            (
                "病假",
                !monthly.leave_summary_present.sick && sick_process_hours > 0.001,
            ),
            (
                "哺乳假",
                !monthly.leave_summary_present.breastfeeding && breastfeeding_process_hours > 0.001,
            ),
            (
                "婚假",
                !monthly.leave_summary_present.marriage && marriage_process_hours > 0.001,
            ),
            (
                "产假/陪产假",
                (!monthly.leave_summary_present.maternity && maternity_process_hours > 0.001)
                    || (!monthly.leave_summary_present.paternity
                        && paternity_process_hours > 0.001),
            ),
            (
                "丧假",
                !monthly.leave_summary_present.bereavement && bereavement_process_hours > 0.001,
            ),
            (
                "例假",
                !monthly.leave_summary_present.menstrual && menstrual_process_hours > 0.001,
            ),
        ] {
            if used_fallback {
                *leave_fallback_categories.entry(label).or_default() += 1;
            }
        }
        let absent_hours = absent_days * standard_daily_hours;
        let leave_hours = personal_leave_hours
            + compensatory_leave_hours
            + sick_leave_hours
            + annual_leave_hours
            + breastfeeding_leave_hours
            + marriage_leave_hours
            + maternity_leave_hours
            + bereavement_leave_hours
            + menstrual_leave_hours
            + childcare_leave_hours
            + prenatal_leave_hours;
        let expected_attendance_hours = expected_days * standard_daily_hours;
        let normal_attendance_hours =
            (expected_attendance_hours - leave_hours - absent_hours).max(0.0);
        let (attendance_meal_count, overtime_meal_count) = calculate_meal_allowance(
            monthly,
            &active_daily_records,
            dataset.period.year,
            dataset.period.month,
            meal_policy,
            uses_flexible_arrival_shift,
            employment,
        );
        let mut days = build_daily_attendance(
            monthly,
            daily_records,
            dataset.period.year,
            dataset.period.month,
            uses_flexible_arrival_shift,
            standard_daily_hours,
            uses_six_day_schedule,
            employment,
        );
        let has_daily_overtime = days.iter().any(|day| day.overtime_hours > 0.0)
            || active_daily_records
                .iter()
                .any(|daily| daily.overtime_hours > 0.0);
        let raw_daily_overtime =
            overtime_summary(&days, dataset.period.year, dataset.period.month, config);
        for (index, day) in days.iter_mut().enumerate() {
            day.overtime_hours = if excludes_overtime {
                0.0
            } else if keeps_only_statutory_holiday_overtime
                && !config.is_statutory_holiday(
                    dataset.period.year,
                    dataset.period.month,
                    (index + 1) as u8,
                )
            {
                0.0
            } else {
                payable_overtime_hours(day.overtime_hours)
            };
        }
        let overtime_categories = if excludes_overtime {
            (0.0, 0.0, 0.0)
        } else if has_daily_overtime {
            let payable_daily =
                overtime_summary(&days, dataset.period.year, dataset.period.month, config);
            let raw_daily_total =
                raw_daily_overtime.0 + raw_daily_overtime.1 + raw_daily_overtime.2;
            let monthly_total = monthly.weekday_overtime_hours
                + monthly.weekend_overtime_hours
                + monthly.holiday_overtime_hours;
            if has_statutory_holiday_override {
                payable_daily
            } else if (raw_daily_total - monthly_total).abs() < 0.001 {
                normalize_overtime_categories(
                    [
                        (monthly.weekday_overtime_hours - (raw_daily_overtime.0 - payable_daily.0))
                            .max(0.0),
                        (monthly.weekend_overtime_hours - (raw_daily_overtime.1 - payable_daily.1))
                            .max(0.0),
                        (monthly.holiday_overtime_hours - (raw_daily_overtime.2 - payable_daily.2))
                            .max(0.0),
                    ],
                    payable_daily.0 + payable_daily.1 + payable_daily.2,
                )
            } else {
                payable_daily
            }
        } else {
            if has_statutory_holiday_override
                && monthly.weekday_overtime_hours
                    + monthly.weekend_overtime_hours
                    + monthly.holiday_overtime_hours
                    > 0.001
            {
                statutory_holiday_override_missing_daily_data = true;
            }
            (
                payable_overtime_hours(monthly.weekday_overtime_hours),
                payable_overtime_hours(monthly.weekend_overtime_hours),
                payable_overtime_hours(monthly.holiday_overtime_hours),
            )
        };
        let (weekday_overtime_hours, weekend_overtime_hours, holiday_overtime_hours) =
            if keeps_only_statutory_holiday_overtime {
                (0.0, 0.0, overtime_categories.2)
            } else {
                overtime_categories
            };
        let overtime_hours =
            weekday_overtime_hours + weekend_overtime_hours + holiday_overtime_hours;
        let annual_leave_balance_hours = match annual_leave_balance_before_month(
            &dataset.annual_leave_records,
            &monthly.employee_no,
            &monthly.name,
            &company,
        ) {
            AnnualLeaveMatch::Matched(balance_before_month) => {
                let balance = balance_before_month - annual_leave_hours;
                Some(if balance.abs() < 0.001 { 0.0 } else { balance })
            }
            AnnualLeaveMatch::Missing => {
                annual_leave_missing_employees += 1;
                None
            }
            AnnualLeaveMatch::Ambiguous => {
                annual_leave_ambiguous_employees += 1;
                None
            }
        };
        let summary = SummaryRow {
            employee_no: monthly.employee_no.clone(),
            name: monthly.name.clone(),
            attendance_meal_count,
            overtime_meal_count,
            meal_allowance_count: Some(attendance_meal_count + overtime_meal_count),
            travel_days,
            weekday_overtime_hours,
            weekend_overtime_hours,
            holiday_overtime_hours,
            expected_attendance_hours,
            actual_attendance_hours: Some(normal_attendance_hours + overtime_hours),
            annual_leave_hours,
            annual_leave_balance_hours,
            sick_leave_hours,
            personal_leave_hours,
            breastfeeding_leave_hours,
            marriage_leave_hours,
            maternity_leave_hours,
            bereavement_leave_hours,
            childcare_leave_hours,
            absent_hours,
        };
        detail_rows.push(DetailRow {
            employee_no: monthly.employee_no.clone(),
            name: monthly.name.clone(),
            company,
            company_from_employment: employment_company.is_some(),
            works_saturdays: uses_six_day_schedule,
            days,
            summary: summary.clone(),
        });
        summary_rows.push(summary);

        let (exception, unclassified) = calculate_exceptions(
            &monthly.employee_no,
            &monthly.name,
            &monthly.department,
            &active_daily_records,
            &active_invalid_punches,
            &monthly.daily_results,
            ExceptionOptions {
                uses_flexible_arrival_shift,
                hire_day: employment
                    .and_then(|record| record.hire_date)
                    .filter(|date| {
                        date.year == dataset.period.year && date.month == dataset.period.month
                    })
                    .map(|date| usize::from(date.day)),
            },
        );
        unclassified_late_events += unclassified;
        if let Some(exception) = exception {
            exception_rows.push(exception);
        }
    }

    let mut warnings = vec![
        "本应出勤：一般人员按有效班次日 × 8 小时；不打卡人员按法定工作日 × 8 小时；六天制人员按周一至周六及配置的每日小时数计算。"
            .to_owned(),
        "出差天数：按月度汇总中的出差日期计。".to_owned(),
        "不在范围内打卡：按原始记录中的“当前不在可打卡的时间范围”计。".to_owned(),
    ];
    if !leave_fallback_categories.is_empty() {
        let recovered = leave_fallback_categories
            .iter()
            .map(|(label, employees)| format!("{label}{employees}人"))
            .collect::<Vec<_>>()
            .join("、");
        warnings.insert(
            0,
            format!("月度请假汇总为空，已从逐日审批流程回退统计：{recovered}。"),
        );
    }
    if dataset.annual_leave_records.is_empty() {
        warnings.insert(
            0,
            "年假剩余未计算：未读取到“年假明细”工作表或其中没有有效余额。".to_owned(),
        );
    } else {
        if annual_leave_ambiguous_employees > 0 {
            warnings.insert(
                0,
                format!(
                    "年假余额匹配不唯一 {annual_leave_ambiguous_employees} 人：请在“年假明细”中补充工号，结果留空。"
                ),
            );
        }
        if annual_leave_missing_employees > 0 {
            warnings.insert(
                0,
                format!(
                    "年假余额未匹配 {annual_leave_missing_employees} 人：在“年假明细”中未找到对应工号或姓名，结果留空。"
                ),
            );
        }
    }
    if dataset.employment_records.is_empty() {
        warnings.push(
            "未读取到入职名单/离职名单：无法按在职区间裁剪考勤，新员工入职当天餐补及上班卡异常豁免未应用。"
                .to_owned(),
        );
    } else {
        let hires = dataset
            .employment_records
            .iter()
            .filter(|record| record.hire_date.is_some())
            .count();
        let terminations = dataset
            .employment_records
            .iter()
            .filter(|record| record.termination_date.is_some())
            .count();
        warnings.push(format!(
            "已读取入离职信息：{hires} 条入职、{terminations} 条离职；已按在职区间计算，并应用入职当天餐补及上班卡异常豁免。"
        ));
    }
    if unclassified_late_events > 0 {
        warnings.push(format!(
            "有 {unclassified_late_events} 条迟到/早退记录无法从班次和打卡时间计算分钟数，未计入绩效分档。"
        ));
    }
    if !holiday::has_calendar(dataset.period.year) {
        warnings.push(format!(
            "未内置 {} 年法定节假日数据：暂按普通工作日和周末判断。",
            dataset.period.year
        ));
    }
    if has_statutory_holiday_override {
        warnings.push(format!(
            "已按设置中的 {} 个三倍工资日重新划分 {} 年周末与法定加班。",
            config.statutory_holiday_count(dataset.period.year),
            dataset.period.year
        ));
    }
    if statutory_holiday_override_missing_daily_data {
        warnings.push(
            "部分人员缺少每日加班数据，无法按三倍工资日重新划分；相关人员暂沿用钉钉月度加班分类。"
                .to_owned(),
        );
    }
    let matched_special_people = config.special_personnel.matched_count(
        dataset
            .monthly
            .iter()
            .filter(|row| !config.excludes_employee(&row.employee_no, &row.name))
            .map(|row| (row.employee_no.as_str(), row.name.as_str())),
    );
    if matched_special_people > 0 {
        warnings.push(format!(
            "已按特殊人员配置排除 {matched_special_people} 人的加班时长。"
        ));
    }
    let statutory_holiday_overtime_only_people = config
        .special_personnel
        .statutory_holiday_overtime_only_matched_count(
            dataset
                .monthly
                .iter()
                .filter(|row| !config.excludes_employee(&row.employee_no, &row.name))
                .map(|row| (row.employee_no.as_str(), row.name.as_str())),
        );
    if statutory_holiday_overtime_only_people > 0 {
        warnings.push(format!(
            "已对 {statutory_holiday_overtime_only_people} 人清零工作日和周末加班时长，保留法定节假日加班；餐补按实际打卡计算。"
        ));
    }
    let flexible_arrival_people = config.special_personnel.flexible_arrival_matched_count(
        dataset
            .monthly
            .iter()
            .filter(|row| !config.excludes_employee(&row.employee_no, &row.name))
            .map(|row| (row.employee_no.as_str(), row.name.as_str())),
    );
    if flexible_arrival_people > 0 {
        warnings.push(format!(
            "已对 {flexible_arrival_people} 人应用 08:30 到岗分界的弹性下班规则。"
        ));
    }
    let six_day_people = config.special_personnel.six_day_matched_count(
        dataset
            .monthly
            .iter()
            .filter(|row| !config.excludes_employee(&row.employee_no, &row.name))
            .map(|row| (row.employee_no.as_str(), row.name.as_str())),
    );
    if six_day_people > 0 {
        warnings.push(format!(
            "已对 {six_day_people} 人应用周一至周六工作制；所有加班和餐补均固定为 0。"
        ));
    }
    let excluded_people = config.excluded_count(
        dataset
            .monthly
            .iter()
            .map(|row| (row.employee_no.as_str(), row.name.as_str())),
    );
    if excluded_people > 0 {
        warnings.push(format!("已按不参与考勤配置忽略 {excluded_people} 人。"));
    }

    AttendanceReport {
        detail_rows,
        summary_rows,
        exception_rows,
        warnings,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MealPolicy {
    Regular,
    PunchOnly,
    ScheduledWithoutPunch,
    None,
}

fn meal_policy(config: &AttendanceConfig, employee_no: &str, name: &str) -> MealPolicy {
    if config.special_personnel.excludes_meal(employee_no, name) {
        MealPolicy::None
    } else if config
        .special_personnel
        .uses_scheduled_meal_without_punch(employee_no, name)
    {
        MealPolicy::ScheduledWithoutPunch
    } else if config
        .special_personnel
        .uses_punch_only_meal(employee_no, name)
    {
        MealPolicy::PunchOnly
    } else {
        MealPolicy::Regular
    }
}

fn calculate_meal_allowance(
    monthly: &crate::model::MonthlyRecord,
    daily_records: &[&DailyRecord],
    year: u16,
    month: u8,
    policy: MealPolicy,
    uses_flexible_arrival_shift: bool,
    employment: Option<&EmploymentRecord>,
) -> (f64, f64) {
    if policy == MealPolicy::None {
        return (0.0, 0.0);
    }
    if policy == MealPolicy::ScheduledWithoutPunch {
        let workdays = calendar_workdays_in_employment(year, month, employment);
        return (workdays as f64, 0.0);
    }

    let hire_day_meal = employment
        .and_then(|record| record.hire_date)
        .filter(|date| date.year == year && date.month == month)
        .is_some_and(|date| {
            policy == MealPolicy::Regular
                && holiday::is_workday(year, month, date.day)
                && monthly
                    .daily_results
                    .get(date.day as usize - 1)
                    .is_none_or(|result| !result.contains("出差") && !result.contains("居家办公"))
        });
    let mut attendance_meals = u32::from(hire_day_meal);
    let mut overtime_meals = 0_u32;
    for daily in daily_records {
        let Some(day) = day_from_date(&daily.date) else {
            continue;
        };
        if day == 0 || day > days_in_month(year, month) {
            continue;
        }
        let result = monthly
            .daily_results
            .get(day - 1)
            .map(String::as_str)
            .unwrap_or("");
        let is_workday = holiday::is_workday(year, month, day as u8);

        let punches = punch_range(daily);
        if is_workday {
            if !is_hire_day(employment, year, month, day)
                && workday_attendance_meal(result, punches)
            {
                attendance_meals += 1;
            }
            overtime_meals += workday_overtime_meals(
                daily,
                punches,
                overtime_application_ranges(result, year, month, day as u8).as_deref(),
                policy == MealPolicy::PunchOnly,
                uses_flexible_arrival_shift,
            );
        } else {
            overtime_meals += off_day_overtime_meals(
                punches,
                daily.overtime_hours,
                policy == MealPolicy::PunchOnly,
            );
        }
    }
    (attendance_meals as f64, overtime_meals as f64)
}

fn workday_attendance_meal(result: &str, punches: Option<(u32, u32)>) -> bool {
    !result.contains("出差")
        && !result.contains("居家办公")
        && punches.is_some_and(|(first_in, _)| first_in <= 10 * 60)
}

fn punch_range(daily: &DailyRecord) -> Option<(u32, u32)> {
    let first_in = daily
        .punch_slots
        .iter()
        .filter(|slot| slot.kind == PunchKind::In)
        .filter_map(|slot| parse_punch_minutes(&slot.time))
        .min()?;
    let last_out = last_out_minutes(daily)?;
    (last_out >= first_in).then_some((first_in, last_out))
}

fn last_out_minutes(daily: &DailyRecord) -> Option<u32> {
    daily
        .punch_slots
        .iter()
        .filter(|slot| slot.kind == PunchKind::Out)
        .filter_map(|slot| parse_punch_minutes(&slot.time))
        .max()
}

fn expected_workday_end(daily: &DailyRecord, uses_flexible_arrival_shift: bool) -> Option<u32> {
    if uses_flexible_arrival_shift {
        let arrival = punch_range(daily)?.0;
        return Some(if arrival <= 8 * 60 + 30 {
            17 * 60 + 30
        } else {
            18 * 60
        });
    }
    let times = extract_clock_minutes(&daily.shift);
    let start = times.first().copied()?;
    let end = times.get(1).copied()?;
    Some(if end < start { end + 24 * 60 } else { end })
}

fn workday_overtime_meals(
    daily: &DailyRecord,
    punches: Option<(u32, u32)>,
    applications: Option<&[(u32, u32)]>,
    punch_only: bool,
    uses_flexible_arrival_shift: bool,
) -> u32 {
    let Some((first_in, last_out)) = punches else {
        return 0;
    };
    let effective_end = if punch_only {
        last_out
    } else {
        if daily.overtime_hours <= 0.0 {
            return 0;
        }
        if let Some(applications) = applications {
            return [19 * 60 + 30, 24 * 60]
                .into_iter()
                .filter(|&threshold| {
                    applications.iter().any(|&(start, end)| {
                        start.max(first_in) < threshold && end.min(last_out) >= threshold
                    })
                })
                .count() as u32;
        }
        let Some(expected_end) = expected_workday_end(daily, uses_flexible_arrival_shift) else {
            return 0;
        };
        let start = expected_end;
        let actual_minutes = last_out.saturating_sub(start);
        if actual_minutes == 0 {
            return 0;
        }
        let approved_minutes = (daily.overtime_hours * 60.0).round().max(0.0) as u32;
        start + actual_minutes.min(approved_minutes)
    };
    u32::from(effective_end >= 19 * 60 + 30) + u32::from(effective_end >= 24 * 60)
}

fn off_day_overtime_meals(
    punches: Option<(u32, u32)>,
    approved_hours: f64,
    punch_only: bool,
) -> u32 {
    let Some((first_in, last_out)) = punches else {
        return 0;
    };
    if !punch_only && approved_hours <= 0.0 {
        return 0;
    }

    let segments = [
        (0, 12 * 60),
        (13 * 60, 19 * 60 + 30),
        (19 * 60 + 30, 24 * 60),
    ];
    let mut remaining = if punch_only {
        u32::MAX
    } else {
        (approved_hours * 60.0).round().max(0.0) as u32
    };
    let mut meals = 0;
    for (start, end) in segments {
        let actual = last_out.min(end).saturating_sub(first_in.max(start));
        let effective = actual.min(remaining);
        if effective >= 2 * 60 && (end != 19 * 60 + 30 || last_out >= end) {
            meals += 1;
        }
        remaining = remaining.saturating_sub(effective);
    }
    meals
}

pub fn apply_company_history(
    report: &mut AttendanceReport,
    companies: &HashMap<String, String>,
) -> usize {
    let mut applied = 0;
    for row in &mut report.detail_rows {
        if !row.company_from_employment
            && let Some(company) = companies.get(&row.name)
        {
            row.company.clone_from(company);
            applied += 1;
        }
    }
    applied
}

fn company_from_attendance_group(group: &str) -> String {
    if group.contains("滁州") {
        "滁州".to_owned()
    } else if group.contains("昆山") {
        "KSFAE".to_owned()
    } else {
        "JSFAE".to_owned()
    }
}

fn employment_record<'a>(
    records: &'a [EmploymentRecord],
    employee_no: &str,
    name: &str,
) -> Option<&'a EmploymentRecord> {
    if !employee_no.trim().is_empty() {
        records
            .iter()
            .find(|record| record.employee_no.trim() == employee_no.trim())
    } else {
        records.iter().find(|record| {
            record.employee_no.trim().is_empty() && record.name.trim() == name.trim()
        })
    }
}

fn is_active_day(employment: Option<&EmploymentRecord>, year: u16, month: u8, day: usize) -> bool {
    let Ok(day) = u8::try_from(day) else {
        return false;
    };
    let date = CalendarDate { year, month, day };
    employment.is_none_or(|record| {
        record.hire_date.is_none_or(|hire_date| date >= hire_date)
            && record
                .termination_date
                .is_none_or(|termination_date| date <= termination_date)
    })
}

fn calendar_workdays_in_employment(
    year: u16,
    month: u8,
    employment: Option<&EmploymentRecord>,
) -> usize {
    (1..=days_in_month(year, month))
        .filter(|day| {
            holiday::is_workday(year, month, *day as u8)
                && is_active_day(employment, year, month, *day)
        })
        .count()
}

fn is_hire_day(employment: Option<&EmploymentRecord>, year: u16, month: u8, day: usize) -> bool {
    let Ok(day) = u8::try_from(day) else {
        return false;
    };
    employment.and_then(|record| record.hire_date) == Some(CalendarDate { year, month, day })
}

fn build_daily_attendance(
    monthly: &crate::model::MonthlyRecord,
    daily_records: &[&DailyRecord],
    year: u16,
    month: u8,
    uses_flexible_arrival_shift: bool,
    standard_daily_hours: f64,
    uses_six_day_schedule: bool,
    employment: Option<&EmploymentRecord>,
) -> Vec<DailyAttendance> {
    let day_count = days_in_month(year, month);
    let mut daily_by_day = HashMap::new();
    for daily in daily_records {
        if let Some(day) = day_from_date(&daily.date) {
            daily_by_day.insert(day, *daily);
        }
    }

    let mut cells = Vec::with_capacity(day_count);
    let mut leave_processes: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for day_index in 0..day_count {
        if !is_active_day(employment, year, month, day_index + 1) {
            cells.push(DailyAttendance {
                attendance: String::new(),
                overtime_hours: 0.0,
            });
            continue;
        }
        let result = monthly
            .daily_results
            .get(day_index)
            .map(String::as_str)
            .unwrap_or("");
        let daily = daily_by_day.get(&(day_index + 1)).copied();
        let mut attendance = if result.contains("休息") || result.contains("未排班") {
            "☆".to_owned()
        } else if result.is_empty() && daily.is_none_or(|row| !is_scheduled_shift(&row.shift)) {
            String::new()
        } else {
            "√".to_owned()
        };
        let mut overtime_hours = daily.map(|row| row.overtime_hours).unwrap_or(0.0);
        for part in result
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            if part.starts_with("加班") {
                if daily.is_none() {
                    overtime_hours += extract_amount_before(part, "小时").unwrap_or(0.0);
                }
            } else if part.starts_with("出差") {
                attendance = "C".to_owned();
            } else if leave_symbol(part).is_some() {
                leave_processes
                    .entry(part.to_owned())
                    .or_default()
                    .push(day_index);
            }
        }
        if let Some(daily) = daily {
            overtime_hours = cap_overtime_by_effective_time(
                overtime_hours,
                daily,
                overtime_application_ranges(result, year, month, (day_index + 1) as u8).as_deref(),
            );
        }

        if result.contains("旷工") {
            let hours = daily.map(|row| row.absent_days * 8.0).unwrap_or(8.0);
            attendance = format!("X{}", format_hours(hours));
        } else if let Some(daily) = daily {
            let irregular = daily_irregular_mark(
                daily,
                result,
                uses_flexible_arrival_shift,
                is_hire_day(employment, year, month, day_index + 1),
            );
            if !irregular.is_empty() {
                attendance.push_str(&irregular);
            }
        }
        cells.push(DailyAttendance {
            attendance,
            overtime_hours,
        });
    }

    let mut leave_hours_by_day = vec![BTreeMap::<&'static str, f64>::new(); day_count];
    for (process, indexes) in leave_processes {
        let Some(symbol) = leave_symbol(&process) else {
            continue;
        };
        for (index, amount) in process_allocations_for_month(
            &process,
            &indexes,
            daily_records,
            year,
            month,
            standard_daily_hours,
            uses_six_day_schedule,
            employment,
        ) {
            *leave_hours_by_day[index].entry(symbol).or_default() += amount;
        }
    }
    for (index, leave_hours) in leave_hours_by_day.into_iter().enumerate() {
        if !leave_hours.is_empty() {
            let prefix = if cells[index].attendance == "C" {
                "C"
            } else {
                ""
            };
            let marks = leave_hours
                .into_iter()
                .map(|(symbol, amount)| format!("{symbol}{}", format_hours(amount)))
                .collect::<String>();
            cells[index].attendance = format!("{prefix}{marks}");
        }
    }

    cells
}

fn leave_symbol(process: &str) -> Option<&'static str> {
    [
        ("陪产假", "M"),
        ("产假", "M"),
        ("年假", "N"),
        ("病假", "△"),
        ("事假", "O"),
        ("婚假", "H"),
        ("丧假", "S"),
        ("育儿假", "Y"),
        ("哺乳假", "B"),
        ("产检假", "P"),
        ("调休", "T"),
    ]
    .into_iter()
    .find_map(|(name, symbol)| process.starts_with(name).then_some(symbol))
}

fn allocate_process_hours(process: &str, occurrence_count: usize) -> Vec<f64> {
    if occurrence_count == 0 {
        return Vec::new();
    }
    if extract_amount_before(process, "天").is_some() {
        return vec![8.0; occurrence_count];
    }
    let total = extract_amount_before(process, "小时").unwrap_or(0.0);
    let mut remaining = total;
    let mut result = vec![0.0; occurrence_count];
    for amount in result.iter_mut().rev() {
        *amount = remaining.min(8.0);
        remaining = (remaining - *amount).max(0.0);
    }
    result
}

fn flexible_arrival_irregular_minutes(daily: &DailyRecord, attendance_result: &str) -> Option<u32> {
    const ARRIVAL_BOUNDARY: u32 = 8 * 60 + 30;
    const EARLY_ARRIVAL_END: u32 = 17 * 60 + 30;
    const LATE_ARRIVAL_END: u32 = 18 * 60;

    if !is_scheduled_shift(&daily.shift)
        || attendance_result.contains("休息")
        || attendance_result.contains("未排班")
        || attendance_result.contains("出差")
        || attendance_result
            .split(',')
            .map(str::trim)
            .any(|part| leave_symbol(part).is_some())
    {
        return None;
    }

    let arrival = daily
        .punch_slots
        .iter()
        .filter(|slot| slot.kind == PunchKind::In)
        .filter_map(|slot| parse_punch_minutes(&slot.time))
        .min()?;
    let departure = daily
        .punch_slots
        .iter()
        .filter(|slot| slot.kind == PunchKind::Out)
        .filter_map(|slot| parse_punch_minutes(&slot.time))
        .max()?;
    let expected_end = if arrival <= ARRIVAL_BOUNDARY {
        EARLY_ARRIVAL_END
    } else {
        LATE_ARRIVAL_END
    };
    let early_minutes = expected_end.saturating_sub(departure);
    (early_minutes > 0).then_some(early_minutes)
}

fn daily_irregular_mark(
    daily: &DailyRecord,
    attendance_result: &str,
    uses_flexible_arrival_shift: bool,
    ignores_in_exception: bool,
) -> String {
    if uses_flexible_arrival_shift {
        let mut marks = flexible_arrival_irregular_minutes(daily, attendance_result)
            .map(|minutes| format!("Z{minutes}"))
            .into_iter()
            .collect::<Vec<_>>();
        if (!ignores_in_exception && daily.missing_in_count > 0.0) || daily.missing_out_count > 0.0
        {
            marks.push("缺卡".to_owned());
        }
        return marks.join("");
    }

    let expected_times = extract_clock_minutes(&daily.shift);
    let expected_start = expected_times.first().copied();
    let expected_end = expected_times.get(1).copied().map(|end| {
        if Some(end) < expected_start {
            end + 24 * 60
        } else {
            end
        }
    });
    let mut marks = Vec::new();
    for slot in &daily.punch_slots {
        if ignores_in_exception && slot.kind == PunchKind::In {
            continue;
        }
        let value = match (slot.kind, slot.result.as_str()) {
            (PunchKind::In, "迟到") => expected_start.and_then(|expected| {
                parse_punch_minutes(&slot.time).map(|actual| ("D", actual.saturating_sub(expected)))
            }),
            (PunchKind::Out, "早退") => expected_end.and_then(|expected| {
                parse_punch_minutes(&slot.time).map(|actual| ("Z", expected.saturating_sub(actual)))
            }),
            _ => None,
        };
        if let Some((symbol, minutes)) = value.filter(|(_, minutes)| *minutes > 0) {
            marks.push(format!("{symbol}{minutes}"));
        }
    }
    if (!ignores_in_exception && daily.missing_in_count > 0.0) || daily.missing_out_count > 0.0 {
        marks.push("缺卡".to_owned());
    }
    marks.join("")
}

fn day_from_date(text: &str) -> Option<usize> {
    text.split_whitespace()
        .next()?
        .split('-')
        .nth(2)?
        .parse()
        .ok()
}

pub(crate) fn days_in_month(year: u16, month: u8) -> usize {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 0,
    }
}

fn format_hours(value: f64) -> String {
    if (value - value.round()).abs() < 0.000_001 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[derive(Clone, Copy, Default)]
struct ExceptionOptions {
    uses_flexible_arrival_shift: bool,
    hire_day: Option<usize>,
}

fn calculate_exceptions(
    employee_no: &str,
    name: &str,
    department: &str,
    daily_records: &[&DailyRecord],
    invalid_punches: &[&crate::model::InvalidPunch],
    daily_results: &[String],
    options: ExceptionOptions,
) -> (Option<ExceptionRow>, u32) {
    let mut row = ExceptionRow {
        employee_no: employee_no.to_owned(),
        name: name.to_owned(),
        missing_in: 0,
        missing_out: 0,
        late_or_early_under_10: 0,
        late_or_early_11_to_30: 0,
        late_or_early_30_to_120_minutes: 0,
        out_of_range: invalid_punches.len() as u32,
        score: 0.0,
        notes: Vec::new(),
    };
    let mut unclassified = 0;
    let mut under_10_notes = Vec::new();

    for daily in daily_records {
        let date = display_date(&daily.date);
        let ignores_in_exception = options
            .hire_day
            .is_some_and(|hire_day| day_from_date(&daily.date) == Some(hire_day));
        let reported_missing_in = daily.missing_in_count.round().max(0.0) as u32;
        let missing_in = if ignores_in_exception {
            0
        } else {
            reported_missing_in
        };
        let missing_out = daily.missing_out_count.round().max(0.0) as u32;
        if missing_in > 0 {
            row.missing_in += missing_in;
            row.notes.push(format!("{date}上班未签到"));
        }
        if missing_out > 0 {
            row.missing_out += missing_out;
            row.notes.push(format!("{date}下班未签退"));
        }

        if daily.absent_days > 0.0
            && daily.punch_slots.is_empty()
            && is_scheduled_shift(&daily.shift)
            && reported_missing_in == 0
            && missing_out == 0
        {
            let missing_days = daily.absent_days.round().max(0.0) as u32;
            if !ignores_in_exception {
                row.missing_in += missing_days;
                row.notes.push(format!("{date}上班未签到"));
            }
            row.missing_out += missing_days;
            row.notes.push(format!("{date}下班未签退"));
        }

        if options.uses_flexible_arrival_shift {
            let attendance_result = day_from_date(&daily.date)
                .and_then(|day| daily_results.get(day.saturating_sub(1)))
                .map(String::as_str)
                .unwrap_or("");
            if let Some(minutes) = flexible_arrival_irregular_minutes(daily, attendance_result) {
                classify_minutes(
                    &mut row,
                    &mut under_10_notes,
                    minutes,
                    &date,
                    PunchKind::Out,
                );
            }
            continue;
        }

        let expected_times = extract_clock_minutes(&daily.shift);
        let expected_start = expected_times.first().copied();
        let expected_end = expected_times.get(1).copied().map(|end| {
            if Some(end) < expected_start {
                end + 24 * 60
            } else {
                end
            }
        });

        for slot in &daily.punch_slots {
            if ignores_in_exception && slot.kind == PunchKind::In {
                continue;
            }
            let minutes = match (slot.kind, slot.result.as_str()) {
                (PunchKind::In, "迟到") => expected_start.and_then(|expected| {
                    parse_punch_minutes(&slot.time).map(|actual| actual.saturating_sub(expected))
                }),
                (PunchKind::Out, "早退") => expected_end.and_then(|expected| {
                    parse_punch_minutes(&slot.time).map(|actual| expected.saturating_sub(actual))
                }),
                _ => continue,
            };

            if let Some(minutes) = minutes.filter(|value| *value > 0) {
                classify_minutes(&mut row, &mut under_10_notes, minutes, &date, slot.kind);
            } else {
                unclassified += 1;
            }
        }
    }

    for invalid in invalid_punches {
        row.notes.push(format!(
            "{}不在范围内打卡",
            display_date(&invalid.attendance_date)
        ));
    }

    let exempt_under_10 = if department.contains("人事行政部") {
        0
    } else {
        2
    };
    let counted_under_10 = under_10_notes.len().saturating_sub(exempt_under_10);
    row.late_or_early_under_10 = counted_under_10 as u32;
    row.notes
        .extend(under_10_notes.into_iter().skip(exempt_under_10));

    row.score = (row.missing_in + row.missing_out + row.late_or_early_under_10 + row.out_of_range)
        as f64
        + row.late_or_early_11_to_30 as f64 * 2.0
        + (row.late_or_early_30_to_120_minutes as f64 / 30.0).ceil();

    let has_exception = row.score > 0.0 || row.late_or_early_30_to_120_minutes > 0;
    (has_exception.then_some(row), unclassified)
}

fn classify_minutes(
    row: &mut ExceptionRow,
    under_10_notes: &mut Vec<String>,
    minutes: u32,
    date: &str,
    kind: PunchKind,
) {
    let label = match kind {
        PunchKind::In => "迟到",
        PunchKind::Out => "早退",
    };
    let note = format!("{date}{label}{minutes}分钟");
    match minutes {
        1..=10 => {
            under_10_notes.push(note);
            return;
        }
        11..=29 => row.late_or_early_11_to_30 += 1,
        30..=120 => row.late_or_early_30_to_120_minutes += minutes,
        _ => return,
    }
    row.notes.push(note);
}

fn is_scheduled_shift(shift: &str) -> bool {
    !shift.is_empty() && shift != "休息"
}

fn summary_or_process_hours(summary_present: bool, summary: f64, process: f64) -> f64 {
    if summary_present { summary } else { process }
}

fn unique_process_amount_for_month(
    daily_results: &[String],
    process_name: &str,
    daily_records: &[&DailyRecord],
    year: u16,
    month: u8,
    standard_daily_hours: f64,
    uses_six_day_schedule: bool,
    employment: Option<&EmploymentRecord>,
) -> f64 {
    let mut processes = BTreeMap::<String, Vec<usize>>::new();
    for (day_index, result) in daily_results.iter().enumerate() {
        for part in result.split(',').map(str::trim) {
            if part.starts_with(process_name) {
                processes
                    .entry(part.to_owned())
                    .or_default()
                    .push(day_index);
            }
        }
    }

    processes
        .into_iter()
        .map(|(process, indexes)| {
            process_allocations_for_month(
                &process,
                &indexes,
                daily_records,
                year,
                month,
                standard_daily_hours,
                uses_six_day_schedule,
                employment,
            )
            .into_iter()
            .map(|(_, hours)| hours)
            .sum::<f64>()
        })
        .sum()
}

#[derive(Debug, Clone, Copy)]
struct ProcessSpan {
    start: CalendarDate,
    end: CalendarDate,
    start_minutes: u32,
    end_minutes: u32,
    total_hours: f64,
}

fn process_allocations_for_month(
    process: &str,
    occurrence_indexes: &[usize],
    daily_records: &[&DailyRecord],
    year: u16,
    month: u8,
    standard_daily_hours: f64,
    uses_six_day_schedule: bool,
    employment: Option<&EmploymentRecord>,
) -> Vec<(usize, f64)> {
    let Some(span) = parse_process_span(process, year, month, standard_daily_hours) else {
        return occurrence_indexes
            .iter()
            .copied()
            .zip(allocate_process_hours(process, occurrence_indexes.len()))
            .collect();
    };
    let daily_by_day = daily_records
        .iter()
        .filter_map(|daily| day_from_date(&daily.date).map(|day| (day, *daily)))
        .collect::<HashMap<_, _>>();
    let mut allocations = Vec::<(CalendarDate, f64)>::new();
    let mut date = span.start;
    while date <= span.end {
        if is_leave_workday(date, &daily_by_day, year, month, uses_six_day_schedule) {
            let segments =
                leave_work_segments(date, &daily_by_day, year, month, standard_daily_hours);
            let lower = if date == span.start {
                span.start_minutes
            } else {
                0
            };
            let upper = if date == span.end {
                span.end_minutes
            } else {
                u32::MAX
            };
            let hours = segments
                .iter()
                .map(|(start, end)| {
                    end.min(&upper).saturating_sub((*start).max(lower)) as f64 / 60.0
                })
                .sum::<f64>()
                .min(standard_daily_hours);
            if hours > 0.001 {
                allocations.push((date, hours));
            }
        }
        let Some(next) = next_calendar_date(date) else {
            break;
        };
        date = next;
    }

    let allocated_hours = allocations.iter().map(|(_, hours)| *hours).sum::<f64>();
    if allocated_hours > 0.001 && (allocated_hours - span.total_hours).abs() > 0.001 {
        let ratio = span.total_hours / allocated_hours;
        for (_, hours) in &mut allocations {
            *hours *= ratio;
        }
    }

    allocations
        .into_iter()
        .filter(|(date, _)| {
            date.year == year
                && date.month == month
                && is_active_day(employment, date.year, date.month, date.day as usize)
        })
        .map(|(date, hours)| (date.day as usize - 1, hours))
        .collect()
}

fn parse_process_span(
    process: &str,
    report_year: u16,
    report_month: u8,
    standard_daily_hours: f64,
) -> Option<ProcessSpan> {
    let month_days = extract_month_days(process);
    let [(start_month, start_day), (end_month, end_day), ..] = month_days.as_slice() else {
        return None;
    };
    let times = extract_clock_minutes(process);
    let [start_minutes, end_minutes, ..] = times.as_slice() else {
        return None;
    };
    let mut start_year = report_year;
    if *start_month > report_month && *end_month <= report_month {
        start_year = start_year.checked_sub(1)?;
    }
    let end_year = if end_month < start_month {
        start_year.checked_add(1)?
    } else {
        start_year
    };
    let start = valid_calendar_date(start_year, *start_month, *start_day)?;
    let end = valid_calendar_date(end_year, *end_month, *end_day)?;
    if end < start {
        return None;
    }
    let total_hours = extract_amount_before(process, "天")
        .map(|days| days * standard_daily_hours)
        .or_else(|| extract_amount_before(process, "小时"))?;
    Some(ProcessSpan {
        start,
        end,
        start_minutes: *start_minutes,
        end_minutes: *end_minutes,
        total_hours,
    })
}

fn extract_month_days(text: &str) -> Vec<(u8, u8)> {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index + 4 < bytes.len() {
        if bytes[index].is_ascii_digit()
            && bytes[index + 1].is_ascii_digit()
            && bytes[index + 2] == b'-'
            && bytes[index + 3].is_ascii_digit()
            && bytes[index + 4].is_ascii_digit()
        {
            let month = (bytes[index] - b'0') * 10 + (bytes[index + 1] - b'0');
            let day = (bytes[index + 3] - b'0') * 10 + (bytes[index + 4] - b'0');
            if (1..=12).contains(&month) && (1..=31).contains(&day) {
                result.push((month, day));
            }
            index += 5;
        } else {
            index += 1;
        }
    }
    result
}

fn valid_calendar_date(year: u16, month: u8, day: u8) -> Option<CalendarDate> {
    (day > 0 && day as usize <= days_in_month(year, month)).then_some(CalendarDate {
        year,
        month,
        day,
    })
}

fn next_calendar_date(date: CalendarDate) -> Option<CalendarDate> {
    if (date.day as usize) < days_in_month(date.year, date.month) {
        return Some(CalendarDate {
            day: date.day + 1,
            ..date
        });
    }
    if date.month < 12 {
        Some(CalendarDate {
            year: date.year,
            month: date.month + 1,
            day: 1,
        })
    } else {
        Some(CalendarDate {
            year: date.year.checked_add(1)?,
            month: 1,
            day: 1,
        })
    }
}

fn is_leave_workday(
    date: CalendarDate,
    daily_by_day: &HashMap<usize, &DailyRecord>,
    report_year: u16,
    report_month: u8,
    uses_six_day_schedule: bool,
) -> bool {
    if date.year == report_year
        && date.month == report_month
        && let Some(daily) = daily_by_day.get(&(date.day as usize))
    {
        if is_scheduled_shift(&daily.shift) {
            return true;
        }
        if daily.shift == "休息" {
            return false;
        }
    }
    if uses_six_day_schedule {
        holiday::is_six_day_workday(date.year, date.month, date.day)
    } else {
        holiday::is_workday(date.year, date.month, date.day)
    }
}

fn leave_work_segments(
    date: CalendarDate,
    daily_by_day: &HashMap<usize, &DailyRecord>,
    report_year: u16,
    report_month: u8,
    standard_daily_hours: f64,
) -> Vec<(u32, u32)> {
    let shift_times = (date.year == report_year && date.month == report_month)
        .then(|| daily_by_day.get(&(date.day as usize)))
        .flatten()
        .map(|daily| extract_clock_minutes(&daily.shift))
        .unwrap_or_default();
    let (start, end) = match shift_times.as_slice() {
        [start, end, ..] => (*start, *end),
        _ if standard_daily_hours >= 7.0 => (8 * 60 + 30, 17 * 60 + 30),
        _ => (
            8 * 60 + 30,
            8 * 60 + 30 + (standard_daily_hours * 60.0) as u32,
        ),
    };
    if standard_daily_hours >= 7.0 && start < 12 * 60 && end > 13 * 60 {
        vec![(start, 12 * 60), (13 * 60, end)]
    } else {
        vec![(start, end)]
    }
}

fn extract_amount_before(text: &str, unit: &str) -> Option<f64> {
    let unit_index = text.rfind(unit)?;
    let before = &text[..unit_index];
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.')
        .last()
        .map(|(index, _)| index)?;
    before[start..].parse().ok()
}

fn overtime_application_ranges(
    result: &str,
    year: u16,
    month: u8,
    day: u8,
) -> Option<Vec<(u32, u32)>> {
    let date = valid_calendar_date(year, month, day)?;
    let mut ranges = Vec::new();
    for part in result
        .split(',')
        .map(str::trim)
        .filter(|part| part.starts_with("加班"))
    {
        let span = parse_process_span(part, year, month, 8.0)?;
        if date < span.start || date > span.end {
            continue;
        }
        let start = if span.start == date {
            span.start_minutes
        } else {
            0
        };
        let mut end = span.end_minutes;
        let mut end_date = date;
        while end_date < span.end {
            end_date = next_calendar_date(end_date)?;
            end += 1440;
        }
        if end <= start {
            return None;
        }
        ranges.push((start, end));
    }
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    Some(merged)
}

fn extract_clock_minutes(text: &str) -> Vec<u32> {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index + 4 < bytes.len() {
        if bytes[index].is_ascii_digit()
            && bytes[index + 1].is_ascii_digit()
            && bytes[index + 2] == b':'
            && bytes[index + 3].is_ascii_digit()
            && bytes[index + 4].is_ascii_digit()
        {
            let hour = ((bytes[index] - b'0') as u32) * 10 + (bytes[index + 1] - b'0') as u32;
            let minute = ((bytes[index + 3] - b'0') as u32) * 10 + (bytes[index + 4] - b'0') as u32;
            if (hour < 24 && minute < 60) || (hour == 24 && minute == 0) {
                result.push(hour * 60 + minute);
            }
            index += 5;
        } else {
            index += 1;
        }
    }
    result
}

fn parse_punch_minutes(text: &str) -> Option<u32> {
    let mut minutes = extract_clock_minutes(text).into_iter().next()?;
    if text.contains("次日") {
        minutes += 24 * 60;
    }
    Some(minutes)
}

fn display_date(text: &str) -> String {
    let date = text.split_whitespace().next().unwrap_or(text);
    let mut parts = date.split('-');
    let _year = parts.next();
    let month = parts.next().and_then(|value| value.parse::<u32>().ok());
    let day = parts.next().and_then(|value| value.parse::<u32>().ok());
    match (month, day) {
        (Some(month), Some(day)) => format!("{month}.{day}"),
        _ => date.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flexible_daily(arrival: &str, departure: &str) -> DailyRecord {
        DailyRecord {
            employee_key: "26333".to_owned(),
            employee_no: "26333".to_owned(),
            name: "张一成".to_owned(),
            date: "26-07-01 星期三".to_owned(),
            shift: "默认班次 08:30-17:30".to_owned(),
            overtime_hours: 0.0,
            punch_slots: vec![
                crate::model::PunchSlot {
                    kind: PunchKind::In,
                    time: arrival.to_owned(),
                    result: "迟到".to_owned(),
                },
                crate::model::PunchSlot {
                    kind: PunchKind::Out,
                    time: departure.to_owned(),
                    result: "正常".to_owned(),
                },
            ],
            late_count: 1.0,
            severe_late_count: 0.0,
            absent_late_days: 0.0,
            early_count: 0.0,
            missing_in_count: 0.0,
            missing_out_count: 0.0,
            absent_days: 0.0,
        }
    }

    fn meal_daily(arrival: &str, departure: &str, approved_hours: f64) -> DailyRecord {
        let mut daily = flexible_daily(arrival, departure);
        daily.employee_key = "10001".to_owned();
        daily.employee_no = "10001".to_owned();
        daily.name = "测试员工".to_owned();
        daily.overtime_hours = approved_hours;
        daily.punch_slots[0].result = "正常".to_owned();
        daily
    }

    fn overtime_daily(date: &str, overtime_hours: f64) -> DailyRecord {
        DailyRecord {
            employee_key: "10001".to_owned(),
            employee_no: "10001".to_owned(),
            name: "测试员工".to_owned(),
            date: date.to_owned(),
            shift: "休息".to_owned(),
            overtime_hours,
            punch_slots: vec![],
            late_count: 0.0,
            severe_late_count: 0.0,
            absent_late_days: 0.0,
            early_count: 0.0,
            missing_in_count: 0.0,
            missing_out_count: 0.0,
            absent_days: 0.0,
        }
    }

    fn overtime_daily_with_punches(
        date: &str,
        first_in: &str,
        last_out: &str,
        overtime_hours: f64,
    ) -> DailyRecord {
        let mut daily = overtime_daily(date, overtime_hours);
        daily.shift = "默认班次 08:30-17:30".to_owned();
        daily.punch_slots = vec![
            crate::model::PunchSlot {
                kind: PunchKind::In,
                time: first_in.to_owned(),
                result: "正常".to_owned(),
            },
            crate::model::PunchSlot {
                kind: PunchKind::Out,
                time: last_out.to_owned(),
                result: "正常".to_owned(),
            },
        ];
        daily
    }

    fn empty_monthly_record() -> crate::model::MonthlyRecord {
        crate::model::MonthlyRecord {
            employee_key: "10001".to_owned(),
            employee_no: "10001".to_owned(),
            name: "测试员工".to_owned(),
            user_id: String::new(),
            attendance_group: String::new(),
            department: String::new(),
            position: String::new(),
            attendance_days: 0.0,
            weekday_overtime_hours: 0.0,
            weekend_overtime_hours: 0.0,
            holiday_overtime_hours: 0.0,
            personal_leave_hours: 0.0,
            compensatory_leave_hours: 0.0,
            sick_leave_hours: 0.0,
            annual_leave_hours: 0.0,
            maternity_leave_days: 0.0,
            paternity_leave_days: 0.0,
            marriage_leave_days: 0.0,
            menstrual_leave_days: 0.0,
            bereavement_leave_days: 0.0,
            breastfeeding_leave_hours: 0.0,
            leave_summary_present: Default::default(),
            daily_results: vec![],
        }
    }

    #[test]
    fn parses_shift_and_next_day_times() {
        assert_eq!(
            extract_clock_minutes("默认班次 08:30-17:30"),
            vec![510, 1050]
        );
        assert_eq!(parse_punch_minutes("次日 00:04"), Some(1444));
    }

    #[test]
    fn parses_leave_days_and_hours() {
        assert_eq!(
            extract_amount_before("育儿假07-01到07-02 2天", "天"),
            Some(2.0)
        );
        assert_eq!(
            extract_amount_before("育儿假07-01 3.5小时", "小时"),
            Some(3.5)
        );
    }

    #[test]
    fn leave_process_fallback_clips_cross_month_hours_to_workdays() {
        let sick = ["病假08-17 08:30到09-16 17:30 184小时".to_owned()];
        assert_eq!(
            unique_process_amount_for_month(&sick, "病假", &[], 2026, 8, 8.0, false, None,),
            88.0
        );

        let personal = ["事假08-31 13:30到09-01 17:30 12小时".to_owned()];
        assert_eq!(
            unique_process_amount_for_month(&personal, "事假", &[], 2026, 8, 8.0, false, None,),
            4.0
        );

        let marriage = ["婚假07-19 08:30到08-02 17:30 10天".to_owned()];
        assert_eq!(
            unique_process_amount_for_month(&marriage, "婚假", &[], 2026, 8, 8.0, false, None,),
            0.0
        );
    }

    #[test]
    fn blank_monthly_leave_summary_falls_back_and_merges_same_day_processes() {
        let mut monthly = empty_monthly_record();
        monthly.daily_results = vec![String::new(); 31];
        monthly.daily_results[2] =
            "事假08-03 08:30到08-03 13:00 3.5小时,事假08-03 13:00到08-03 13:30 0.5小时".to_owned();
        let daily = DailyRecord {
            employee_key: "10001".to_owned(),
            employee_no: "10001".to_owned(),
            name: "测试员工".to_owned(),
            date: "26-08-03 星期一".to_owned(),
            shift: "默认班次 08:30-17:30".to_owned(),
            overtime_hours: 0.0,
            punch_slots: vec![],
            late_count: 0.0,
            severe_late_count: 0.0,
            absent_late_days: 0.0,
            early_count: 0.0,
            missing_in_count: 0.0,
            missing_out_count: 0.0,
            absent_days: 0.0,
        };
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 8,
            },
            monthly: vec![monthly],
            daily: vec![daily],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        });

        assert_eq!(report.summary_rows[0].personal_leave_hours, 4.0);
        assert_eq!(report.summary_rows[0].expected_attendance_hours, 8.0);
        assert_eq!(report.summary_rows[0].actual_attendance_hours, Some(4.0));
        assert_eq!(report.detail_rows[0].days[2].attendance, "O4");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("事假1人"))
        );
    }

    #[test]
    fn populated_monthly_leave_summary_remains_authoritative() {
        let mut monthly = empty_monthly_record();
        monthly.personal_leave_hours = 6.0;
        monthly.leave_summary_present.personal = true;
        monthly.daily_results = vec!["事假08-03 08:30到08-03 17:30 8小时".to_owned()];
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 8,
            },
            monthly: vec![monthly],
            daily: vec![],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        });

        assert_eq!(report.summary_rows[0].personal_leave_hours, 6.0);
    }

    #[test]
    fn overtime_starts_at_one_hour_and_increases_by_completed_half_hours() {
        for (source, expected) in [
            (0.0, 0.0),
            (0.99, 0.0),
            (1.0, 1.0),
            (1.49, 1.0),
            (1.5, 1.5),
            (1.99, 1.5),
            (2.0, 2.0),
            (2.49, 2.0),
            (2.5, 2.5),
        ] {
            assert_eq!(payable_overtime_hours(source), expected, "source={source}");
        }
        let categories = normalize_overtime_categories([0.26, 0.26, 0.48], 1.0);
        assert_eq!(categories, (0.0, 0.5, 0.5));
        assert_eq!(categories.0 + categories.1 + categories.2, 1.0);
    }

    #[test]
    fn overtime_increment_applies_to_daily_monthly_and_actual_attendance() {
        let monthly = crate::model::MonthlyRecord {
            employee_key: "10001".to_owned(),
            employee_no: "10001".to_owned(),
            name: "测试员工".to_owned(),
            user_id: String::new(),
            attendance_group: String::new(),
            department: String::new(),
            position: String::new(),
            attendance_days: 3.0,
            weekday_overtime_hours: 5.97,
            weekend_overtime_hours: 0.0,
            holiday_overtime_hours: 0.0,
            personal_leave_hours: 0.0,
            compensatory_leave_hours: 0.0,
            sick_leave_hours: 0.0,
            annual_leave_hours: 0.0,
            maternity_leave_days: 0.0,
            paternity_leave_days: 0.0,
            marriage_leave_days: 0.0,
            menstrual_leave_days: 0.0,
            bereavement_leave_days: 0.0,
            breastfeeding_leave_hours: 0.0,
            leave_summary_present: Default::default(),
            daily_results: vec![String::new(); 3],
        };
        let daily = [1.49, 1.99, 2.49]
            .into_iter()
            .enumerate()
            .map(|(index, overtime_hours)| DailyRecord {
                employee_key: "10001".to_owned(),
                employee_no: "10001".to_owned(),
                name: "测试员工".to_owned(),
                date: format!("26-07-{:02}", index + 1),
                shift: "默认班次 08:30-17:30".to_owned(),
                overtime_hours,
                punch_slots: vec![],
                late_count: 0.0,
                severe_late_count: 0.0,
                absent_late_days: 0.0,
                early_count: 0.0,
                missing_in_count: 0.0,
                missing_out_count: 0.0,
                absent_days: 0.0,
            })
            .collect();
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily,
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        });

        let detail = &report.detail_rows[0];
        assert_eq!(
            detail.days[..3]
                .iter()
                .map(|day| day.overtime_hours)
                .collect::<Vec<_>>(),
            vec![1.0, 1.5, 2.0]
        );
        assert_eq!(detail.summary.weekday_overtime_hours, 4.5);
        assert_eq!(detail.summary.weekend_overtime_hours, 0.0);
        assert_eq!(detail.summary.holiday_overtime_hours, 0.0);
        assert_eq!(detail.summary.actual_attendance_hours, Some(28.5));
    }

    #[test]
    fn monthly_overtime_is_used_when_daily_overtime_is_unavailable() {
        let mut monthly = empty_monthly_record();
        monthly.weekday_overtime_hours = 1.49;
        monthly.weekend_overtime_hours = 1.99;
        monthly.holiday_overtime_hours = 2.49;
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        });

        let summary = &report.summary_rows[0];
        assert_eq!(summary.weekday_overtime_hours, 1.0);
        assert_eq!(summary.weekend_overtime_hours, 1.5);
        assert_eq!(summary.holiday_overtime_hours, 2.0);
        assert_eq!(summary.actual_attendance_hours, Some(4.5));
    }

    #[test]
    fn annual_leave_balance_subtracts_current_month_usage() {
        let mut monthly = empty_monthly_record();
        monthly.annual_leave_hours = 99.0;
        monthly.daily_results = vec!["年假07-01 13:00到07-01 16:30 3.5小时".to_owned()];
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![AnnualLeaveRecord {
                employee_no: String::new(),
                name: "测试员工".to_owned(),
                company: "JSFAE".to_owned(),
                balance_before_month_hours: 32.5,
            }],
        });

        assert_eq!(
            report.summary_rows[0].annual_leave_balance_hours,
            Some(29.0)
        );
        assert_eq!(report.summary_rows[0].annual_leave_hours, 3.5);
        assert_eq!(report.detail_rows[0].days[0].attendance, "N3.5");
    }

    #[test]
    fn annual_leave_summary_deduplicates_multi_day_daily_processes() {
        let mut monthly = empty_monthly_record();
        monthly.daily_results = vec![
            "年假07-01 08:30到07-02 17:30 16小时".to_owned(),
            "年假07-01 08:30到07-02 17:30 16小时".to_owned(),
            "年假07-03 08:30到07-03 09:00 0.5小时".to_owned(),
        ];
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        });

        assert_eq!(report.summary_rows[0].annual_leave_hours, 16.5);
        assert_eq!(report.detail_rows[0].days[0].attendance, "N8");
        assert_eq!(report.detail_rows[0].days[1].attendance, "N8");
        assert_eq!(report.detail_rows[0].days[2].attendance, "N0.5");
    }

    #[test]
    fn duplicate_annual_leave_names_without_employee_numbers_are_ambiguous() {
        let records = vec![
            AnnualLeaveRecord {
                employee_no: String::new(),
                name: "同名员工".to_owned(),
                company: "JSFAE".to_owned(),
                balance_before_month_hours: 8.0,
            },
            AnnualLeaveRecord {
                employee_no: String::new(),
                name: "同名员工".to_owned(),
                company: "JSFAE".to_owned(),
                balance_before_month_hours: 16.0,
            },
        ];

        assert_eq!(
            annual_leave_balance_before_month(&records, "10001", "同名员工", "JSFAE"),
            AnnualLeaveMatch::Ambiguous
        );
    }

    #[test]
    fn dinner_break_is_excluded_at_1930_before_half_hour_rounding() {
        let mut monthly = empty_monthly_record();
        monthly.weekday_overtime_hours = 5.49;
        monthly.daily_results = vec![
            "加班07-01 17:30到07-01 19:29 2小时".to_owned(),
            "加班07-02 17:30到07-02 19:30 2小时".to_owned(),
            "加班07-03 17:30到07-03 19:30 1.49小时".to_owned(),
        ];
        let dataset = AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![
                overtime_daily_with_punches("26-07-01", "08:30", "19:29", 2.0),
                overtime_daily_with_punches("26-07-02", "08:30", "19:30", 2.0),
                overtime_daily_with_punches("26-07-03", "08:30", "19:30", 1.49),
            ],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        };

        let report = calculate_attendance(&dataset);
        let detail = &report.detail_rows[0];
        assert_eq!(
            detail.days[..3]
                .iter()
                .map(|day| day.overtime_hours)
                .collect::<Vec<_>>(),
            vec![1.5, 1.5, 1.0]
        );
        assert_eq!(detail.summary.weekday_overtime_hours, 4.0);
        assert_eq!(detail.summary.actual_attendance_hours, Some(28.0));
        assert_eq!(detail.summary.overtime_meal_count, 2.0);
    }

    #[test]
    fn declared_overtime_is_capped_without_deducting_breaks_twice() {
        for (start, end, arrival, departure, declared, expected, meals) in [
            ("17:30", "19:30", "08:30", "19:30", 2.0, 1.5, 1),
            ("17:30", "19:30", "08:30", "19:30", 1.5, 1.5, 1),
            ("17:30", "19:30", "08:30", "19:30", 1.0, 1.0, 1),
            ("17:30", "19:30", "08:30", "19:30", 0.0, 0.0, 0),
            ("17:30", "19:30", "08:30", "19:29", 2.0, 1.5, 0),
            ("17:30", "19:00", "08:30", "20:00", 2.0, 1.0, 0),
            ("18:00", "19:30", "08:30", "19:44", 1.5, 1.5, 1),
            ("17:30", "19:30", "18:00", "19:30", 1.5, 1.5, 1),
            ("17:30", "24:00", "08:30", "次日 00:00", 6.0, 6.0, 2),
            ("08:30", "19:30", "08:30", "19:30", 10.0, 9.5, 1),
            ("08:30", "19:30", "08:30", "19:30", 9.5, 9.5, 1),
        ] {
            let result = format!("加班07-01 {start}到07-01 {end} {declared}小时");
            let applications = overtime_application_ranges(&result, 2026, 7, 1);
            let daily = overtime_daily_with_punches("26-07-01", arrival, departure, declared);
            assert_eq!(
                payable_overtime_hours(cap_overtime_by_effective_time(
                    declared,
                    &daily,
                    applications.as_deref()
                )),
                expected,
                "{result}, {arrival}-{departure}"
            );
            assert_eq!(
                workday_overtime_meals(
                    &daily,
                    punch_range(&daily),
                    applications.as_deref(),
                    false,
                    false
                ),
                meals,
                "{result}, {arrival}-{departure}"
            );
        }
    }

    #[test]
    fn overtime_ranges_preserve_gaps_merge_overlap_and_handle_midnight() {
        let result = "加班07-31 17:30到07-31 19:00 1小时,加班07-31 18:00到07-31 19:00 1小时,加班07-31 20:00到08-01 00:00 4小时";
        let applications = overtime_application_ranges(result, 2026, 7, 31);
        assert_eq!(applications, Some(vec![(1050, 1140), (1200, 1440)]));
        let daily = overtime_daily_with_punches("26-07-31", "08:30", "次日 00:00", 6.0);
        assert_eq!(
            cap_overtime_by_effective_time(6.0, &daily, applications.as_deref()),
            5.0
        );
        assert_eq!(
            workday_overtime_meals(
                &daily,
                punch_range(&daily),
                applications.as_deref(),
                false,
                false
            ),
            1
        );
        assert_eq!(
            overtime_application_ranges("加班12-31 23:00到01-01 01:00 2小时", 2026, 12, 31),
            Some(vec![(1380, 1500)])
        );
        assert_eq!(
            overtime_application_ranges("加班07-01 17:30 1.5小时", 2026, 7, 1),
            None
        );
        assert_eq!(cap_overtime_by_effective_time(1.5, &daily, None), 1.5);
    }

    #[test]
    fn zero_effective_overtime_does_not_restore_monthly_declared_hours() {
        let mut monthly = empty_monthly_record();
        monthly.weekday_overtime_hours = 2.0;
        monthly.daily_results = vec!["加班07-01 20:00到07-01 22:00 2小时".to_owned()];
        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![overtime_daily_with_punches(
                "26-07-01", "08:30", "19:30", 2.0,
            )],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        });
        let detail = &report.detail_rows[0];
        assert_eq!(detail.days[0].overtime_hours, 0.0);
        assert_eq!(detail.summary.weekday_overtime_hours, 0.0);
        assert_eq!(detail.summary.actual_attendance_hours, Some(8.0));
        assert_eq!(detail.summary.overtime_meal_count, 0.0);
    }

    #[test]
    fn dinner_break_is_not_subtracted_when_application_starts_after_1730() {
        let daily = overtime_daily_with_punches("26-07-01", "08:30", "21:00", 3.0);
        assert_eq!(
            cap_overtime_by_effective_time(3.0, &daily, Some(&[(18 * 60, 21 * 60)])),
            3.0
        );
    }

    #[test]
    fn dinner_break_uses_application_start_instead_of_first_punch() {
        let daily = overtime_daily_with_punches("26-07-01", "18:00", "21:00", 3.5);
        assert_eq!(
            cap_overtime_by_effective_time(3.5, &daily, Some(&[(17 * 60 + 30, 21 * 60)])),
            3.0
        );
        assert_eq!(cap_overtime_by_effective_time(3.5, &daily, None), 3.5);
    }

    #[test]
    fn configured_triple_pay_dates_reclassify_other_holiday_days_as_weekends() {
        let mut monthly = empty_monthly_record();
        monthly.holiday_overtime_hours = 6.0;
        let dataset = AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 9,
            },
            monthly: vec![monthly],
            daily: vec![
                overtime_daily("26-09-25", 2.0),
                overtime_daily("26-09-26", 2.0),
                overtime_daily("26-09-27", 2.0),
            ],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        };

        let fallback = calculate_attendance(&dataset);
        assert_eq!(fallback.summary_rows[0].holiday_overtime_hours, 6.0);
        assert_eq!(fallback.summary_rows[0].weekend_overtime_hours, 0.0);

        let configured = calculate_attendance_with_config(
            &dataset,
            &AttendanceConfig {
                statutory_holiday_dates: vec!["2026-09-25".to_owned()],
                ..Default::default()
            },
        );
        assert_eq!(configured.summary_rows[0].holiday_overtime_hours, 2.0);
        assert_eq!(configured.summary_rows[0].weekend_overtime_hours, 4.0);
        assert!(configured.warnings.iter().any(|warning| {
            warning.contains("已按设置中的 1 个三倍工资日重新划分 2026 年")
        }));
    }

    #[test]
    fn weekday_weekend_rule_keeps_only_holiday_overtime_and_punch_meals() {
        let mut monthly = empty_monthly_record();
        monthly.daily_results = vec![String::new(); 5];
        monthly.weekday_overtime_hours = 2.0;
        monthly.weekend_overtime_hours = 2.0;
        monthly.holiday_overtime_hours = 2.0;
        let dataset = AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![
                overtime_daily_with_punches("26-07-01", "08:30", "19:30", 2.0),
                overtime_daily_with_punches("26-07-04", "10:00", "12:00", 2.0),
                overtime_daily_with_punches("26-07-05", "10:00", "12:00", 2.0),
            ],
            invalid_punches: vec![],
            employment_records: vec![],
            annual_leave_records: vec![],
        };
        let report = calculate_attendance_with_config(
            &dataset,
            &AttendanceConfig {
                special_personnel: crate::config::SpecialPersonnelConfig {
                    weekday_weekend_punch_meal_holiday_overtime: vec![
                        crate::config::SpecialPerson {
                            employee_no: "10001".to_owned(),
                            name: "测试员工".to_owned(),
                        },
                    ],
                    ..Default::default()
                },
                statutory_holiday_dates: vec!["2026-07-05".to_owned()],
                ..Default::default()
            },
        );

        let detail = &report.detail_rows[0];
        assert_eq!(detail.days[0].overtime_hours, 0.0);
        assert_eq!(detail.days[3].overtime_hours, 0.0);
        assert_eq!(detail.days[4].overtime_hours, 2.0);
        assert_eq!(detail.summary.weekday_overtime_hours, 0.0);
        assert_eq!(detail.summary.weekend_overtime_hours, 0.0);
        assert_eq!(detail.summary.holiday_overtime_hours, 2.0);
        assert_eq!(detail.summary.attendance_meal_count, 1.0);
        assert_eq!(detail.summary.overtime_meal_count, 3.0);
        assert_eq!(detail.summary.meal_allowance_count, Some(4.0));
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("清零工作日和周末加班时长，保留法定节假日加班"))
        );
    }

    #[test]
    fn formats_dingtalk_date() {
        assert_eq!(display_date("26-07-01 星期三"), "7.1");
    }

    #[test]
    fn flexible_arrival_shift_uses_0830_boundary_without_late_mark() {
        let boundary = flexible_daily("08:30", "17:30");
        assert_eq!(daily_irregular_mark(&boundary, "", true, false), "");
        assert!(
            calculate_exceptions(
                "26333",
                "张一成",
                "",
                &[&boundary],
                &[],
                &[],
                ExceptionOptions {
                    uses_flexible_arrival_shift: true,
                    ..Default::default()
                },
            )
            .0
            .is_none()
        );

        let after_boundary = flexible_daily("08:31", "18:00");
        assert_eq!(daily_irregular_mark(&after_boundary, "", true, false), "");
        assert!(
            calculate_exceptions(
                "26333",
                "张一成",
                "",
                &[&after_boundary],
                &[],
                &[],
                ExceptionOptions {
                    uses_flexible_arrival_shift: true,
                    ..Default::default()
                },
            )
            .0
            .is_none()
        );
    }

    #[test]
    fn flexible_arrival_shift_uses_1800_for_early_departure() {
        let daily = flexible_daily("08:31", "17:45");
        assert_eq!(daily_irregular_mark(&daily, "", true, false), "Z15");

        let exception = calculate_exceptions(
            "26333",
            "张一成",
            "",
            &[&daily],
            &[],
            &[],
            ExceptionOptions {
                uses_flexible_arrival_shift: true,
                ..Default::default()
            },
        )
        .0
        .expect("17:45 下班应按 18:00 计算早退");
        assert_eq!(exception.late_or_early_11_to_30, 1);
        assert_eq!(exception.late_or_early_under_10, 0);
        assert_eq!(exception.score, 2.0);
        assert_eq!(exception.notes, vec!["7.1早退15分钟"]);
        assert_eq!(
            daily_irregular_mark(&daily, "年假07-01 8小时", true, false),
            ""
        );
        let leave_results = vec!["年假07-01 8小时".to_owned()];
        assert!(
            calculate_exceptions(
                "26333",
                "张一成",
                "",
                &[&daily],
                &[],
                &leave_results,
                ExceptionOptions {
                    uses_flexible_arrival_shift: true,
                    ..Default::default()
                },
            )
            .0
            .is_none()
        );
    }

    #[test]
    fn non_hr_department_exempts_first_two_under_10_events_each_month() {
        let mut first = flexible_daily("08:35", "17:30");
        first.date = "26-07-01 星期三".to_owned();
        let mut second = flexible_daily("08:36", "17:30");
        second.date = "26-07-02 星期四".to_owned();
        let mut third = flexible_daily("08:37", "17:30");
        third.date = "26-07-03 星期五".to_owned();

        let first_two = calculate_exceptions(
            "10001",
            "测试员工",
            "研发部",
            &[&first, &second],
            &[],
            &[],
            ExceptionOptions::default(),
        );
        assert!(first_two.0.is_none());

        let exception = calculate_exceptions(
            "10001",
            "测试员工",
            "研发部",
            &[&first, &second, &third],
            &[],
            &[],
            ExceptionOptions::default(),
        )
        .0
        .expect("第三次 10 分钟内迟到应计入异常");
        assert_eq!(exception.late_or_early_under_10, 1);
        assert_eq!(exception.score, 1.0);
        assert_eq!(exception.notes, vec!["7.3迟到7分钟"]);
    }

    #[test]
    fn hr_department_does_not_exempt_under_10_events() {
        let mut first = flexible_daily("08:35", "17:30");
        first.date = "26-07-01 星期三".to_owned();
        let mut second = flexible_daily("08:36", "17:30");
        second.date = "26-07-02 星期四".to_owned();

        let exception = calculate_exceptions(
            "10001",
            "测试员工",
            "人事行政部",
            &[&first, &second],
            &[],
            &[],
            ExceptionOptions::default(),
        )
        .0
        .expect("人事行政部不享受两次豁免");
        assert_eq!(exception.late_or_early_under_10, 2);
        assert_eq!(exception.score, 2.0);
        assert_eq!(exception.notes, vec!["7.1迟到5分钟", "7.2迟到6分钟"]);
    }

    #[test]
    fn full_day_missing_punches_count_as_missing_in_and_missing_out() {
        let mut daily = flexible_daily("08:30", "17:30");
        daily.punch_slots.clear();
        daily.late_count = 0.0;
        daily.absent_days = 1.0;

        let exception = calculate_exceptions(
            "10001",
            "测试员工",
            "研发部",
            &[&daily],
            &[],
            &[],
            ExceptionOptions::default(),
        )
        .0
        .expect("整天无打卡应拆成未签到和未签退");
        assert_eq!(exception.missing_in, 1);
        assert_eq!(exception.missing_out, 1);
        assert_eq!(exception.score, 2.0);
        assert_eq!(exception.notes, vec!["7.1上班未签到", "7.1下班未签退"]);
    }

    #[test]
    fn hire_day_ignores_only_in_punch_exceptions() {
        let mut daily = flexible_daily("09:00", "17:00");
        daily.punch_slots[1].result = "早退".to_owned();
        daily.missing_in_count = 1.0;
        daily.missing_out_count = 1.0;

        assert_eq!(daily_irregular_mark(&daily, "", false, true), "Z30缺卡");
        let exception = calculate_exceptions(
            "10001",
            "测试员工",
            "人事行政部",
            &[&daily],
            &[],
            &[],
            ExceptionOptions {
                hire_day: Some(1),
                ..Default::default()
            },
        )
        .0
        .expect("入职日的下班卡异常仍应计入");

        assert_eq!(exception.missing_in, 0);
        assert_eq!(exception.missing_out, 1);
        assert_eq!(exception.late_or_early_30_to_120_minutes, 30);
        assert_eq!(exception.score, 2.0);
        assert_eq!(exception.notes, vec!["7.1下班未签退", "7.1早退30分钟"]);
    }

    #[test]
    fn report_applies_in_punch_exception_exemption_only_on_hire_day() {
        let mut monthly = empty_monthly_record();
        monthly.department = "人事行政部".to_owned();
        monthly.daily_results = vec![String::new(); 31];

        let mut hire_day = flexible_daily("09:00", "17:30");
        hire_day.employee_key = monthly.employee_key.clone();
        hire_day.employee_no = monthly.employee_no.clone();
        hire_day.name = monthly.name.clone();
        hire_day.date = "26-07-20 星期一".to_owned();
        hire_day.missing_in_count = 1.0;
        let mut next_day = hire_day.clone();
        next_day.date = "26-07-21 星期二".to_owned();
        next_day.missing_in_count = 0.0;

        let report = calculate_attendance(&AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![hire_day, next_day],
            invalid_punches: vec![],
            employment_records: vec![EmploymentRecord {
                employee_no: "10001".to_owned(),
                name: "测试员工".to_owned(),
                company: "JSFAE".to_owned(),
                hire_date: Some(CalendarDate {
                    year: 2026,
                    month: 7,
                    day: 20,
                }),
                termination_date: None,
            }],
            annual_leave_records: vec![],
        });

        assert_eq!(report.detail_rows[0].days[19].attendance, "√");
        assert_eq!(report.detail_rows[0].days[20].attendance, "√D30");
        let exception = &report.exception_rows[0];
        assert_eq!(exception.missing_in, 0);
        assert_eq!(exception.late_or_early_30_to_120_minutes, 30);
        assert_eq!(exception.notes, vec!["7.21迟到30分钟"]);
    }

    #[test]
    fn thirty_minutes_belongs_to_minute_bucket() {
        let mut twenty_nine = flexible_daily("08:59", "17:30");
        twenty_nine.date = "26-07-01 星期三".to_owned();
        let mut thirty = flexible_daily("09:00", "17:30");
        thirty.date = "26-07-02 星期四".to_owned();

        let exception = calculate_exceptions(
            "10001",
            "测试员工",
            "研发部",
            &[&twenty_nine, &thirty],
            &[],
            &[],
            ExceptionOptions::default(),
        )
        .0
        .expect("29 分钟和 30 分钟均应进入异常表");
        assert_eq!(exception.late_or_early_11_to_30, 1);
        assert_eq!(exception.late_or_early_30_to_120_minutes, 30);
        assert_eq!(exception.score, 3.0);
    }

    #[test]
    fn attendance_meal_includes_1000_and_excludes_travel_and_home_office() {
        let at_boundary = meal_daily("10:00", "17:30", 0.0);
        assert!(workday_attendance_meal("", punch_range(&at_boundary)));

        let after_boundary = meal_daily("10:01", "17:30", 0.0);
        assert!(!workday_attendance_meal("", punch_range(&after_boundary)));
        assert!(!workday_attendance_meal(
            "出差1天",
            punch_range(&at_boundary)
        ));
        assert!(!workday_attendance_meal(
            "居家办公",
            punch_range(&at_boundary)
        ));
    }

    #[test]
    fn regular_workday_meal_requires_approval_and_uses_shorter_duration() {
        let no_approval = meal_daily("08:30", "19:30", 0.0);
        assert_eq!(
            workday_overtime_meals(&no_approval, punch_range(&no_approval), None, false, false),
            0
        );

        let insufficient_approval = meal_daily("08:30", "24:00", 1.5);
        assert_eq!(
            workday_overtime_meals(
                &insufficient_approval,
                punch_range(&insufficient_approval),
                None,
                false,
                false
            ),
            0
        );

        let one_meal = meal_daily("08:30", "19:30", 2.0);
        assert_eq!(
            workday_overtime_meals(&one_meal, punch_range(&one_meal), None, false, false),
            1
        );

        let two_meals = meal_daily("08:30", "次日 00:00", 6.5);
        assert_eq!(
            workday_overtime_meals(&two_meals, punch_range(&two_meals), None, false, false),
            2
        );
    }

    #[test]
    fn workday_meal_uses_application_start_and_actual_departure() {
        let result = "正常,加班08-03 18:00到08-03 19:30 1.5小时";
        for (departure, expected) in [("19:29", 0), ("19:30", 1), ("19:44", 1)] {
            let daily = meal_daily("08:25", departure, 1.5);
            assert_eq!(
                workday_overtime_meals(
                    &daily,
                    punch_range(&daily),
                    overtime_application_ranges(result, 2026, 8, 3).as_deref(),
                    false,
                    false
                ),
                expected,
                "{departure}"
            );
        }
        let daily = meal_daily("08:25", "19:44", 1.5);
        assert_eq!(
            workday_overtime_meals(
                &daily,
                punch_range(&daily),
                overtime_application_ranges(
                    "正常,加班08-03 20:00到08-03 21:30 1.5小时",
                    2026,
                    8,
                    3
                )
                .as_deref(),
                false,
                false
            ),
            0
        );
    }

    #[test]
    fn august_meals_count_seven_weekdays_and_four_saturdays() {
        let mut monthly = empty_monthly_record();
        monthly.daily_results = vec![String::new(); 31];
        let mut daily = Vec::new();
        for (day, arrival, departure, hours) in [
            (1, "08:23", "17:47", 8.0),
            (3, "08:25", "19:44", 1.5),
            (4, "08:24", "19:58", 1.5),
            (5, "08:26", "19:46", 1.5),
            (6, "08:24", "19:53", 1.5),
            (8, "08:28", "17:44", 8.0),
            (10, "08:25", "19:51", 1.5),
            (11, "08:27", "19:44", 1.5),
            (15, "08:26", "17:43", 8.0),
            (17, "08:25", "19:39", 1.5),
            (29, "08:16", "17:39", 8.0),
        ] {
            monthly.daily_results[day - 1] = if hours == 1.5 {
                format!("正常,加班08-{day:02} 18:00到08-{day:02} 19:30 1.5小时")
            } else {
                format!("休息并打卡,加班08-{day:02} 08:30到08-{day:02} 17:30 8小时")
            };
            daily.push(overtime_daily_with_punches(
                &format!("26-08-{day:02}"),
                arrival,
                departure,
                hours,
            ));
        }
        assert_eq!(
            calculate_meal_allowance(
                &monthly,
                &daily.iter().collect::<Vec<_>>(),
                2026,
                8,
                MealPolicy::Regular,
                false,
                None,
            ),
            (7.0, 11.0)
        );
    }

    #[test]
    fn off_day_afternoon_meal_requires_two_hours_and_1930_departure() {
        for punch_only in [false, true] {
            for (departure, expected) in [(17 * 60 + 30, 1), (19 * 60 + 29, 1), (19 * 60 + 30, 2)] {
                assert_eq!(
                    off_day_overtime_meals(Some((8 * 60 + 30, departure)), 8.0, punch_only),
                    expected
                );
            }
            assert_eq!(
                off_day_overtime_meals(Some((17 * 60 + 31, 19 * 60 + 30)), 2.0, punch_only),
                0
            );
            assert_eq!(
                off_day_overtime_meals(Some((17 * 60 + 30, 19 * 60 + 30)), 2.0, punch_only),
                1
            );
        }
    }

    #[test]
    fn punch_only_workday_meal_ignores_approval() {
        let daily = meal_daily("08:30", "次日 00:00", 0.0);
        assert_eq!(
            workday_overtime_meals(&daily, punch_range(&daily), None, true, false),
            2
        );
    }

    #[test]
    fn off_day_segments_each_require_two_hours_and_exclude_lunch() {
        assert_eq!(
            off_day_overtime_meals(Some((10 * 60, 21 * 60 + 30)), 20.0, false),
            3
        );
        assert_eq!(
            off_day_overtime_meals(Some((10 * 60 + 1, 21 * 60 + 29)), 20.0, false),
            1
        );
        assert_eq!(
            off_day_overtime_meals(Some((11 * 60, 14 * 60)), 20.0, false),
            0
        );
    }

    #[test]
    fn off_day_approval_caps_effective_time_chronologically() {
        let punches = Some((10 * 60, 21 * 60 + 30));
        assert_eq!(off_day_overtime_meals(punches, 0.0, false), 0);
        assert_eq!(off_day_overtime_meals(punches, 2.0, false), 1);
        assert_eq!(off_day_overtime_meals(punches, 0.0, true), 3);
    }

    #[test]
    fn no_punch_policy_counts_calendar_workdays_without_daily_records() {
        let mut monthly = crate::model::MonthlyRecord {
            employee_key: "25182".to_owned(),
            employee_no: "25182".to_owned(),
            name: "欧智元".to_owned(),
            user_id: String::new(),
            attendance_group: String::new(),
            department: String::new(),
            position: String::new(),
            attendance_days: 0.0,
            weekday_overtime_hours: 0.0,
            weekend_overtime_hours: 0.0,
            holiday_overtime_hours: 0.0,
            personal_leave_hours: 0.0,
            compensatory_leave_hours: 0.0,
            sick_leave_hours: 0.0,
            annual_leave_hours: 0.0,
            maternity_leave_days: 0.0,
            paternity_leave_days: 0.0,
            marriage_leave_days: 0.0,
            menstrual_leave_days: 0.0,
            bereavement_leave_days: 0.0,
            breastfeeding_leave_hours: 0.0,
            leave_summary_present: Default::default(),
            daily_results: vec![],
        };
        assert_eq!(
            calculate_meal_allowance(
                &monthly,
                &[],
                2026,
                7,
                MealPolicy::ScheduledWithoutPunch,
                false,
                None,
            ),
            (23.0, 0.0)
        );

        let employment = EmploymentRecord {
            employee_no: "25182".to_owned(),
            name: "欧智元".to_owned(),
            company: "JSFAE".to_owned(),
            hire_date: Some(CalendarDate {
                year: 2026,
                month: 7,
                day: 20,
            }),
            termination_date: None,
        };
        assert_eq!(
            calculate_meal_allowance(
                &monthly,
                &[],
                2026,
                7,
                MealPolicy::Regular,
                false,
                Some(&employment),
            ),
            (1.0, 0.0),
            "入职当天即使没有有效打卡记录也应记出勤餐补"
        );
        assert_eq!(
            calculate_meal_allowance(
                &monthly,
                &[],
                2026,
                7,
                MealPolicy::ScheduledWithoutPunch,
                false,
                Some(&employment),
            ),
            (10.0, 0.0),
            "不打卡人员应按在职区间内的法定工作日记出勤餐补"
        );
        assert!(!is_active_day(Some(&employment), 2026, 7, 19));
        assert!(is_active_day(Some(&employment), 2026, 7, 20));

        monthly.personal_leave_hours = 8.0;
        monthly.leave_summary_present.personal = true;
        monthly.weekday_overtime_hours = 4.0;
        monthly.weekend_overtime_hours = 6.0;
        monthly.holiday_overtime_hours = 8.0;
        let dataset = AttendanceDataset {
            period: crate::model::AttendancePeriod {
                year: 2026,
                month: 7,
            },
            monthly: vec![monthly],
            daily: vec![],
            invalid_punches: vec![],
            employment_records: vec![employment],
            annual_leave_records: vec![],
        };
        let report = calculate_attendance_with_config(
            &dataset,
            &AttendanceConfig {
                special_personnel: crate::config::SpecialPersonnelConfig {
                    no_punch_meal_no_overtime: vec![crate::config::SpecialPerson {
                        employee_no: "25182".to_owned(),
                        name: "欧智元".to_owned(),
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let summary = &report.summary_rows[0];
        assert_eq!(summary.expected_attendance_hours, 80.0);
        assert_eq!(summary.actual_attendance_hours, Some(72.0));
        assert_eq!(summary.attendance_meal_count, 10.0);
        assert_eq!(summary.weekday_overtime_hours, 0.0);
        assert_eq!(summary.weekend_overtime_hours, 0.0);
        assert_eq!(summary.holiday_overtime_hours, 0.0);
    }
}
