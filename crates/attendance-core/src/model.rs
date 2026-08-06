use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AttendancePeriod {
    pub year: u16,
    pub month: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AttendanceDataset {
    pub period: AttendancePeriod,
    pub monthly: Vec<MonthlyRecord>,
    pub daily: Vec<DailyRecord>,
    pub invalid_punches: Vec<InvalidPunch>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonthlyRecord {
    pub employee_key: String,
    pub employee_no: String,
    pub name: String,
    pub user_id: String,
    pub attendance_group: String,
    pub department: String,
    pub position: String,
    pub attendance_days: f64,
    pub weekday_overtime_hours: f64,
    pub weekend_overtime_hours: f64,
    pub holiday_overtime_hours: f64,
    pub personal_leave_hours: f64,
    pub compensatory_leave_hours: f64,
    pub sick_leave_hours: f64,
    pub annual_leave_hours: f64,
    pub maternity_leave_days: f64,
    pub paternity_leave_days: f64,
    pub marriage_leave_days: f64,
    pub menstrual_leave_days: f64,
    pub bereavement_leave_days: f64,
    pub breastfeeding_leave_hours: f64,
    pub daily_results: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyRecord {
    pub employee_key: String,
    pub employee_no: String,
    pub name: String,
    pub date: String,
    pub shift: String,
    pub overtime_hours: f64,
    pub punch_slots: Vec<PunchSlot>,
    pub late_count: f64,
    pub severe_late_count: f64,
    pub absent_late_days: f64,
    pub early_count: f64,
    pub missing_in_count: f64,
    pub missing_out_count: f64,
    pub absent_days: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PunchSlot {
    pub kind: PunchKind,
    pub time: String,
    pub result: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum PunchKind {
    In,
    Out,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InvalidPunch {
    pub employee_key: String,
    pub employee_no: String,
    pub name: String,
    pub attendance_date: String,
    pub punch_time: String,
    pub result: String,
}
