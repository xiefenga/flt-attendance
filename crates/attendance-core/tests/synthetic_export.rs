use std::collections::HashMap;
use std::path::PathBuf;

use attendance_core::{
    AttendanceConfig, AttendancePeriod, AttendanceReport, DetailRow, SpecialPerson,
    SpecialPersonnelConfig, apply_company_history, calculate_attendance,
    calculate_attendance_with_config, inspect_dingtalk, load_dingtalk,
};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples/input")
        .join("钉钉考勤报表示例_202607.xlsx")
}

fn detail<'a>(report: &'a AttendanceReport, employee_no: &str) -> &'a DetailRow {
    report
        .detail_rows
        .iter()
        .find(|row| row.employee_no == employee_no)
        .expect("synthetic employee must be present")
}

fn person(employee_no: &str) -> SpecialPerson {
    SpecialPerson {
        employee_no: employee_no.to_owned(),
        name: String::new(),
    }
}

#[test]
fn parses_only_synthetic_employee_records() {
    let inspected = inspect_dingtalk(fixture_path()).unwrap();
    let dataset = load_dingtalk(fixture_path()).unwrap();
    assert_eq!(
        dataset.period,
        AttendancePeriod {
            year: 2026,
            month: 7
        }
    );
    assert_eq!(inspected.period, dataset.period);
    assert_eq!(dataset.monthly.len(), 12);
    assert_eq!(dataset.daily.len(), 372);
    assert_eq!(dataset.employment_records.len(), 2);
    assert_eq!(dataset.annual_leave_records.len(), 12);
    assert_eq!(dataset.invalid_punches.len(), 1);
    assert_eq!(inspected.employees.len(), 12);
    assert_eq!(inspected.sheet("月度汇总").unwrap().data_rows, 12);
    assert_eq!(inspected.sheet("每日统计").unwrap().data_rows, 372);
    assert_eq!(inspected.sheet("年假明细").unwrap().data_rows, 12);
    let identities: HashMap<_, _> = dataset
        .monthly
        .iter()
        .map(|row| (row.employee_no.as_str(), row.name.as_str()))
        .collect();
    for index in 1..=12 {
        let employee_no = format!("SYN{index:03}");
        assert_eq!(
            identities.get(employee_no.as_str()).copied(),
            Some(format!("合成员工{index:02}").as_str())
        );
        let monthly = dataset
            .monthly
            .iter()
            .find(|row| row.employee_no == employee_no)
            .unwrap();
        assert_eq!(monthly.user_id, format!("synthetic-user-{index:03}"));
        assert_eq!(
            dataset
                .daily
                .iter()
                .filter(|row| row.employee_no == employee_no)
                .count(),
            31
        );
    }
    let all_identities = dataset
        .daily
        .iter()
        .map(|row| (&row.employee_no, &row.name))
        .chain(
            dataset
                .employment_records
                .iter()
                .map(|row| (&row.employee_no, &row.name)),
        )
        .chain(
            dataset
                .annual_leave_records
                .iter()
                .map(|row| (&row.employee_no, &row.name)),
        )
        .chain(
            dataset
                .invalid_punches
                .iter()
                .map(|row| (&row.employee_no, &row.name)),
        )
        .chain(
            inspected
                .employees
                .iter()
                .map(|row| (&row.employee_no, &row.name)),
        );
    for (employee_no, name) in all_identities {
        assert_eq!(identities.get(employee_no.as_str()).copied(), Some(name.as_str()));
    }
    assert!(
        dataset
            .annual_leave_records
            .iter()
            .all(|row| row.balance_before_month_hours == 40.0)
    );
    assert_eq!(
        dataset
            .employment_records
            .iter()
            .filter(|row| row.hire_date.is_some())
            .count(),
        1
    );
    assert_eq!(
        dataset
            .employment_records
            .iter()
            .filter(|row| row.termination_date.is_some())
            .count(),
        1
    );
}

#[test]
fn reconciles_synthetic_attendance_scenarios() {
    let dataset = load_dingtalk(fixture_path()).unwrap();
    let report = calculate_attendance(&dataset);
    assert_eq!(report.detail_rows.len(), 12);
    assert_eq!(report.summary_rows.len(), 12);

    let ordinary = detail(&report, "SYN001");
    assert_eq!(ordinary.days[0].attendance, "√");
    assert_eq!(ordinary.days[3].attendance, "☆");
    assert_eq!(ordinary.summary.expected_attendance_hours, 184.0);
    assert_eq!(ordinary.summary.actual_attendance_hours, Some(184.0));
    assert_eq!(ordinary.summary.attendance_meal_count, 23.0);
    assert_eq!(ordinary.summary.annual_leave_balance_hours, Some(40.0));

    let overtime = detail(&report, "SYN002");
    assert_eq!(overtime.days[0].overtime_hours, 2.5);
    assert_eq!(overtime.days[3].overtime_hours, 8.0);
    assert_eq!(overtime.summary.weekday_overtime_hours, 2.5);
    assert_eq!(overtime.summary.weekend_overtime_hours, 8.0);
    assert_eq!(overtime.summary.actual_attendance_hours, Some(194.5));
    assert_eq!(overtime.summary.overtime_meal_count, 2.0);

    let leave = detail(&report, "SYN003");
    for (day, expected) in [
        (6, "N8"),
        (7, "Y8"),
        (8, "Y8"),
        (9, "Y8"),
        (10, "Y8"),
        (13, "O8"),
        (14, "△8"),
        (15, "T8"),
    ] {
        assert_eq!(leave.days[day - 1].attendance, expected);
    }
    assert_eq!(leave.summary.annual_leave_hours, 8.0);
    assert_eq!(leave.summary.annual_leave_balance_hours, Some(32.0));
    assert_eq!(leave.summary.childcare_leave_hours, 32.0);
    assert_eq!(leave.summary.personal_leave_hours, 8.0);
    assert_eq!(leave.summary.sick_leave_hours, 8.0);
    assert_eq!(leave.summary.expected_attendance_hours, 184.0);
    assert_eq!(leave.summary.actual_attendance_hours, Some(120.0));

    let hire = detail(&report, "SYN004");
    assert!(
        hire.days[..19]
            .iter()
            .all(|day| day.attendance.is_empty())
    );
    assert_eq!(hire.days[19].attendance, "√");
    assert_eq!(hire.days[19].attendance_meal_count, 1.0);
    assert_eq!(hire.summary.expected_attendance_hours, 80.0);
    assert_eq!(hire.summary.attendance_meal_count, 10.0);
    assert!(
        !report
            .exception_rows
            .iter()
            .any(|row| row.employee_no == "SYN004")
    );
    let terminated = detail(&report, "SYN005");
    assert_eq!(terminated.days[14].attendance, "√");
    assert!(
        terminated.days[15..]
            .iter()
            .all(|day| day.attendance.is_empty())
    );
    assert_eq!(terminated.summary.expected_attendance_hours, 88.0);
    assert_eq!(terminated.summary.attendance_meal_count, 11.0);

    let travel = detail(&report, "SYN006");
    assert_eq!(travel.summary.travel_days, 2.0);
    assert_eq!(travel.summary.attendance_meal_count, 21.0);
    assert_eq!(travel.days[22].attendance, "C缺卡");
    assert_eq!(travel.days[23].attendance, "C");
    let travel_exception = report
        .exception_rows
        .iter()
        .find(|row| row.employee_no == "SYN006")
        .unwrap();
    assert_eq!((travel_exception.missing_in, travel_exception.missing_out), (0, 1));
    assert_eq!(travel_exception.score, 1.0);
    assert!(travel_exception.notes.contains(&"7.23下班未签退".to_owned()));

    let missing = detail(&report, "SYN007");
    assert_eq!(missing.days[7].attendance, "√缺卡");
    assert_eq!(missing.days[8].attendance, "√缺卡");
    assert_eq!(missing.days[8].attendance_meal_count, 0.0);
    let missing_exception = report
        .exception_rows
        .iter()
        .find(|row| row.employee_no == "SYN007")
        .unwrap();
    assert_eq!((missing_exception.missing_in, missing_exception.missing_out), (1, 2));
    assert_eq!(missing_exception.out_of_range, 1);
    assert_eq!(missing_exception.score, 4.0);
    assert!(missing_exception.notes.contains(&"7.9不在范围内打卡".to_owned()));

    let late = detail(&report, "SYN008");
    for (day, minutes) in [(20, 8), (21, 10), (22, 46)] {
        assert_eq!(late.days[day - 1].attendance, format!("√D{minutes}"));
    }
    let late_exception = report
        .exception_rows
        .iter()
        .find(|row| row.employee_no == "SYN008")
        .unwrap();
    assert_eq!(late_exception.late_or_early_under_10, 2);
    assert_eq!(late_exception.late_or_early_11_to_30, 0);
    assert_eq!(late_exception.late_or_early_30_to_120_minutes, 46);
    assert_eq!(late_exception.score, 4.0);
    let home = detail(&report, "SYN011");
    assert_eq!(home.days[15].attendance_meal_count, 0.0);
    assert_eq!(home.summary.attendance_meal_count, 22.0);
    assert_eq!(home.summary.actual_attendance_hours, Some(184.0));
    assert_eq!(detail(&report, "SYN012").summary.attendance_meal_count, 0.0);

    for row in &report.detail_rows {
        assert_eq!(row.days.len(), 31);
        assert_eq!(
            row.days
                .iter()
                .map(|day| day.attendance_meal_count)
                .sum::<f64>(),
            row.summary.attendance_meal_count
        );
        assert_eq!(
            row.days
                .iter()
                .map(|day| day.overtime_meal_count)
                .sum::<f64>(),
            row.summary.overtime_meal_count
        );
        assert_eq!(
            row.summary.meal_allowance_count,
            Some(row.summary.attendance_meal_count + row.summary.overtime_meal_count)
        );
        let overtime = row.summary.weekday_overtime_hours
            + row.summary.weekend_overtime_hours
            + row.summary.holiday_overtime_hours;
        assert_eq!(
            row.days
                .iter()
                .map(|day| day.overtime_hours)
                .sum::<f64>(),
            overtime
        );
        assert_eq!(
            row.days
                .iter()
                .filter(|day| day.attendance.starts_with('C'))
                .count() as f64,
            row.summary.travel_days
        );
        assert!(
            row.days
                .iter()
                .all(|day| (day.overtime_hours * 2.0).fract() == 0.0)
        );
    }
}

#[test]
fn missing_annual_sheet_leaves_balances_unknown() {
    let mut dataset = load_dingtalk(fixture_path()).unwrap();
    dataset.annual_leave_records.clear();
    let report = calculate_attendance(&dataset);
    assert!(
        report
            .summary_rows
            .iter()
            .all(|row| row.annual_leave_balance_hours.is_none())
    );
    assert_eq!(detail(&report, "SYN003").summary.annual_leave_hours, 8.0);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("未读取到“年假明细”工作表"))
    );
}

#[test]
fn in_memory_company_history_respects_employment_company() {
    let dataset = load_dingtalk(fixture_path()).unwrap();
    let mut report = calculate_attendance(&dataset);
    let ordinary_name = detail(&report, "SYN001").name.clone();
    let hire_name = detail(&report, "SYN004").name.clone();
    let hire_company = detail(&report, "SYN004").company.clone();
    assert!(detail(&report, "SYN004").company_from_employment);
    let companies = HashMap::from([
        (ordinary_name, "合成历史公司".to_owned()),
        (hire_name, "合成历史公司".to_owned()),
    ]);
    assert_eq!(apply_company_history(&mut report, &companies), 1);
    assert_eq!(detail(&report, "SYN001").company, "合成历史公司");
    assert_eq!(detail(&report, "SYN004").company, hire_company);
}

#[test]
fn applies_special_personnel_and_exclusion_settings() {
    let dataset = load_dingtalk(fixture_path()).unwrap();
    let config = AttendanceConfig {
        special_personnel: SpecialPersonnelConfig {
            no_meal_no_overtime: vec![person("SYN002")],
            no_punch_meal_no_overtime: vec![person("SYN012"), person("SYN006")],
            six_day_no_meal: vec![person("SYN009")],
            six_day_four_hour_no_meal: vec![person("SYN010")],
            ..Default::default()
        },
        ..Default::default()
    };
    let report = calculate_attendance_with_config(&dataset, &config);
    let overtime = detail(&report, "SYN002");
    assert_eq!(overtime.summary.actual_attendance_hours, Some(184.0));
    assert_eq!(overtime.summary.meal_allowance_count, Some(0.0));
    for (employee_no, meals) in [("SYN012", 23.0), ("SYN006", 21.0)] {
        let row = detail(&report, employee_no);
        assert_eq!(row.summary.expected_attendance_hours, 184.0);
        assert_eq!(row.summary.actual_attendance_hours, Some(184.0));
        assert_eq!(row.summary.attendance_meal_count, meals);
    }
    for (employee_no, hours) in [("SYN009", 216.0), ("SYN010", 108.0)] {
        let row = detail(&report, employee_no);
        assert!(row.works_saturdays);
        assert_eq!(row.summary.expected_attendance_hours, hours);
        assert_eq!(row.summary.actual_attendance_hours, Some(hours));
        assert_eq!(row.summary.meal_allowance_count, Some(0.0));
    }
    for employee_no in ["SYN002", "SYN006", "SYN009", "SYN010", "SYN012"] {
        let row = detail(&report, employee_no);
        assert_eq!(row.summary.weekday_overtime_hours, 0.0);
        assert_eq!(row.summary.weekend_overtime_hours, 0.0);
        assert_eq!(row.summary.holiday_overtime_hours, 0.0);
        assert_eq!(row.summary.overtime_meal_count, 0.0);
        assert!(
            row.days
                .iter()
                .all(|day| day.overtime_hours == 0.0)
        );
    }
    assert!(report.warnings.iter().any(|warning| {
        warning.contains("2 人") && warning.contains("周一至周六工作制")
    }));
    // Exclude an employee who has an exception to check all three report sections.
    let excluded = calculate_attendance_with_config(
        &dataset,
        &AttendanceConfig {
            excluded_personnel: vec![person("SYN008")],
            ..Default::default()
        },
    );
    assert_eq!(excluded.summary_rows.len(), 11);
    assert_eq!(excluded.detail_rows.len(), 11);
    assert!(
        !excluded
            .summary_rows
            .iter()
            .any(|row| row.employee_no == "SYN008")
    );
    assert!(
        !excluded
            .detail_rows
            .iter()
            .any(|row| row.employee_no == "SYN008")
    );
    assert!(
        !excluded
            .exception_rows
            .iter()
            .any(|row| row.employee_no == "SYN008")
    );
}

#[test]
fn exported_daily_meal_columns_match_calculation_and_monthly_totals() {
    use calamine::{DataType, Reader, open_workbook_auto};
    let dataset = load_dingtalk(fixture_path()).unwrap();
    let report = calculate_attendance(&dataset);
    let path = std::env::temp_dir().join(format!(
        "flt-attendance-daily-meals-{}.xlsx",
        std::process::id()
    ));
    attendance_core::generate_attendance_report(&report, &path, 2026, 7).unwrap();
    let mut workbook = open_workbook_auto(&path).unwrap();
    let detail = workbook.worksheet_range("考勤明细").unwrap();
    let summary = workbook.worksheet_range("考勤汇总表").unwrap();
    assert_eq!(detail.width(), 5 + 31 * 2 + 18);
    for (index, row) in report.detail_rows.iter().enumerate() {
        let attendance_row = 4 + index as u32 * 2;
        for (day_index, day) in row.days.iter().enumerate() {
            let column = 5 + day_index as u32 * 2;
            assert_eq!(
                detail.get_value((2, column)).and_then(DataType::as_f64),
                Some((day_index + 1) as f64)
            );
            assert_eq!(
                detail
                    .get_value((2, column + 1))
                    .and_then(DataType::get_string),
                Some("餐补")
            );
            for (offset, meals) in [(0, day.attendance_meal_count), (1, day.overtime_meal_count)] {
                assert_eq!(
                    detail
                        .get_value((attendance_row + offset, column + 1))
                        .and_then(DataType::as_f64)
                        .unwrap_or(0.0),
                    meals
                );
            }
            assert_eq!(
                detail
                    .get_value((attendance_row + 1, column))
                    .and_then(DataType::as_f64)
                    .unwrap_or(0.0),
                day.overtime_hours
            );
        }
        for (offset, meals) in [
            (0, row.summary.attendance_meal_count),
            (1, row.summary.overtime_meal_count),
        ] {
            assert_eq!(
                detail
                    .get_value((attendance_row + offset, 67))
                    .and_then(DataType::as_f64),
                Some(meals)
            );
        }
        assert_eq!(
            summary
                .get_value((index as u32 + 2, 3))
                .and_then(DataType::as_f64),
            row.summary.meal_allowance_count
        );
    }
    std::fs::remove_file(path).unwrap();
}

