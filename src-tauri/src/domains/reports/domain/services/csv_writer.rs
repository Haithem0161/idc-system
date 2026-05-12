//! CSV writers for the three reports (§7.7). UTF-8 BOM + CRLF line endings +
//! RFC 4180 quoting (every field that contains a quote, comma, or newline is
//! double-quoted with embedded quotes doubled).

use std::path::Path;
use uuid::Uuid;

use crate::domains::reports::domain::entities::{
    DoctorEarningsRow, OperatorEarningsRow, VisitRow, VisitsReportTotals,
};
use crate::error::{AppError, AppResult};

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

const VISITS_HEADERS: &[&str] = &[
    "Date",
    "Visit #",
    "Patient",
    "Check",
    "Subtype",
    "Doctor",
    "Operator",
    "Dye",
    "Report",
    "Price (IQD)",
    "Doctor Cut (IQD)",
    "Operator Cut (IQD)",
    "Net (IQD)",
];

const DOCTOR_HEADERS: &[&str] = &[
    "Doctor",
    "Specialty",
    "Visits",
    "Revenue (IQD)",
    "Doctor Cut Total (IQD)",
    "Avg Cut Per Visit (IQD)",
];

const OPERATOR_HEADERS: &[&str] = &[
    "Operator",
    "Visits",
    "Visits With Dye",
    "Operator Cut Total (IQD)",
    "Hours On Shift",
    "Avg Cut Per Hour (IQD)",
];

fn quote_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn write_row(buf: &mut Vec<u8>, fields: &[String]) {
    let line = fields
        .iter()
        .map(|s| quote_field(s))
        .collect::<Vec<_>>()
        .join(",");
    buf.extend_from_slice(line.as_bytes());
    buf.extend_from_slice(b"\r\n");
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "Y"
    } else {
        "N"
    }
}

fn naive_date_string(dt: Option<chrono::DateTime<chrono::Utc>>) -> String {
    dt.map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn visit_number(id: Uuid) -> String {
    let s = id.simple().to_string();
    format!("V-{}", &s[s.len() - 6..])
}

/// §7.25: rows sorted by `(locked_at ASC, visit_id ASC)`; footer `TOTAL,...`.
pub fn write_visits_csv(
    rows: &[VisitRow],
    totals: &VisitsReportTotals,
    path: &Path,
) -> AppResult<()> {
    let mut sorted: Vec<&VisitRow> = rows.iter().collect();
    sorted.sort_by(|a, b| match (a.locked_at, b.locked_at) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.visit_id.cmp(&b.visit_id)),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a.visit_id.cmp(&b.visit_id),
    });

    let mut buf = Vec::with_capacity(rows.len() * 128);
    buf.extend_from_slice(BOM);
    write_row(
        &mut buf,
        &VISITS_HEADERS
            .iter()
            .map(|s| (*s).into())
            .collect::<Vec<_>>(),
    );
    for row in &sorted {
        let cells: Vec<String> = vec![
            naive_date_string(row.locked_at),
            visit_number(row.visit_id),
            row.patient_name.clone(),
            row.check_type_name_en
                .clone()
                .unwrap_or_else(|| row.check_type_name_ar.clone()),
            row.check_subtype_name_en
                .clone()
                .or_else(|| row.check_subtype_name_ar.clone())
                .unwrap_or_default(),
            row.doctor_name.clone().unwrap_or_else(|| "(house)".into()),
            row.operator_name.clone(),
            yes_no(row.dye).into(),
            yes_no(row.report).into(),
            row.price_iqd.to_string(),
            row.doctor_cut_iqd.to_string(),
            row.operator_cut_iqd.to_string(),
            row.net_iqd.to_string(),
        ];
        write_row(&mut buf, &cells);
    }
    // Footer.
    let footer: Vec<String> = vec![
        "TOTAL".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        totals.revenue_iqd.to_string(),
        totals.doctor_cut_iqd.to_string(),
        totals.operator_cut_iqd.to_string(),
        totals.net_iqd.to_string(),
    ];
    write_row(&mut buf, &footer);

    write_atomic(path, &buf)
}

/// Doctor earnings: sort by `name ASC` with `(house)` last; footer `TOTAL,...`.
pub fn write_doctor_earnings_csv(rows: &[DoctorEarningsRow], path: &Path) -> AppResult<()> {
    let mut sorted: Vec<&DoctorEarningsRow> = rows.iter().collect();
    sorted.sort_by(|a, b| match (a.doctor_id, b.doctor_id) {
        (None, _) => std::cmp::Ordering::Greater,
        (_, None) => std::cmp::Ordering::Less,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let mut buf = Vec::with_capacity(rows.len() * 64);
    buf.extend_from_slice(BOM);
    write_row(
        &mut buf,
        &DOCTOR_HEADERS
            .iter()
            .map(|s| (*s).into())
            .collect::<Vec<_>>(),
    );

    let mut sum_visits: i64 = 0;
    let mut sum_revenue: i64 = 0;
    let mut sum_cut: i64 = 0;
    for row in &sorted {
        sum_visits += row.visits;
        sum_revenue += row.revenue_iqd;
        sum_cut += row.doctor_cut_total_iqd;
        let cells: Vec<String> = vec![
            row.name.clone(),
            row.specialty.clone().unwrap_or_default(),
            row.visits.to_string(),
            row.revenue_iqd.to_string(),
            row.doctor_cut_total_iqd.to_string(),
            row.avg_cut_per_visit_iqd.to_string(),
        ];
        write_row(&mut buf, &cells);
    }

    let avg = if sum_visits > 0 {
        sum_cut / sum_visits
    } else {
        0
    };
    let footer: Vec<String> = vec![
        "TOTAL".into(),
        String::new(),
        sum_visits.to_string(),
        sum_revenue.to_string(),
        sum_cut.to_string(),
        avg.to_string(),
    ];
    write_row(&mut buf, &footer);
    write_atomic(path, &buf)
}

pub fn write_operator_earnings_csv(rows: &[OperatorEarningsRow], path: &Path) -> AppResult<()> {
    let mut sorted: Vec<&OperatorEarningsRow> = rows.iter().collect();
    sorted.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut buf = Vec::with_capacity(rows.len() * 64);
    buf.extend_from_slice(BOM);
    write_row(
        &mut buf,
        &OPERATOR_HEADERS
            .iter()
            .map(|s| (*s).into())
            .collect::<Vec<_>>(),
    );

    let mut sum_visits: i64 = 0;
    let mut sum_dye: i64 = 0;
    let mut sum_cut: i64 = 0;
    let mut sum_hours_milli: i64 = 0;
    for row in &sorted {
        sum_visits += row.visits;
        sum_dye += row.visits_with_dye;
        sum_cut += row.operator_cut_total_iqd;
        sum_hours_milli += row.hours_on_shift_milli;
        let hours = format!("{:.2}", (row.hours_on_shift_milli as f64) / 3_600_000.0);
        let cells: Vec<String> = vec![
            row.name.clone(),
            row.visits.to_string(),
            row.visits_with_dye.to_string(),
            row.operator_cut_total_iqd.to_string(),
            hours,
            row.avg_cut_per_hour_iqd.to_string(),
        ];
        write_row(&mut buf, &cells);
    }

    let hours_total = format!("{:.2}", (sum_hours_milli as f64) / 3_600_000.0);
    let avg_per_hour = if sum_hours_milli > 0 {
        // Hours in millis: avg = cut * 3600_000 / hours_milli.
        sum_cut
            .saturating_mul(3_600_000)
            .checked_div(sum_hours_milli)
            .unwrap_or(0)
    } else {
        0
    };
    let footer: Vec<String> = vec![
        "TOTAL".into(),
        sum_visits.to_string(),
        sum_dye.to_string(),
        sum_cut.to_string(),
        hours_total,
        avg_per_hour.to_string(),
    ];
    write_row(&mut buf, &footer);
    write_atomic(path, &buf)
}

/// Atomic write via temp file + rename so a crash mid-write leaves the
/// previous file intact (matches the receipts/render pattern).
fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Validation("csv path has no parent directory".into()))?;
    std::fs::create_dir_all(parent).map_err(AppError::from)?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("report.csv")
    ));
    std::fs::write(&tmp, bytes).map_err(AppError::from)?;
    std::fs::rename(&tmp, path).map_err(AppError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::io::Read;
    use tempfile::tempdir;

    fn fixture_row() -> VisitRow {
        VisitRow {
            visit_id: Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap(),
            locked_at: Some(Utc.with_ymd_and_hms(2026, 5, 12, 13, 30, 0).unwrap()),
            status: "locked".into(),
            patient_name: "Ahmed, Smith".into(),
            check_type_name_ar: "نوع".into(),
            check_type_name_en: Some("Check".into()),
            check_subtype_name_ar: None,
            check_subtype_name_en: None,
            doctor_name: Some("Dr. \"Smiles\"".into()),
            operator_name: "Op A".into(),
            dye: true,
            report: false,
            price_iqd: 50_000,
            doctor_cut_iqd: 20_000,
            operator_cut_iqd: 5_000,
            net_iqd: 25_000,
        }
    }

    #[test]
    fn visits_csv_has_bom_crlf_and_quoting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("visits.csv");
        let totals = VisitsReportTotals {
            visits: 1,
            revenue_iqd: 50_000,
            doctor_cut_iqd: 20_000,
            operator_cut_iqd: 5_000,
            net_iqd: 25_000,
        };
        write_visits_csv(&[fixture_row()], &totals, &path).unwrap();
        let mut buf = Vec::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        // BOM.
        assert_eq!(&buf[..3], &[0xEF, 0xBB, 0xBF]);
        let text = std::str::from_utf8(&buf[3..]).unwrap();
        // Quoting on patient and doctor.
        assert!(text.contains("\"Ahmed, Smith\""));
        assert!(text.contains("\"Dr. \"\"Smiles\"\"\""));
        // CRLF.
        assert!(text.contains("\r\n"));
        // Footer.
        assert!(text.contains("TOTAL,,,,,,,,,50000,20000,5000,25000"));
    }

    #[test]
    fn doctor_csv_sorts_house_last() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doctors.csv");
        let rows = vec![
            DoctorEarningsRow {
                doctor_id: None,
                name: "(house)".into(),
                specialty: None,
                visits: 5,
                revenue_iqd: 250_000,
                doctor_cut_total_iqd: 100_000,
                avg_cut_per_visit_iqd: 20_000,
            },
            DoctorEarningsRow {
                doctor_id: Some(Uuid::nil()),
                name: "Dr. Bee".into(),
                specialty: Some("Rad".into()),
                visits: 3,
                revenue_iqd: 90_000,
                doctor_cut_total_iqd: 30_000,
                avg_cut_per_visit_iqd: 10_000,
            },
            DoctorEarningsRow {
                doctor_id: Some(Uuid::nil()),
                name: "Dr. Apple".into(),
                specialty: None,
                visits: 1,
                revenue_iqd: 20_000,
                doctor_cut_total_iqd: 8_000,
                avg_cut_per_visit_iqd: 8_000,
            },
        ];
        write_doctor_earnings_csv(&rows, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // BOM-prefixed header line[0].
        assert!(lines[1].starts_with("Dr. Apple"));
        assert!(lines[2].starts_with("Dr. Bee"));
        assert!(lines[3].starts_with("(house)"));
        assert!(lines[4].starts_with("TOTAL"));
    }
}
