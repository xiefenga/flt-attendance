use attendance_core::{
    AttendanceConfig, apply_company_history, calculate_attendance_with_config,
    generate_attendance_report, inspect_dingtalk as inspect_workbook, load_company_history,
    load_dingtalk,
};
use napi::{Error, Result, Status};
use napi_derive::napi;

fn napi_error(message: impl ToString) -> Error {
    Error::new(Status::GenericFailure, message.to_string())
}

#[napi]
pub fn inspect_dingtalk(path: String) -> Result<String> {
    let summary = inspect_workbook(path).map_err(napi_error)?;
    serde_json::to_string(&summary).map_err(napi_error)
}

#[napi]
pub fn generate_report(
    input_path: String,
    output_path: String,
    history_path: Option<String>,
    config_json: Option<String>,
) -> Result<String> {
    let dataset = load_dingtalk(input_path).map_err(napi_error)?;
    let period = dataset.period;
    let config = config_json
        .map(|value| serde_json::from_str::<AttendanceConfig>(&value))
        .transpose()
        .map_err(napi_error)?
        .unwrap_or_default();
    let mut report = calculate_attendance_with_config(&dataset, &config);
    if let Some(history_path) = history_path {
        let companies = load_company_history(history_path).map_err(napi_error)?;
        apply_company_history(&mut report, &companies);
    }
    generate_attendance_report(&report, output_path, period.year, period.month)
        .map_err(napi_error)?;
    serde_json::to_string(&report).map_err(napi_error)
}
