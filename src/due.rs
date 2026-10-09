//! Parsing and formatting of due dates.
//!
//! Accepted inputs (all interpreted in the local time zone):
//! - `none` / `clear` / `-`          → no due date
//! - `today`, `tomorrow`             → end of that day
//! - `mon` … `sunday`                → end of the next such weekday
//! - `2026-10-12`                    → end of that day
//! - `2026-10-12 17:00` / `…T17:00`  → that exact time
//! - `3d`, `+2h`, `1w`, `2 days`     → now plus that span

use jiff::civil::{Date, DateTime, Weekday};
use jiff::{Span, Timestamp, Zoned};

use crate::error::{Error, Result};

/// Parses a due date relative to the current local time.
pub fn parse(input: &str) -> Result<Option<Timestamp>> {
    parse_at(input, &Zoned::now())
}

/// Parses a due date relative to `now`.
pub fn parse_at(input: &str, now: &Zoned) -> Result<Option<Timestamp>> {
    let s = input.trim().to_lowercase();
    let invalid = || Error::InvalidDue(input.to_string());

    if matches!(s.as_str(), "none" | "clear" | "-" | "") {
        return Ok(None);
    }

    let end_of = |date: Date| -> Result<Option<Timestamp>> {
        let zoned = date
            .at(23, 59, 59, 0)
            .to_zoned(now.time_zone().clone())
            .map_err(|_| invalid())?;
        Ok(Some(zoned.timestamp()))
    };

    match s.as_str() {
        "today" => return end_of(now.date()),
        "tomorrow" => return end_of(now.date().tomorrow().map_err(|_| invalid())?),
        _ => {}
    }
    if let Some(weekday) = parse_weekday(&s) {
        return end_of(now.date().nth_weekday(1, weekday).map_err(|_| invalid())?);
    }
    // `Date` also parses datetimes (dropping the time), so only try it
    // when no time is given.
    if !s.contains(':')
        && let Ok(date) = s.parse::<Date>()
    {
        return end_of(date);
    }
    if let Ok(datetime) = s.parse::<DateTime>() {
        let zoned = datetime
            .to_zoned(now.time_zone().clone())
            .map_err(|_| invalid())?;
        return Ok(Some(zoned.timestamp()));
    }
    if let Ok(timestamp) = s.parse::<Timestamp>() {
        return Ok(Some(timestamp));
    }
    if let Ok(span) = s.trim_start_matches('+').parse::<Span>() {
        let zoned = now.checked_add(span).map_err(|_| invalid())?;
        return Ok(Some(zoned.timestamp()));
    }
    Err(invalid())
}

/// Formats a due date in local time. Dates set via a day-only input
/// (end of day) are shown without the time.
pub fn format(due: Timestamp) -> String {
    let zoned = due.to_zoned(jiff::tz::TimeZone::system());
    let time = zoned.time();
    if (time.hour(), time.minute(), time.second()) == (23, 59, 59) {
        zoned.strftime("%a %Y-%m-%d").to_string()
    } else {
        zoned.strftime("%a %Y-%m-%d %H:%M").to_string()
    }
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    let weekday = match s {
        "mon" | "monday" => Weekday::Monday,
        "tue" | "tues" | "tuesday" => Weekday::Tuesday,
        "wed" | "wednesday" => Weekday::Wednesday,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thursday,
        "fri" | "friday" => Weekday::Friday,
        "sat" | "saturday" => Weekday::Saturday,
        "sun" | "sunday" => Weekday::Sunday,
        _ => return None,
    };
    Some(weekday)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Zoned {
        // A Friday.
        "2026-10-09T10:30:00[UTC]".parse().unwrap()
    }

    fn parsed(input: &str) -> String {
        let ts = parse_at(input, &now()).unwrap().unwrap();
        ts.to_zoned(jiff::tz::TimeZone::UTC)
            .strftime("%Y-%m-%d %H:%M")
            .to_string()
    }

    #[test]
    fn keywords() {
        assert_eq!(parsed("today"), "2026-10-09 23:59");
        assert_eq!(parsed("Tomorrow"), "2026-10-10 23:59");
        assert_eq!(parsed("mon"), "2026-10-12 23:59");
        assert_eq!(parsed("friday"), "2026-10-16 23:59");
        assert_eq!(parse_at("none", &now()).unwrap(), None);
    }

    #[test]
    fn absolute() {
        assert_eq!(parsed("2026-12-01"), "2026-12-01 23:59");
        assert_eq!(parsed("2026-12-01 17:00"), "2026-12-01 17:00");
        assert_eq!(parsed("2026-12-01T09:15"), "2026-12-01 09:15");
    }

    #[test]
    fn relative() {
        assert_eq!(parsed("3d"), "2026-10-12 10:30");
        assert_eq!(parsed("+2h"), "2026-10-09 12:30");
        assert_eq!(parsed("1w"), "2026-10-16 10:30");
        assert_eq!(parsed("2 days"), "2026-10-11 10:30");
    }

    #[test]
    fn invalid() {
        assert!(parse_at("someday", &now()).is_err());
    }
}
