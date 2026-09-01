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
    assert_eq!(dataset.employment_records.len(), 44);
    assert!(dataset.annual_leave_records.is_empty());
    assert!(summary.sheet("年假信息").is_none());
    assert_eq!(
        dataset
            .employment_records
            .iter()
            .filter(|record| record.hire_date.is_some())
            .count(),
        39
    );
    assert_eq!(
        dataset
            .employment_records
            .iter()
            .filter(|record| record.termination_date.is_some())
            .count(),
        5
    );

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
    assert!(report_weekday_overtime <= source_weekday_overtime);
    assert!(report_weekend_overtime <= source_weekend_overtime);
    assert!(
        report
            .summary_rows
            .iter()
            .flat_map(|row| [
                row.weekday_overtime_hours,
                row.weekend_overtime_hours,
                row.holiday_overtime_hours,
            ])
            .all(|hours| (hours * 2.0 - (hours * 2.0).round()).abs() < 0.001),
        "折算后的月度加班必须以半小时为单位"
    );
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
    assert_eq!(li_wenxiang.actual_attendance_hours, Some(308.0));

    let ma_zhen = report
        .summary_rows
        .iter()
        .find(|row| row.name == "马镇")
        .expect("应包含马镇");
    assert_eq!(ma_zhen.annual_leave_hours, 8.0);
    assert_eq!(ma_zhen.annual_leave_balance_hours, None);

    assert!(
        report
            .summary_rows
            .iter()
            .filter(|row| row.name == "李欣")
            .all(|row| row.annual_leave_balance_hours.is_none()),
        "没有年假明细时余额应留空"
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("未读取到“年假明细”工作表"))
    );

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
    assert_eq!(chen_wenjie.late_or_early_30_to_120_minutes, 120);
    assert_eq!(chen_wenjie.score, 4.0);
    let chen_wenjie_summary = report
        .summary_rows
        .iter()
        .find(|row| row.employee_no == "26320")
        .expect("应包含 7 月 20 日入职的陈文杰");
    assert_eq!(chen_wenjie_summary.expected_attendance_hours, 80.0);
    assert_eq!(chen_wenjie_summary.attendance_meal_count, 10.0);

    let zhang_yicheng = report
        .summary_rows
        .iter()
        .find(|row| row.employee_no == "26333")
        .expect("应包含 7 月 31 日入职的张一成");
    assert_eq!(zhang_yicheng.expected_attendance_hours, 8.0);
    assert_eq!(zhang_yicheng.attendance_meal_count, 1.0);

    let configured = calculate_attendance_with_config(
        &dataset,
        &AttendanceConfig {
            special_personnel: SpecialPersonnelConfig {
                no_punch_meal_no_overtime: vec![
                    SpecialPerson {
                        employee_no: "25196".to_owned(),
                        name: "李阳".to_owned(),
                    },
                    SpecialPerson {
                        employee_no: "25182".to_owned(),
                        name: "欧智元".to_owned(),
                    },
                    SpecialPerson {
                        employee_no: "24135".to_owned(),
                        name: "张丽雯".to_owned(),
                    },
                ],
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
    for (employee_no, expected_actual, expected_absent) in [
        ("25196", 136.0, 48.0),
        ("25182", 184.0, 0.0),
        ("24135", 184.0, 0.0),
    ] {
        let summary = configured
            .summary_rows
            .iter()
            .find(|row| row.employee_no == employee_no)
            .expect("应包含不打卡人员");
        assert_eq!(summary.expected_attendance_hours, 184.0);
        assert_eq!(summary.actual_attendance_hours, Some(expected_actual));
        assert_eq!(summary.absent_hours, expected_absent);
        assert_eq!(summary.attendance_meal_count, 23.0);
        assert_eq!(summary.overtime_meal_count, 0.0);
        assert_eq!(summary.weekday_overtime_hours, 0.0);
        assert_eq!(summary.weekend_overtime_hours, 0.0);
        assert_eq!(summary.holiday_overtime_hours, 0.0);
    }

    let six_day_configured = calculate_attendance_with_config(
        &dataset,
        &AttendanceConfig {
            special_personnel: SpecialPersonnelConfig {
                six_day_no_meal: vec![SpecialPerson {
                    employee_no: String::new(),
                    name: "廖传兰".to_owned(),
                }],
                six_day_four_hour_no_meal: vec![SpecialPerson {
                    employee_no: String::new(),
                    name: "廖传霞".to_owned(),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let liao_chuanlan = six_day_configured
        .detail_rows
        .iter()
        .find(|row| row.name == "廖传兰")
        .expect("六天制配置应匹配廖传兰");
    assert!(liao_chuanlan.works_saturdays);
    assert_eq!(liao_chuanlan.summary.expected_attendance_hours, 216.0);
    assert_eq!(liao_chuanlan.summary.meal_allowance_count, Some(0.0));
    assert_eq!(liao_chuanlan.summary.weekday_overtime_hours, 0.0);
    assert_eq!(liao_chuanlan.summary.weekend_overtime_hours, 0.0);
    assert_eq!(liao_chuanlan.summary.holiday_overtime_hours, 0.0);
    assert!(
        liao_chuanlan
            .days
            .iter()
            .all(|day| day.overtime_hours == 0.0)
    );

    let liao_chuanxia = six_day_configured
        .detail_rows
        .iter()
        .find(|row| row.name == "廖传霞")
        .expect("四小时六天制配置应匹配廖传霞");
    assert!(liao_chuanxia.works_saturdays);
    assert_eq!(liao_chuanxia.summary.expected_attendance_hours, 108.0);
    assert_eq!(liao_chuanxia.summary.actual_attendance_hours, Some(108.0));
    assert_eq!(liao_chuanxia.summary.meal_allowance_count, Some(0.0));
    assert_eq!(liao_chuanxia.summary.weekday_overtime_hours, 0.0);
    assert_eq!(liao_chuanxia.summary.weekend_overtime_hours, 0.0);
    assert_eq!(liao_chuanxia.summary.holiday_overtime_hours, 0.0);
    assert!(
        liao_chuanxia
            .days
            .iter()
            .all(|day| day.overtime_hours == 0.0)
    );
    assert!(six_day_configured.warnings.iter().any(|warning| {
        warning.contains("2 人") && warning.contains("周一至周六工作制")
    }));

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
