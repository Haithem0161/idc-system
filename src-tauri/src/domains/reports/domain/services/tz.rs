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
}
