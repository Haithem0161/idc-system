//! CSV writers for the three reports (§7.7). UTF-8 BOM + CRLF line endings +
//! RFC 4180 quoting (every field that contains a quote, comma, or newline is
//! double-quoted with embedded quotes doubled).

use std::path::Path;
use uuid::Uuid;

use crate::domains::reports::domain::entities::{
    DoctorEarningsRow, MandoubEarningsRow, OperatorEarningsRow, VisitRow, VisitsReportTotals,
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
    "Report (IQD)",
    "Mandoub (IQD)",
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

const MANDOUB_HEADERS: &[&str] = &[
    "Mandoub",
    "Visits",
    "Mandoub Cut Total (IQD)",
    "Avg Cut Per Visit (IQD)",
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
            row.report_amount_iqd.to_string(),
            row.mandoub_cut_iqd.to_string(),
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
        totals.report_iqd.to_string(),
        totals.mandoub_cut_iqd.to_string(),
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

/// مندوب earnings: sort by `name ASC`; footer `TOTAL,...`. Mirrors the operator
/// CSV without the dye / hours dimensions (a مندوب earns a flat per-visit cut).
pub fn write_mandoub_earnings_csv(rows: &[MandoubEarningsRow], path: &Path) -> AppResult<()> {
    let mut sorted: Vec<&MandoubEarningsRow> = rows.iter().collect();
    sorted.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut buf = Vec::with_capacity(rows.len() * 64);
    buf.extend_from_slice(BOM);
    write_row(
        &mut buf,
        &MANDOUB_HEADERS
            .iter()
            .map(|s| (*s).into())
            .collect::<Vec<_>>(),
    );

    let mut sum_visits: i64 = 0;
    let mut sum_cut: i64 = 0;
    for row in &sorted {
        sum_visits += row.visits;
        sum_cut += row.mandoub_cut_total_iqd;
        let cells: Vec<String> = vec![
            row.name.clone(),
            row.visits.to_string(),
            row.mandoub_cut_total_iqd.to_string(),
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
        sum_visits.to_string(),
        sum_cut.to_string(),
        avg.to_string(),
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
            report_amount_iqd: 0,
            mandoub_cut_iqd: 0,
            total_iqd: 50_000,
            amount_paid_override_iqd: None,
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
            report_iqd: 0,
            mandoub_cut_iqd: 0,
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
        // Footer (report + mandoub columns 0 inserted before net).
        assert!(text.contains("TOTAL,,,,,,,,,50000,20000,5000,0,0,25000"));
    }

    /// The visits CSV carries a Report (IQD) money column reflecting the
    /// per-visit carve-out, and the totals footer sums it.
    #[test]
    fn visits_csv_renders_report_amount_column() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("visits.csv");
        let mut row = fixture_row();
        row.report = true;
        row.report_amount_iqd = 3_000;
        // Net = collected - doctor_cut - operator_cut - report_amount.
        row.net_iqd = 50_000 - 20_000 - 5_000 - 3_000;
        let totals = VisitsReportTotals {
            visits: 1,
            revenue_iqd: 50_000,
            doctor_cut_iqd: 20_000,
            operator_cut_iqd: 5_000,
            report_iqd: 3_000,
            mandoub_cut_iqd: 0,
            net_iqd: 22_000,
        };
        write_visits_csv(&[row], &totals, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        // Header carries the Report (IQD) column.
        let header = text.lines().next().unwrap();
        assert!(header.contains("Report (IQD)"));
        // Footer: report total 3000 then mandoub 0 sit before the net 22000.
        assert!(text.contains("TOTAL,,,,,,,,,50000,20000,5000,3000,0,22000"));
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

    /// §7.7 doctor header is exact + footer column count matches body width
    /// (Pass-2 P07-G19).
    #[test]
    fn doctor_csv_footer_column_count_matches_header_and_sums_aggregate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doctors.csv");
        let rows = vec![
            DoctorEarningsRow {
                doctor_id: Some(Uuid::nil()),
                name: "Dr. A".into(),
                specialty: Some("Spec".into()),
                visits: 2,
                revenue_iqd: 10_000,
                doctor_cut_total_iqd: 3_000,
                avg_cut_per_visit_iqd: 1_500,
            },
            DoctorEarningsRow {
                doctor_id: Some(Uuid::nil()),
                name: "Dr. B".into(),
                specialty: None,
                visits: 3,
                revenue_iqd: 30_000,
                doctor_cut_total_iqd: 9_000,
                avg_cut_per_visit_iqd: 3_000,
            },
        ];
        write_doctor_earnings_csv(&rows, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let header_line = text.lines().next().unwrap();
        let footer_line = text.lines().last().unwrap();
        let header_cols = header_line.split(',').count();
        let footer_cols = footer_line.split(',').count();
        assert_eq!(header_cols, 6);
        assert_eq!(footer_cols, 6);
        // Body sums: visits=5, revenue=40000, cut=12000, avg=12000/5=2400.
        assert!(footer_line.contains(",5,40000,12000,2400"));
    }

    /// Operators CSV header + footer parity (Pass-2 P07-G19 mirror).
    #[test]
    fn operator_csv_has_bom_and_header_with_hours_column() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("operators.csv");
        let rows = vec![
            OperatorEarningsRow {
                operator_id: Uuid::nil(),
                name: "Op A".into(),
                visits: 4,
                visits_with_dye: 2,
                operator_cut_total_iqd: 16_000,
                hours_on_shift_milli: 4 * 3_600_000,
                avg_cut_per_hour_iqd: 4_000,
            },
            OperatorEarningsRow {
                operator_id: Uuid::nil(),
                name: "Op B".into(),
                visits: 2,
                visits_with_dye: 0,
                operator_cut_total_iqd: 8_000,
                hours_on_shift_milli: 2 * 3_600_000,
                avg_cut_per_hour_iqd: 4_000,
            },
        ];
        write_operator_earnings_csv(&rows, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // BOM.
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        // Header has exactly 6 cells including Hours On Shift.
        let header_line = text.lines().next().unwrap();
        assert_eq!(header_line.split(',').count(), 6);
        assert!(header_line.contains("Hours On Shift"));
        // Footer sums + decimal hours_total.
        let footer = text.lines().last().unwrap();
        assert!(footer.starts_with("TOTAL"));
        // hours sum = 6h => "6.00"
        assert!(footer.contains(",6.00,"));
    }

    /// مندوب CSV: BOM + a 4-column header (no Hours/Dye) + a summing footer.
    #[test]
    fn mandoub_csv_has_bom_header_and_summing_footer() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mandoubs.csv");
        let rows = vec![
            MandoubEarningsRow {
                mandoub_id: Uuid::nil(),
                name: "Rep A".into(),
                visits: 4,
                mandoub_cut_total_iqd: 4_000,
                avg_cut_per_visit_iqd: 1_000,
            },
            MandoubEarningsRow {
                mandoub_id: Uuid::nil(),
                name: "Rep B".into(),
                visits: 2,
                mandoub_cut_total_iqd: 1_000,
                avg_cut_per_visit_iqd: 500,
            },
        ];
        write_mandoub_earnings_csv(&rows, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        let header_line = text.lines().next().unwrap();
        assert_eq!(header_line.split(',').count(), 4);
        assert!(header_line.contains("Mandoub Cut Total (IQD)"));
        // Footer sums: visits 6, cut 5000, avg 5000/6 = 833.
        let footer = text.lines().last().unwrap();
        assert!(footer.starts_with("TOTAL"));
        assert!(footer.contains(",6,5000,833"));
    }

    #[test]
    fn write_atomic_leaves_no_tmp_file_on_success_mandoubs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mandoubs.csv");
        write_mandoub_earnings_csv(&[], &path).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1);
    }

    /// §7.25: rows in the visits CSV come out sorted by (locked_at ASC, visit_id ASC).
    #[test]
    fn visits_csv_sorts_rows_by_locked_at_then_visit_id() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("visits.csv");
        let earlier = Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap();
        let later = Utc.with_ymd_and_hms(2026, 5, 12, 17, 0, 0).unwrap();
        let mut row_later = fixture_row();
        row_later.visit_id = Uuid::parse_str("01900000-0000-7000-8000-000000000002").unwrap();
        row_later.patient_name = "Beta".into();
        row_later.locked_at = Some(later);
        let mut row_earlier = fixture_row();
        row_earlier.visit_id = Uuid::parse_str("01900000-0000-7000-8000-000000000003").unwrap();
        row_earlier.patient_name = "Alpha".into();
        row_earlier.locked_at = Some(earlier);
        let totals = VisitsReportTotals::default();
        // Pass rows in reverse-time order; writer must sort to (Alpha, Beta).
        write_visits_csv(&[row_later, row_earlier], &totals, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let alpha_idx = text.find("Alpha").unwrap();
        let beta_idx = text.find("Beta").unwrap();
        assert!(alpha_idx < beta_idx, "Alpha must precede Beta");
    }

    /// `quote_field` covers comma, quote, newline; bare strings pass through.
    #[test]
    fn quote_field_only_quotes_when_special_chars_present() {
        assert_eq!(quote_field("plain text"), "plain text");
        assert_eq!(quote_field("hello, world"), "\"hello, world\"");
        assert_eq!(quote_field("she said \"hi\""), "\"she said \"\"hi\"\"\"");
        assert_eq!(quote_field("line1\nline2"), "\"line1\nline2\"");
        // CR alone also triggers quoting per RFC 4180.
        assert_eq!(quote_field("a\rb"), "\"a\rb\"");
    }

    /// `visit_number` formats as `V-<last 6 simple-hex chars>`.
    #[test]
    fn visit_number_uses_last_six_simple_hex_chars() {
        let id = Uuid::parse_str("01900000-0000-7000-8000-0000000000ab").unwrap();
        assert_eq!(visit_number(id), "V-0000ab");
    }

    /// Atomic rename: the writer leaves no `.tmp` file in the parent
    /// directory after a successful write (Pass-2 P07-G26 / phase-07 §6.5).
    #[test]
    fn write_atomic_leaves_no_tmp_file_on_success_visits() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("visits.csv");
        let totals = VisitsReportTotals::default();
        write_visits_csv(&[fixture_row()], &totals, &path).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        // Only the destination file remains; no stray ".visits.csv.tmp".
        assert_eq!(entries.len(), 1);
        let only = entries[0].to_string_lossy().to_string();
        assert_eq!(only, "visits.csv");
        assert!(!only.starts_with('.'));
        assert!(!only.ends_with(".tmp"));
    }

    /// Mirror atomic-rename invariant on doctor + operator writers.
    #[test]
    fn write_atomic_leaves_no_tmp_file_on_success_doctors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doctors.csv");
        write_doctor_earnings_csv(&[], &path).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn write_atomic_leaves_no_tmp_file_on_success_operators() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("operators.csv");
        write_operator_earnings_csv(&[], &path).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1);
    }

    /// Empty rows + zero totals still produce a valid CSV with header + footer
    /// and BOM. Used by the empty-day code path (phase-07 §6.5).
    #[test]
    fn visits_csv_with_zero_rows_still_renders_header_and_footer() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("visits.csv");
        let totals = VisitsReportTotals::default();
        write_visits_csv(&[], &totals, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Date,Visit #,Patient"));
        assert!(lines[1].starts_with("TOTAL"));
    }
}
