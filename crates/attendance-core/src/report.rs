use std::path::Path;

use rust_xlsxwriter::{
    Color, Format, FormatAlign, FormatBorder, Formula, Workbook, Worksheet, XlsxError,
};

use crate::calculate::{AttendanceReport, DetailRow, ExceptionRow, SummaryRow, days_in_month};
use crate::holiday;

const DETAIL_SUMMARY_HEADERS: [&str; 18] = [
    "餐补",
    "餐补次数",
    "出差天数",
    "平时加班",
    "周末加班",
    "法定加班",
    "本应出勤",
    "实际出勤",
    "年休假",
    "年假剩余小时",
    "病假",
    "事假",
    "哺乳假",
    "婚假",
    "产假",
    "丧假",
    "育儿假",
    "旷工",
];

const SUMMARY_HEADERS: [&str; 20] = [
    "序号",
    "工号",
    "姓名",
    "餐补\n次数",
    "出差天数",
    "平时加班（小时）",
    "周末加班（小时）",
    "法定加班（小时）",
    "本应出勤（小时）",
    "实际出勤（小时）",
    "年休假（小时）",
    "年假剩余\n（小时）",
    "病假（小时）",
    "事假（小时）",
    "哺乳假（小时）",
    "婚假（小时）",
    "产假（小时）",
    "丧假（小时）",
    "育儿假（小时）",
    "旷工（小时）",
];

const EXCEPTION_HEADERS: [&str; 10] = [
    "序号",
    "姓名",
    "未签到（次）",
    "未签退（次）",
    "迟到/早退10分钟以内（次）",
    "迟到/早退11-30分钟（次）",
    "迟到/早退30-120分钟（分钟）",
    "不在范围内打卡（次）",
    "绩效扣除分数",
    "异常说明",
];

pub fn generate_report_skeleton(
    output_path: impl AsRef<Path>,
    year: u16,
    month: u8,
) -> Result<(), XlsxError> {
    generate_attendance_report(
        &AttendanceReport {
            detail_rows: Vec::new(),
            summary_rows: Vec::new(),
            exception_rows: Vec::new(),
            warnings: Vec::new(),
        },
        output_path,
        year,
        month,
    )
}

pub fn generate_attendance_report(
    report: &AttendanceReport,
    output_path: impl AsRef<Path>,
    year: u16,
    month: u8,
) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    let title = Format::new()
        .set_bold()
        .set_font_size(15)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let header = Format::new()
        .set_bold()
        .set_text_wrap()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_background_color(Color::RGB(0xFFF2CC));
    let body_text = Format::new()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    let body_number = body_text.clone().set_num_format("General");
    let body_note = body_text
        .clone()
        .set_text_wrap()
        .set_align(FormatAlign::Left);
    let pending = body_number
        .clone()
        .set_background_color(Color::RGB(0xFFF2CC));

    write_detail_sheet(&mut workbook, report, year, month)?;

    {
        let sheet = workbook.add_worksheet().set_name("考勤汇总表")?;
        sheet.set_row_height(0, 28)?;
        sheet.set_row_height(1, 42)?;
        sheet.merge_range(
            0,
            0,
            0,
            19,
            &format!("江苏福拉特{year}年{month}月考勤汇总表"),
            &title,
        )?;
        for (column, value) in SUMMARY_HEADERS.iter().enumerate() {
            sheet.write_string_with_format(1, column as u16, *value, &header)?;
            let width = match column {
                0 => 6.0,
                1 | 2 => 11.0,
                _ => 14.0,
            };
            sheet.set_column_width(column as u16, width)?;
        }
        sheet.set_freeze_panes(2, 3)?;
        for (index, row) in report.summary_rows.iter().enumerate() {
            write_summary_row(
                sheet,
                (index + 2) as u32,
                index + 1,
                row,
                &body_text,
                &body_number,
                &pending,
            )?;
        }
    }

    {
        let sheet = workbook.add_worksheet().set_name("异常打卡明细")?;
        sheet.set_row_height(0, 28)?;
        sheet.set_row_height(1, 42)?;
        sheet.merge_range(0, 0, 0, 9, &format!("{month}月份打卡异常明细表"), &title)?;
        for (column, value) in EXCEPTION_HEADERS.iter().enumerate() {
            sheet.write_string_with_format(1, column as u16, *value, &header)?;
            let width = match column {
                0 => 6.0,
                1 => 11.0,
                9 => 42.0,
                _ => 15.0,
            };
            sheet.set_column_width(column as u16, width)?;
        }
        sheet.set_freeze_panes(2, 2)?;
        for (index, row) in report.exception_rows.iter().enumerate() {
            write_exception_row(
                sheet,
                (index + 2) as u32,
                index + 1,
                row,
                &body_text,
                &body_number,
                &body_note,
            )?;
        }
    }

    workbook.save(output_path)
}

fn write_detail_sheet(
    workbook: &mut Workbook,
    report: &AttendanceReport,
    year: u16,
    month: u8,
) -> Result<(), XlsxError> {
    let day_count = days_in_month(year, month);
    let summary_start = 5 + day_count as u16;
    let last_column = summary_start + DETAIL_SUMMARY_HEADERS.len() as u16 - 1;
    let title = Format::new()
        .set_font_name("宋体")
        .set_bold()
        .set_font_size(16)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let period = Format::new()
        .set_font_name("宋体")
        .set_font_size(11)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let header = Format::new()
        .set_font_name("宋体")
        .set_bold()
        .set_text_wrap()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    let weekend_header = header.clone().set_background_color(Color::RGB(0xD9D9D9));
    let body = Format::new()
        .set_font_name("宋体")
        .set_font_size(9)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    let weekend_body = body.clone().set_background_color(Color::RGB(0xD9D9D9));
    let pending = body.clone().set_background_color(Color::RGB(0xFFF2CC));
    let note = Format::new().set_font_name("宋体").set_font_size(10);

    let sheet = workbook.add_worksheet().set_name("考勤明细")?;
    sheet.set_row_height(0, 28)?;
    sheet.set_row_height(1, 22)?;
    sheet.set_row_height(2, 25)?;
    sheet.set_row_height(3, 22)?;
    sheet.merge_range(0, 0, 0, last_column, "考勤表", &title)?;
    sheet.merge_range(
        1,
        0,
        1,
        last_column,
        &format!("所属期 {year}年{month}月"),
        &period,
    )?;

    for (column, value, width) in [
        (0_u16, "序号", 6.0),
        (1, "所属公司", 10.0),
        (2, "姓名", 10.0),
        (3, "工号", 10.0),
        (4, "", 6.0),
    ] {
        sheet.merge_range(2, column, 3, column, value, &header)?;
        sheet.set_column_width(column, width)?;
    }
    for day in 1..=day_count {
        let column = 4 + day as u16;
        let off_day = is_off_day(year, month, day as u8);
        let day_format = if off_day { &weekend_header } else { &header };
        sheet.write_number_with_format(2, column, day as f64, day_format)?;
        sheet.write_string_with_format(
            3,
            column,
            weekday_label(year, month, day as u8),
            day_format,
        )?;
        sheet.set_column_width(column, 3.6)?;
    }
    for (offset, value) in DETAIL_SUMMARY_HEADERS.iter().enumerate() {
        let column = summary_start + offset as u16;
        sheet.merge_range(2, column, 3, column, *value, &header)?;
        sheet.set_column_width(column, if offset == 9 { 12.0 } else { 9.0 })?;
    }
    sheet.set_freeze_panes(4, 5)?;

    for (index, row) in report.detail_rows.iter().enumerate() {
        let attendance_row = 4 + index as u32 * 2;
        let overtime_row = attendance_row + 1;
        sheet.set_row_height(attendance_row, 19)?;
        sheet.set_row_height(overtime_row, 19)?;
        sheet.merge_range(
            attendance_row,
            0,
            overtime_row,
            0,
            &(index + 1).to_string(),
            &body,
        )?;
        sheet.merge_range(attendance_row, 1, overtime_row, 1, &row.company, &body)?;
        sheet.merge_range(attendance_row, 2, overtime_row, 2, &row.name, &body)?;
        sheet.merge_range(attendance_row, 3, overtime_row, 3, &row.employee_no, &body)?;
        sheet.write_string_with_format(attendance_row, 4, "出勤", &body)?;
        sheet.write_string_with_format(overtime_row, 4, "加班", &body)?;

        for day_index in 0..day_count {
            let column = 5 + day_index as u16;
            let day = (day_index + 1) as u8;
            let off_day = if row.works_saturdays {
                !holiday::is_six_day_workday(year, month, day)
            } else {
                is_off_day(year, month, day)
            };
            let day_format = if off_day { &weekend_body } else { &body };
            if let Some(day) = row.days.get(day_index) {
                if day.attendance.is_empty() {
                    sheet.write_blank(attendance_row, column, day_format)?;
                } else {
                    sheet.write_string_with_format(
                        attendance_row,
                        column,
                        &day.attendance,
                        day_format,
                    )?;
                }
                write_number_or_blank(sheet, overtime_row, column, day.overtime_hours, day_format)?;
            } else {
                sheet.write_blank(attendance_row, column, day_format)?;
                sheet.write_blank(overtime_row, column, day_format)?;
            }
        }
        write_detail_totals(
            sheet,
            attendance_row,
            overtime_row,
            summary_start,
            row,
            &body,
            &pending,
        )?;
    }

    let note_row = 5 + report.detail_rows.len() as u32 * 2;
    for (offset, text) in [
        "说明：1.通用考勤符号：出勤：√ 休息：☆ 迟到：D 早退：Z 病假：△ 事假：O 旷工：X 年休假：N 婚假：H 产假/陪产假：M 丧假：S 出差：C 育儿假：Y 哺乳假：B 产检假：P 调休：T",
        "2.迟到、早退以分钟为单位，请假和加班以小时为单位。",
        "3.单独的“√”代表正常出勤；加班写在加班行；请假符号后的数字为当日小时数。",
        "4.公休日使用灰色底纹标记。",
    ]
    .iter()
    .enumerate()
    {
        sheet.merge_range(
            note_row + offset as u32,
            0,
            note_row + offset as u32,
            last_column,
            *text,
            &note,
        )?;
    }
    Ok(())
}

fn write_detail_totals(
    sheet: &mut Worksheet,
    attendance_row: u32,
    overtime_row: u32,
    start: u16,
    row: &DetailRow,
    body: &Format,
    pending: &Format,
) -> Result<(), XlsxError> {
    for offset in 0..DETAIL_SUMMARY_HEADERS.len() as u16 {
        sheet.write_blank(attendance_row, start + offset, body)?;
        sheet.write_blank(overtime_row, start + offset, body)?;
    }
    sheet.write_number_with_format(
        attendance_row,
        start,
        row.summary.attendance_meal_count,
        body,
    )?;
    sheet.write_number_with_format(overtime_row, start, row.summary.overtime_meal_count, body)?;
    write_optional_number(
        sheet,
        attendance_row,
        start + 1,
        row.summary.meal_allowance_count,
        body,
        pending,
    )?;
    write_number_or_blank(
        sheet,
        attendance_row,
        start + 2,
        row.summary.travel_days,
        body,
    )?;
    write_number_or_blank(
        sheet,
        overtime_row,
        start + 3,
        row.summary.weekday_overtime_hours,
        body,
    )?;
    write_number_or_blank(
        sheet,
        overtime_row,
        start + 4,
        row.summary.weekend_overtime_hours,
        body,
    )?;
    write_number_or_blank(
        sheet,
        overtime_row,
        start + 5,
        row.summary.holiday_overtime_hours,
        body,
    )?;
    write_number_or_blank(
        sheet,
        attendance_row,
        start + 6,
        row.summary.expected_attendance_hours,
        body,
    )?;
    write_optional_number(
        sheet,
        attendance_row,
        start + 7,
        row.summary.actual_attendance_hours,
        body,
        pending,
    )?;
    write_number_or_blank(
        sheet,
        attendance_row,
        start + 8,
        row.summary.annual_leave_hours,
        body,
    )?;
    write_optional_number(
        sheet,
        attendance_row,
        start + 9,
        row.summary.annual_leave_balance_hours,
        body,
        pending,
    )?;
    for (offset, value) in [
        row.summary.sick_leave_hours,
        row.summary.personal_leave_hours,
        row.summary.breastfeeding_leave_hours,
        row.summary.marriage_leave_hours,
        row.summary.maternity_leave_hours,
        row.summary.bereavement_leave_hours,
        row.summary.childcare_leave_hours,
        row.summary.absent_hours,
    ]
    .into_iter()
    .enumerate()
    {
        write_number_or_blank(
            sheet,
            attendance_row,
            start + 10 + offset as u16,
            value,
            body,
        )?;
    }
    Ok(())
}

fn is_off_day(year: u16, month: u8, day: u8) -> bool {
    holiday::is_off_day(year, month, day)
}

fn weekday_label(year: u16, month: u8, day: u8) -> &'static str {
    ["日", "一", "二", "三", "四", "五", "六"][weekday_index(year, month, day)]
}

fn weekday_index(year: u16, month: u8, day: u8) -> usize {
    let offsets = [0_i32, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut year = year as i32;
    if month < 3 {
        year -= 1;
    }
    (year + year / 4 - year / 100 + year / 400 + offsets[month as usize - 1] + day as i32)
        .rem_euclid(7) as usize
}

fn write_summary_row(
    sheet: &mut Worksheet,
    excel_row: u32,
    serial: usize,
    row: &SummaryRow,
    text: &Format,
    number: &Format,
    pending: &Format,
) -> Result<(), XlsxError> {
    sheet.write_number_with_format(excel_row, 0, serial as f64, number)?;
    sheet.write_string_with_format(excel_row, 1, &row.employee_no, text)?;
    sheet.write_string_with_format(excel_row, 2, &row.name, text)?;
    write_optional_number(
        sheet,
        excel_row,
        3,
        row.meal_allowance_count,
        number,
        pending,
    )?;
    write_number_or_blank(sheet, excel_row, 4, row.travel_days, number)?;
    write_number_or_blank(sheet, excel_row, 5, row.weekday_overtime_hours, number)?;
    write_number_or_blank(sheet, excel_row, 6, row.weekend_overtime_hours, number)?;
    write_number_or_blank(sheet, excel_row, 7, row.holiday_overtime_hours, number)?;
    write_number_or_blank(sheet, excel_row, 8, row.expected_attendance_hours, number)?;
    write_optional_number(
        sheet,
        excel_row,
        9,
        row.actual_attendance_hours,
        number,
        pending,
    )?;
    write_number_or_blank(sheet, excel_row, 10, row.annual_leave_hours, number)?;
    write_optional_number(
        sheet,
        excel_row,
        11,
        row.annual_leave_balance_hours,
        number,
        pending,
    )?;
    write_number_or_blank(sheet, excel_row, 12, row.sick_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 13, row.personal_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 14, row.breastfeeding_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 15, row.marriage_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 16, row.maternity_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 17, row.bereavement_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 18, row.childcare_leave_hours, number)?;
    write_number_or_blank(sheet, excel_row, 19, row.absent_hours, number)?;
    Ok(())
}

fn write_exception_row(
    sheet: &mut Worksheet,
    excel_row: u32,
    serial: usize,
    row: &ExceptionRow,
    text: &Format,
    number: &Format,
    note: &Format,
) -> Result<(), XlsxError> {
    sheet.write_number_with_format(excel_row, 0, serial as f64, number)?;
    sheet.write_string_with_format(excel_row, 1, &row.name, text)?;
    for (column, value) in [
        row.missing_in,
        row.missing_out,
        row.late_or_early_under_10,
        row.late_or_early_11_to_30,
        row.late_or_early_30_to_120_minutes,
        row.out_of_range,
    ]
    .iter()
    .enumerate()
    {
        write_number_or_blank(sheet, excel_row, (column + 2) as u16, *value as f64, number)?;
    }
    let worksheet_row = excel_row + 1;
    let formula = Formula::new(format!(
        "=(C{worksheet_row}+D{worksheet_row}+E{worksheet_row})*1+F{worksheet_row}*2+(CEILING(G{worksheet_row},30)/30)+H{worksheet_row}*1"
    ))
    .set_result(row.score.to_string());
    sheet.write_formula_with_format(excel_row, 8, formula, number)?;
    let notes = row.notes.join("、");
    if notes.chars().count() > 80 {
        sheet.set_row_height(excel_row, 45)?;
    } else if notes.chars().count() > 40 {
        sheet.set_row_height(excel_row, 30)?;
    }
    sheet.write_string_with_format(excel_row, 9, notes, note)?;
    Ok(())
}

fn write_number_or_blank(
    sheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: f64,
    format: &Format,
) -> Result<(), XlsxError> {
    if value == 0.0 {
        sheet.write_blank(row, column, format)?;
    } else {
        sheet.write_number_with_format(row, column, value, format)?;
    }
    Ok(())
}

fn write_optional_number(
    sheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: Option<f64>,
    value_format: &Format,
    blank_format: &Format,
) -> Result<(), XlsxError> {
    match value {
        Some(value) => {
            sheet.write_number_with_format(row, column, value, value_format)?;
        }
        None => {
            sheet.write_blank(row, column, blank_format)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_have_expected_width() {
        assert_eq!(SUMMARY_HEADERS.len(), 20);
        assert_eq!(EXCEPTION_HEADERS.len(), 10);
    }
}
