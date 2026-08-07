use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceConfig {
    #[serde(default)]
    pub special_personnel: SpecialPersonnelConfig,
    #[serde(default)]
    pub excluded_personnel: Vec<SpecialPerson>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpecialPersonnelConfig {
    #[serde(default)]
    pub punch_meal_no_overtime: Vec<SpecialPerson>,
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
}
