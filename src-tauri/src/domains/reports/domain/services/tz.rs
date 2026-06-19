//! Timezone helpers. The IDC operates in `Asia/Baghdad` which is fixed
//! UTC+03:00 year-round (no DST), so a local day for date D resolves to the
//! UTC interval `[D 00:00 +03:00, D+1 00:00 +03:00)`. Phase-07 §7.8.

use chrono::offset::FixedOffset;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

/// Iraq is fixed UTC+03:00 year-round (Iraq dropped DST in 2008).
pub const BAGHDAD_OFFSET_SECS: i32 = 3 * 3600;

pub fn baghdad_offset_seconds() -> i32 {
    BAGHDAD_OFFSET_SECS
}

/// The local calendar date a UTC instant falls on, given the local offset.
/// Used to decide which daily-close a lock/void belongs to (immutability guard).
pub fn utc_to_local_date(instant: DateTime<Utc>, offset_secs: i32) -> NaiveDate {
    let tz =
        FixedOffset::east_opt(offset_secs).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    instant.with_timezone(&tz).date_naive()
}

/// Returns the `[start_utc, end_utc)` interval for a local-tz calendar day.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baghdad_day_starts_three_hours_earlier_in_utc() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let (start, end) = local_day_utc_range(date, BAGHDAD_OFFSET_SECS);
        assert_eq!(start.to_rfc3339(), "2026-05-11T21:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-05-12T21:00:00+00:00");
    }

    /// Iraq has been UTC+03:00 year-round since dropping DST in 2008
    /// (§7.8). Mid-summer vs mid-winter ranges have the same offset.
    #[test]
    fn offset_is_stable_across_summer_and_winter() {
        assert_eq!(baghdad_offset_seconds(), 3 * 3600);
        let summer = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        let winter = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let (s_start, s_end) = local_day_utc_range(summer, BAGHDAD_OFFSET_SECS);
        let (w_start, w_end) = local_day_utc_range(winter, BAGHDAD_OFFSET_SECS);
        assert_eq!(s_end - s_start, Duration::hours(24));
        assert_eq!(w_end - w_start, Duration::hours(24));
    }

    /// Interval is exactly 24 hours -- inclusive-exclusive (`[start, end)`).
    #[test]
    fn interval_is_inclusive_exclusive_24_hours() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let (start, end) = local_day_utc_range(date, BAGHDAD_OFFSET_SECS);
        assert_eq!(end - start, Duration::hours(24));
        // 23:59:59.999 local on D is INSIDE the interval; 00:00:00 local on
        // D+1 is OUTSIDE.
        let last_local = date.and_hms_milli_opt(23, 59, 59, 999).unwrap();
        let last_utc = FixedOffset::east_opt(BAGHDAD_OFFSET_SECS)
            .unwrap()
            .from_local_datetime(&last_local)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        assert!(last_utc >= start && last_utc < end);
    }

    /// Zero offset (UTC) also resolves the same calendar day.
    #[test]
    fn zero_offset_yields_utc_day_boundary() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let (start, end) = local_day_utc_range(date, 0);
        assert_eq!(start.to_rfc3339(), "2026-05-12T00:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-05-13T00:00:00+00:00");
    }

    /// Year boundary edge: Dec 31 in Baghdad starts at 21:00 UTC on Dec 30.
    #[test]
    fn year_boundary_resolves_to_prior_day_utc() {
        let date = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        let (start, end) = local_day_utc_range(date, BAGHDAD_OFFSET_SECS);
        assert_eq!(start.to_rfc3339(), "2026-12-30T21:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-12-31T21:00:00+00:00");
    }

    /// Invalid offset_secs (extreme) clamps to UTC rather than panicking.
    #[test]
    fn invalid_offset_falls_back_to_utc() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        // FixedOffset::east_opt rejects > 86_400; helper falls back to UTC.
        let (start, _) = local_day_utc_range(date, 999_999);
        assert_eq!(start.to_rfc3339(), "2026-05-12T00:00:00+00:00");
    }
}
