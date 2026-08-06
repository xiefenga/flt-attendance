use std::path::PathBuf;

use attendance_core::{
    AttendanceConfig, AttendancePeriod, SpecialPerson, SpecialPersonnelConfig,
    apply_company_history, calculate_attendance, calculate_attendance_with_config,
    inspect_dingtalk, load_company_history, load_dingtalk,
};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples/input")
        .join("钉钉考勤报表示例_202607.xlsx")
}

fn history_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples/templates/考勤统计表模板.xlsx")
}

#[test]
fn parses_and_reconciles_real_dingtalk_export() {
    let summary = inspect_dingtalk(fixture_path()).expect("真实钉钉导出应可检查");
    assert_eq!(
        summary.period,
        AttendancePeriod {
            year: 2026,
            month: 7
        }
    );

    let dataset = load_dingtalk(fixture_path()).expect("真实钉钉导出应可读取");
    assert_eq!(dataset.period, summary.period);
    assert_eq!(dataset.monthly.len(), 236);
    assert_eq!(dataset.daily.len(), 7_316);
    assert_eq!(dataset.invalid_punches.len(), 16);

    let source_weekday_overtime: f64 = dataset
        .monthly
        .iter()
        .map(|row| row.weekday_overtime_hours)
        .sum();
    let source_weekend_overtime: f64 = dataset
        .monthly
        .iter()
        .map(|row| row.weekend_overtime_hours)
        .sum();
    let mut report = calculate_attendance(&dataset);
    let companies = load_company_history(history_path()).expect("历史考勤表应可读取");
    assert!(apply_company_history(&mut report, &companies) > 150);
    let report_weekday_overtime: f64 = report
        .summary_rows
        .iter()
        .map(|row| row.weekday_overtime_hours)
        .sum();
    let report_weekend_overtime: f64 = report
        .summary_rows
        .iter()
        .map(|row| row.weekend_overtime_hours)
        .sum();

    assert_eq!(report.summary_rows.len(), 236);
    assert_eq!(report.exception_rows.len(), 59);
    assert_eq!(source_weekday_overtime, report_weekday_overtime);
    assert_eq!(source_weekend_overtime, report_weekend_overtime);
    assert!(
        report
            .summary_rows
            .iter()
            .all(|row| row.meal_allowance_count
                == Some(row.attendance_meal_count + row.overtime_meal_count))
    );
    assert!(
        report
            .summary_rows
            .iter()
            .map(|row| row.meal_allowance_count.unwrap_or_default())
            .sum::<f64>()
            > 0.0,
        "真实样例的餐补合计不应全部为空或为零"
    );
    for row in &report.detail_rows {
        let daily_overtime: f64 = row.days.iter().map(|day| day.overtime_hours).sum();
        let summary_overtime = row.summary.weekday_overtime_hours
            + row.summary.weekend_overtime_hours
            + row.summary.holiday_overtime_hours;
        assert!(
            (daily_overtime - summary_overtime).abs() < 0.001,
            "{}逐日加班{daily_overtime}与汇总{summary_overtime}不一致",
            row.name
        );
        let detail_travel_days = row
            .days
            .iter()
            .filter(|day| day.attendance.starts_with('C'))
            .count() as f64;
        assert_eq!(
            detail_travel_days, row.summary.travel_days,
            "{}逐日出差与汇总不一致",
            row.name
        );
    }

    let li_sai = report
        .summary_rows
        .iter()
        .find(|row| row.name == "李赛")
        .expect("应包含李赛");
    assert_eq!(li_sai.childcare_leave_hours, 32.0);

    let li_wenxiang = report
        .summary_rows
        .iter()
        .find(|row| row.name == "李文祥")
        .expect("应包含李文祥");
    assert_eq!(li_wenxiang.expected_attendance_hours, 184.0);
    assert_eq!(li_wenxiang.actual_attendance_hours, Some(309.5));

    let li_wenxiang_detail = report
        .detail_rows
        .iter()
        .find(|row| row.name == "李文祥")
        .expect("明细应包含李文祥");
    assert_eq!(li_wenxiang_detail.company, "烨成");
    assert_eq!(li_wenxiang_detail.days.len(), 31);
    assert_eq!(li_wenxiang_detail.days[0].attendance, "√");
    assert_eq!(li_wenxiang_detail.days[0].overtime_hours, 6.0);
    assert_eq!(li_wenxiang_detail.days[4].attendance, "☆");

    let liu_hanfu = report
        .detail_rows
        .iter()
        .find(|row| row.name == "刘瀚夫")
        .expect("明细应包含刘瀚夫");
    assert_eq!(liu_hanfu.days[13].attendance, "O6");
    assert_eq!(liu_hanfu.days[14].attendance, "O8");

    let chen_wenjie = report
        .exception_rows
        .iter()
        .find(|row| row.name == "陈文杰")
        .expect("应包含陈文杰异常");
    assert_eq!(chen_wenjie.late_or_early_31_to_120_minutes, 120);
    assert_eq!(chen_wenjie.score, 4.0);

    let configured = calculate_attendance_with_config(
        &dataset,
        &AttendanceConfig {
            special_personnel: SpecialPersonnelConfig {
                no_meal_no_overtime: vec![SpecialPerson {
                    employee_no: "25181".to_owned(),
                    name: "李文祥".to_owned(),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let configured_li = configured
        .detail_rows
        .iter()
        .find(|row| row.employee_no == "25181")
        .expect("配置后仍应包含李文祥");
    assert_eq!(configured_li.summary.weekday_overtime_hours, 0.0);
    assert_eq!(configured_li.summary.weekend_overtime_hours, 0.0);
    assert_eq!(configured_li.summary.actual_attendance_hours, Some(184.0));
    assert_eq!(configured_li.summary.attendance_meal_count, 0.0);
    assert_eq!(configured_li.summary.overtime_meal_count, 0.0);
    assert_eq!(configured_li.summary.meal_allowance_count, Some(0.0));
    assert!(
        configured_li
            .days
            .iter()
            .all(|day| day.overtime_hours == 0.0)
    );

    let excluded = calculate_attendance_with_config(
        &dataset,
        &AttendanceConfig {
            excluded_personnel: vec![SpecialPerson {
                employee_no: "25181".to_owned(),
                name: "李文祥".to_owned(),
            }],
            ..Default::default()
        },
    );
    assert_eq!(excluded.summary_rows.len(), 235);
    assert!(
        !excluded
            .summary_rows
            .iter()
            .any(|row| row.employee_no == "25181")
    );
    assert!(
        !excluded
            .detail_rows
            .iter()
            .any(|row| row.employee_no == "25181")
    );
    assert!(
        !excluded
            .exception_rows
            .iter()
            .any(|row| row.employee_no == "25181")
    );
}
