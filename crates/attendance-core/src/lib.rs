mod calculate;
mod config;
mod dingtalk;
mod holiday;
mod model;
mod report;

pub use calculate::{
    AttendanceReport, DailyAttendance, DetailRow, ExceptionRow, SummaryRow, apply_company_history,
    calculate_attendance, calculate_attendance_with_config,
};
pub use config::{AttendanceConfig, SpecialPerson, SpecialPersonnelConfig};
pub use dingtalk::{
    EmployeeIdentity, WorkbookSummary, WorksheetSummary, inspect_dingtalk, load_company_history,
    load_dingtalk,
};
pub use model::{
    AnnualLeaveRecord, AttendanceDataset, AttendancePeriod, CalendarDate, DailyRecord,
    EmploymentRecord, InvalidPunch, MonthlyRecord, PunchKind, PunchSlot,
};
pub use report::{generate_attendance_report, generate_report_skeleton};
