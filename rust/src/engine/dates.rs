//! Date and time helpers. Dates cross the bridge as "YYYY-MM-DD" strings.

use anyhow::{bail, Context, Result};
use chrono::{Datelike, NaiveDate};
use std::time::Duration;

pub fn parse_date(text: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").with_context(|| format!("Not a valid date: {text}"))
}

pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// "Saturday, September 26, 2026"
pub fn long_date(date: NaiveDate) -> String {
    date.format("%A, %B %-d, %Y").to_string()
}

/// "Saturday, September 26"
pub fn short_date(date: NaiveDate) -> String {
    date.format("%A, %B %-d").to_string()
}

pub fn first_of_month(year: i32, month: u32) -> Result<NaiveDate> {
    match NaiveDate::from_ymd_opt(year, month, 1) {
        Some(date) => Ok(date),
        None => bail!("Not a valid month: {year}-{month}"),
    }
}

pub fn days_in_month(year: i32, month: u32) -> Result<u32> {
    let first = first_of_month(year, month)?;
    let (next_year, next_month) = shift_month(year, month, 1);
    let next = first_of_month(next_year, next_month)?;
    Ok(next.signed_duration_since(first).num_days() as u32)
}

/// Moves `delta` months forward (or back, if negative).
pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year * 12 + (month as i32 - 1) + delta;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

/// "September 2026"
pub fn month_title(year: i32, month: u32) -> Result<String> {
    Ok(first_of_month(year, month)?.format("%B %Y").to_string())
}

/// A Monday-to-Sunday week: "Sep 21 – 27, 2026", "Sep 28 – Oct 4, 2026" or
/// "Dec 29, 2025 – Jan 4, 2026".
pub fn week_title(monday: NaiveDate) -> String {
    let sunday = monday + chrono::Days::new(6);
    if monday.year() != sunday.year() {
        format!("{} – {}", monday.format("%b %-d, %Y"), sunday.format("%b %-d, %Y"))
    } else if monday.month() != sunday.month() {
        format!("{} – {}", monday.format("%b %-d"), sunday.format("%b %-d, %Y"))
    } else {
        format!("{} – {}", monday.format("%b %-d"), sunday.format("%-d, %Y"))
    }
}

/// The key used for monthly statistics, like "2026-9" (the desktop app's format).
pub fn month_key(date: NaiveDate) -> String {
    format!("{}-{}", date.year(), date.month())
}

/// "1d 2h 3m 4s"
pub fn days_hours_minutes_seconds(total_seconds: u64) -> String {
    let days = total_seconds / (24 * 3600);
    let hours = (total_seconds % (24 * 3600)) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{days}d {hours}h {minutes}m {seconds}s")
}

/// "01:02:03"
pub fn hh_mm_ss(total_seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3600,
        (total_seconds % 3600) / 60,
        total_seconds % 60
    )
}

/// "59:07", with minutes going past 59 for long sessions (like the desktop app).
pub fn mm_ss(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// "2h 5m", "45m", "0m"
pub fn hours_minutes(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// "GMT+07:00", "GMT-03:30"
pub fn gmt_label(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let abs = offset_seconds.unsigned_abs();
    format!("GMT{sign}{:02}:{:02}", abs / 3600, (abs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_dates() {
        let date = parse_date("2026-09-26").unwrap();
        assert_eq!(iso(date), "2026-09-26");
        assert_eq!(long_date(date), "Saturday, September 26, 2026");
        assert_eq!(short_date(date), "Saturday, September 26");
        assert!(parse_date("26/09/2026").is_err());
    }

    #[test]
    fn month_math() {
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 9, -21), (2024, 12));
        assert_eq!(days_in_month(2024, 2).unwrap(), 29);
        assert_eq!(days_in_month(2026, 2).unwrap(), 28);
        assert_eq!(days_in_month(2026, 12).unwrap(), 31);
        assert_eq!(month_title(2026, 9).unwrap(), "September 2026");
        assert!(month_title(2026, 13).is_err());
    }

    #[test]
    fn formats_durations_like_the_desktop_app() {
        assert_eq!(days_hours_minutes_seconds(90061), "1d 1h 1m 1s");
        assert_eq!(hh_mm_ss(3723), "01:02:03");
        assert_eq!(mm_ss(Duration::from_secs(3600)), "60:00");
        assert_eq!(mm_ss(Duration::from_millis(59_900)), "00:59");
        assert_eq!(month_key(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()), "2026-9");
        assert_eq!(hours_minutes(7500), "2h 5m");
        assert_eq!(hours_minutes(2700), "45m");
        assert_eq!(hours_minutes(59), "0m");
    }

    #[test]
    fn labels_time_zones() {
        assert_eq!(gmt_label(7 * 3600), "GMT+07:00");
        assert_eq!(gmt_label(-(3 * 3600 + 1800)), "GMT-03:30");
        assert_eq!(gmt_label(0), "GMT+00:00");
    }

    #[test]
    fn titles_weeks() {
        let monday = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(week_title(monday(2026, 9, 21)), "Sep 21 – 27, 2026");
        assert_eq!(week_title(monday(2026, 9, 28)), "Sep 28 – Oct 4, 2026");
        assert_eq!(week_title(monday(2025, 12, 29)), "Dec 29, 2025 – Jan 4, 2026");
    }
}
