//! Shared timezone helpers. The IDC operates in `Asia/Baghdad`, which is a
//! fixed UTC+03:00 year-round (Iraq dropped DST in 2008), so a local calendar
//! day for date D is the UTC interval `[D 00:00 +03:00, D+1 00:00 +03:00)`.
//!
//! All "today" / day-boundary logic across domains MUST go through these so
//! reception, shifts, and Daily Close agree on where a day starts. Using UTC
//! midnight (as the visits/shifts code did historically) put their day 3 hours
//! behind the reports/Daily Close local day, so counts and lists disagreed for
//! the first 3 hours of every local day.

use chrono::offset::FixedOffset;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

/// Iraq is fixed UTC+03:00 year-round.
pub const BAGHDAD_OFFSET_SECS: i32 = 3 * 3600;

pub fn baghdad_offset_seconds() -> i32 {
    BAGHDAD_OFFSET_SECS
}

/// `[start_utc, end_utc)` for the local-tz calendar day containing `date`.
/// `offset_secs` is the local UTC offset in seconds (positive east).
pub fn local_day_utc_range(date: NaiveDate, offset_secs: i32) -> (DateTime<Utc>, DateTime<Utc>) {
    let tz =
        FixedOffset::east_opt(offset_secs).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let start_local = date
        .and_hms_opt(0, 0, 0)
        .expect("hms 0/0/0 valid for any NaiveDate");
    let start = tz
        .from_local_datetime(&start_local)
        .single()
        .unwrap_or_else(|| tz.from_utc_datetime(&start_local));
    let start_utc: DateTime<Utc> = start.with_timezone(&Utc);
    let end_utc = start_utc + Duration::days(1);
    (start_utc, end_utc)
}

/// `[start_utc, end_utc)` for the Baghdad-local day that is "today" right now.
/// This is the canonical "today" boundary -- reception, shifts, and Daily
/// Close all resolve the same interval.
pub fn baghdad_today_utc_range() -> (DateTime<Utc>, DateTime<Utc>) {
    let offset = baghdad_offset_seconds();
    let tz = FixedOffset::east_opt(offset).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let local_date = Utc::now().with_timezone(&tz).date_naive();
    local_day_utc_range(local_date, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baghdad_day_starts_at_utc_2100_the_previous_day() {
        // 2026-05-13 local 00:00 +03:00 == 2026-05-12 21:00 UTC.
        let date = NaiveDate::from_ymd_opt(2026, 5, 13).unwrap();
        let (start, end) = local_day_utc_range(date, BAGHDAD_OFFSET_SECS);
        assert_eq!(start.to_rfc3339(), "2026-05-12T21:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-05-13T21:00:00+00:00");
    }

    #[test]
    fn today_range_is_exactly_24h_and_well_ordered() {
        let (start, end) = baghdad_today_utc_range();
        assert!(end > start);
        assert_eq!((end - start), Duration::days(1));
    }
}
