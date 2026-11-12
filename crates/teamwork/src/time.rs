//! Moments and calendar dates without a date library: moments are seconds
//! since 1970 (UTC), dates are days in the proleptic Gregorian calendar,
//! written `YYYY-MM-DD`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Seconds since 1970, now.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A calendar date, such as a task's due date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// The date (UTC) of the moment `seconds` since 1970.
    pub fn of(seconds: u64) -> Date {
        Date::from_days((seconds / 86_400) as i64)
    }

    /// Today (UTC).
    pub fn today() -> Date {
        Date::of(now())
    }

    /// Reads `YYYY-MM-DD` (spaces around it are fine).
    pub fn parse(text: &str) -> Option<Date> {
        let mut parts = text.trim().splitn(3, '-');
        let year: i32 = parts.next()?.parse().ok()?;
        let month: u8 = parts.next()?.parse().ok()?;
        let day: u8 = parts.next()?.parse().ok()?;
        let date = Date { year, month, day };
        let valid = (1970..=9999).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month);
        valid.then_some(date)
    }

    /// Days since 1970-01-01.
    pub fn days(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after 1970-01-01.
    pub fn from_days(days: i64) -> Date {
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Date { year, month, day }
    }

    /// The date `days` later (earlier when negative).
    pub fn add_days(self, days: i64) -> Date {
        Date::from_days(self.days() + days)
    }

    /// "Nov 12", or "Nov 12, 2027" when not in `this_year`.
    pub fn short(self, this_year: i32) -> String {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month = MONTHS[usize::from(self.month.clamp(1, 12)) - 1];
        if self.year == this_year {
            format!("{month} {}", self.day)
        } else {
            format!("{month} {}, {}", self.day, self.year)
        }
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        let text = String::deserialize(deserializer)?;
        Date::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("`{text}` isn't a YYYY-MM-DD date")))
    }
}

/// "just now", "5 min ago", "3 h ago", "2 days ago", or the date.
pub fn ago(then: u64, now: u64) -> String {
    let seconds = now.saturating_sub(then);
    match seconds {
        0..60 => "just now".to_owned(),
        60..3_600 => format!("{} min ago", seconds / 60),
        3_600..86_400 => format!("{} h ago", seconds / 3_600),
        86_400..172_800 => "yesterday".to_owned(),
        172_800..1_209_600 => format!("{} days ago", seconds / 86_400),
        _ => Date::of(then).short(Date::of(now).year),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip_through_days() {
        for days in [-1, 0, 59, 60, 365, 10_957, 20_404, 20_769, 2_932_896] {
            assert_eq!(Date::from_days(days).days(), days);
        }
        assert_eq!(Date::from_days(0).to_string(), "1970-01-01");
        // 2026-11-12 is day 20,769.
        assert_eq!(Date::parse("2026-11-12").unwrap().days(), 20_769);
        assert_eq!(Date::of(20_769 * 86_400 + 3_600).to_string(), "2026-11-12");
    }

    #[test]
    fn only_real_dates_parse() {
        assert!(Date::parse("2028-02-29").is_some());
        assert!(Date::parse("2027-02-29").is_none());
        assert!(Date::parse("2026-13-01").is_none());
        assert!(Date::parse("2026-11-31").is_none());
        assert!(Date::parse("next week").is_none());
        assert_eq!(
            Date::parse(" 2026-1-5 ").map(|d| d.to_string()),
            Some("2026-01-05".to_owned())
        );
    }

    #[test]
    fn dates_read_short() {
        let date = Date::parse("2026-11-12").unwrap();
        assert_eq!(date.short(2026), "Nov 12");
        assert_eq!(date.short(2025), "Nov 12, 2026");
        assert_eq!(date.add_days(20).to_string(), "2026-12-02");
        assert_eq!(ago(100, 100), "just now");
        assert_eq!(ago(0, 7_200), "2 h ago");
    }
}
