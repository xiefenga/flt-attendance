use std::env;

use anyhow::{Context, Result, bail};
use attendance_core::{
    apply_company_history, calculate_attendance, generate_attendance_report,
    generate_report_skeleton, inspect_dingtalk, load_company_history, load_dingtalk,
};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("inspect") => {
            let input = args
                .get(2)
                .context("用法：attendance-cli inspect <钉钉导出.xlsx>")?;
            let summary = inspect_dingtalk(input)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Some("generate") => {
            let output = args
                .get(2)
                .context("用法：attendance-cli generate <输出.xlsx> <年份> <月份>")?;
            let year = args
                .get(3)
                .context("缺少年份")?
                .parse::<u16>()
                .context("年份格式错误")?;
            let month = args
                .get(4)
                .context("缺少月份")?
                .parse::<u8>()
                .context("月份格式错误")?;
            if !(1..=12).contains(&month) {
                bail!("月份必须为 1-12");
            }
            generate_report_skeleton(output, year, month)?;
            println!("已生成：{output}");
        }
        Some("calculate") => {
            let input = args
                .get(2)
                .context("用法：attendance-cli calculate <钉钉导出.xlsx> <输出.xlsx>")?;
            let output = args.get(3).context("缺少输出路径")?;
            let dataset = load_dingtalk(input)?;
            let period = dataset.period;
            let mut report = calculate_attendance(&dataset);
            if let Some(history) = args.get(4) {
                let companies = load_company_history(history)?;
                apply_company_history(&mut report, &companies);
            }
            generate_attendance_report(&report, output, period.year, period.month)?;
            println!("已生成：{output}");
            println!("汇总人数：{}", report.summary_rows.len());
            println!("异常人数：{}", report.exception_rows.len());
            for warning in &report.warnings {
                println!("待确认：{warning}");
            }
        }
        _ => bail!("用法：attendance-cli <inspect|generate|calculate> ..."),
    }
    Ok(())
}
