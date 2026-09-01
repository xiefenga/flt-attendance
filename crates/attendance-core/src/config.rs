use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceConfig {
    #[serde(default)]
    pub special_personnel: SpecialPersonnelConfig,
    #[serde(default)]
    pub excluded_personnel: Vec<SpecialPerson>,
    #[serde(default)]
    pub statutory_holiday_dates: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpecialPersonnelConfig {
    #[serde(default)]
    pub punch_meal_no_overtime: Vec<SpecialPerson>,
    #[serde(default)]
    pub weekday_weekend_punch_meal_holiday_overtime: Vec<SpecialPerson>,
    #[serde(default)]
    pub no_punch_meal_no_overtime: Vec<SpecialPerson>,
    #[serde(default)]
    pub no_meal_no_overtime: Vec<SpecialPerson>,
    #[serde(default)]
    pub flexible_arrival_shift: Vec<SpecialPerson>,
    #[serde(default)]
    pub six_day_no_meal: Vec<SpecialPerson>,
    #[serde(default)]
    pub six_day_four_hour_no_meal: Vec<SpecialPerson>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpecialPerson {
    #[serde(default)]
    pub employee_no: String,
    pub name: String,
}

impl SpecialPersonnelConfig {
    pub fn uses_punch_only_meal(&self, employee_no: &str, name: &str) -> bool {
        self.punch_meal_no_overtime
            .iter()
            .chain(&self.weekday_weekend_punch_meal_holiday_overtime)
            .any(|person| person.matches(employee_no, name))
    }

    pub fn keeps_only_statutory_holiday_overtime(&self, employee_no: &str, name: &str) -> bool {
        self.weekday_weekend_punch_meal_holiday_overtime
            .iter()
            .any(|person| person.matches(employee_no, name))
    }

    pub fn uses_scheduled_meal_without_punch(&self, employee_no: &str, name: &str) -> bool {
        self.no_punch_meal_no_overtime
            .iter()
            .any(|person| person.matches(employee_no, name))
    }

    pub fn excludes_meal(&self, employee_no: &str, name: &str) -> bool {
        self.no_meal_no_overtime
            .iter()
            .chain(&self.six_day_no_meal)
            .chain(&self.six_day_four_hour_no_meal)
            .any(|person| person.matches(employee_no, name))
    }

    pub fn six_day_daily_hours(&self, employee_no: &str, name: &str) -> Option<f64> {
        if self
            .six_day_four_hour_no_meal
            .iter()
            .any(|person| person.matches(employee_no, name))
        {
            Some(4.0)
        } else if self
            .six_day_no_meal
            .iter()
            .any(|person| person.matches(employee_no, name))
        {
            Some(8.0)
        } else {
            None
        }
    }

    pub fn uses_six_day_schedule(&self, employee_no: &str, name: &str) -> bool {
        self.six_day_daily_hours(employee_no, name).is_some()
    }

    pub fn excludes_overtime(&self, employee_no: &str, name: &str) -> bool {
        self.all_people()
            .any(|person| person.matches(employee_no, name))
    }

    pub fn matched_count<'a>(&self, employees: impl Iterator<Item = (&'a str, &'a str)>) -> usize {
        employees
            .filter(|(employee_no, name)| self.excludes_overtime(employee_no, name))
            .count()
    }

    pub fn statutory_holiday_overtime_only_matched_count<'a>(
        &self,
        employees: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> usize {
        employees
            .filter(|(employee_no, name)| {
                self.keeps_only_statutory_holiday_overtime(employee_no, name)
            })
            .count()
    }

    pub fn uses_flexible_arrival_shift(&self, employee_no: &str, name: &str) -> bool {
        self.flexible_arrival_shift
            .iter()
            .any(|person| person.matches(employee_no, name))
    }

    pub fn flexible_arrival_matched_count<'a>(
        &self,
        employees: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> usize {
        employees
            .filter(|(employee_no, name)| self.uses_flexible_arrival_shift(employee_no, name))
            .count()
    }

    pub fn six_day_matched_count<'a>(
        &self,
        employees: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> usize {
        employees
            .filter(|(employee_no, name)| self.uses_six_day_schedule(employee_no, name))
            .count()
    }

    fn all_people(&self) -> impl Iterator<Item = &SpecialPerson> {
        self.punch_meal_no_overtime
            .iter()
            .chain(&self.no_punch_meal_no_overtime)
            .chain(&self.no_meal_no_overtime)
            .chain(&self.six_day_no_meal)
            .chain(&self.six_day_four_hour_no_meal)
    }
}

impl AttendanceConfig {
    pub fn excludes_employee(&self, employee_no: &str, name: &str) -> bool {
        self.excluded_personnel
            .iter()
            .any(|person| person.matches(employee_no, name))
    }

    pub fn excluded_count<'a>(&self, employees: impl Iterator<Item = (&'a str, &'a str)>) -> usize {
        employees
            .filter(|(employee_no, name)| self.excludes_employee(employee_no, name))
            .count()
    }

    pub fn has_statutory_holiday_override(&self, year: u16) -> bool {
        self.statutory_holiday_dates
            .iter()
            .any(|date| date_year(date) == Some(year))
    }

    pub fn statutory_holiday_count(&self, year: u16) -> usize {
        self.statutory_holiday_dates
            .iter()
            .filter(|date| date_year(date) == Some(year))
            .count()
    }

    pub(crate) fn is_statutory_holiday(&self, year: u16, month: u8, day: u8) -> bool {
        if !self.has_statutory_holiday_override(year) {
            return crate::holiday::is_holiday(year, month, day);
        }
        let expected = format!("{year:04}-{month:02}-{day:02}");
        self.statutory_holiday_dates
            .iter()
            .any(|date| date == &expected)
    }
}

fn date_year(date: &str) -> Option<u16> {
    let (year, remainder) = date.split_once('-')?;
    if year.len() != 4 || remainder.len() != 5 {
        return None;
    }
    year.parse().ok()
}

impl SpecialPerson {
    fn matches(&self, employee_no: &str, name: &str) -> bool {
        if self.employee_no.trim().is_empty() {
            self.name.trim() == name.trim()
        } else {
            self.employee_no.trim() == employee_no.trim()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn employee_number_takes_priority_over_duplicate_name() {
        let config = SpecialPersonnelConfig {
            no_meal_no_overtime: vec![SpecialPerson {
                employee_no: "17003".to_owned(),
                name: "李欣".to_owned(),
            }],
            ..Default::default()
        };
        assert!(config.excludes_overtime("17003", "李欣"));
        assert!(!config.excludes_overtime("25242", "李欣"));
    }

    #[test]
    fn name_is_used_when_employee_number_is_unknown() {
        let config = SpecialPersonnelConfig {
            punch_meal_no_overtime: vec![SpecialPerson {
                employee_no: String::new(),
                name: "吴晶晶".to_owned(),
            }],
            ..Default::default()
        };
        assert!(config.excludes_overtime("99999", "吴晶晶"));
        assert!(config.uses_punch_only_meal("99999", "吴晶晶"));
    }

    #[test]
    fn flexible_arrival_shift_does_not_exclude_overtime() {
        let config = SpecialPersonnelConfig {
            flexible_arrival_shift: vec![SpecialPerson {
                employee_no: "26333".to_owned(),
                name: "张一成".to_owned(),
            }],
            ..Default::default()
        };
        assert!(config.uses_flexible_arrival_shift("26333", "张一成"));
        assert!(!config.excludes_overtime("26333", "张一成"));
    }

    #[test]
    fn weekday_weekend_rule_uses_punch_meals_and_keeps_holiday_overtime() {
        let config = SpecialPersonnelConfig {
            weekday_weekend_punch_meal_holiday_overtime: vec![SpecialPerson {
                employee_no: "10001".to_owned(),
                name: "测试员工".to_owned(),
            }],
            ..Default::default()
        };
        assert!(config.uses_punch_only_meal("10001", "测试员工"));
        assert!(config.keeps_only_statutory_holiday_overtime("10001", "测试员工"));
        assert!(!config.excludes_overtime("10001", "测试员工"));
    }

    #[test]
    fn six_day_schedules_exclude_meals_and_all_overtime() {
        let config = SpecialPersonnelConfig {
            six_day_no_meal: vec![SpecialPerson {
                employee_no: String::new(),
                name: "廖传兰".to_owned(),
            }],
            six_day_four_hour_no_meal: vec![SpecialPerson {
                employee_no: String::new(),
                name: "廖传霞".to_owned(),
            }],
            ..Default::default()
        };
        assert_eq!(config.six_day_daily_hours("", "廖传兰"), Some(8.0));
        assert_eq!(config.six_day_daily_hours("", "廖传霞"), Some(4.0));
        assert!(config.excludes_meal("", "廖传兰"));
        assert!(config.excludes_meal("", "廖传霞"));
        assert!(config.excludes_overtime("", "廖传兰"));
        assert!(config.excludes_overtime("", "廖传霞"));
    }

    #[test]
    fn excluded_person_matches_by_employee_number() {
        let config = AttendanceConfig {
            excluded_personnel: vec![SpecialPerson {
                employee_no: "25181".to_owned(),
                name: "李文祥".to_owned(),
            }],
            ..Default::default()
        };
        assert!(config.excludes_employee("25181", "李文祥"));
        assert!(!config.excludes_employee("25182", "李文祥"));
    }

    #[test]
    fn statutory_holiday_dates_override_one_year_only() {
        let config = AttendanceConfig {
            statutory_holiday_dates: vec!["2026-09-25".to_owned()],
            ..Default::default()
        };
        assert!(config.has_statutory_holiday_override(2026));
        assert_eq!(config.statutory_holiday_count(2026), 1);
        assert!(config.is_statutory_holiday(2026, 9, 25));
        assert!(!config.is_statutory_holiday(2026, 9, 26));
        assert!(!config.has_statutory_holiday_override(2025));
    }

    #[test]
    fn empty_statutory_holiday_dates_keep_embedded_calendar_behavior() {
        let config = AttendanceConfig::default();
        assert!(config.is_statutory_holiday(2026, 9, 25));
        assert!(config.is_statutory_holiday(2026, 9, 26));
    }
}
