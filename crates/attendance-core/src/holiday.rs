use std::sync::OnceLock;

use serde::Deserialize;

include!(concat!(env!("OUT_DIR"), "/holiday_data.rs"));

#[derive(Debug, Deserialize)]
struct HolidayCalendar {
    year: u16,
    papers: Vec<String>,
    days: Vec<HolidayDay>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HolidayDay {
    name: String,
    date: String,
    is_off_day: bool,
}

fn calendars() -> &'static [HolidayCalendar] {
    static CALENDARS: OnceLock<Vec<HolidayCalendar>> = OnceLock::new();
    CALENDARS.get_or_init(|| {
        EMBEDDED_HOLIDAY_JSON
            .iter()
            .map(|(expected_year, json)| {
                let calendar: HolidayCalendar =
                    serde_json::from_str(json).expect("embedded holiday JSON should be valid");
                assert_eq!(
                    calendar.year, *expected_year,
                    "holiday filename year mismatch"
                );
                assert!(
                    calendar.papers.iter().any(|url| url.contains("gov.cn")),
                    "holiday data should cite a government announcement"
                );
                assert!(
                    calendar.days.iter().all(|day| !day.name.trim().is_empty()),
                    "holiday names should not be empty"
                );
                calendar
            })
            .collect()
    })
}

pub(crate) fn has_calendar(year: u16) -> bool {
    calendars().iter().any(|calendar| calendar.year == year)
}

pub(crate) fn is_workday(year: u16, month: u8, day: u8) -> bool {
    let date = format!("{year:04}-{month:02}-{day:02}");
    if let Some(override_value) = calendars()
        .iter()
        .find(|calendar| calendar.year == year)
        .and_then(|calendar| calendar.days.iter().find(|item| item.date == date))
    {
        return !override_value.is_off_day;
    }
    !matches!(weekday_index(year, month, day), 0 | 6)
}

pub(crate) fn is_off_day(year: u16, month: u8, day: u8) -> bool {
    !is_workday(year, month, day)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_overrides_weekends_and_weekdays() {
        assert!(has_calendar(2026));
        assert!(!is_workday(2026, 1, 1));
        assert!(is_workday(2026, 1, 4));
        assert!(!is_workday(2026, 2, 23));
        assert!(is_workday(2026, 2, 28));
    }

    #[test]
    fn calendar_keeps_normal_weekday_fallback() {
        assert!(is_workday(2026, 7, 1));
        assert!(!is_workday(2026, 7, 4));
    }

    #[test]
    fn calendar_preserves_holiday_names() {
        let calendar = calendars()
            .iter()
            .find(|calendar| calendar.year == 2026)
            .expect("2026 calendar should be embedded");
        assert!(calendar.days.iter().any(|day| day.name == "春节"));
    }
}
