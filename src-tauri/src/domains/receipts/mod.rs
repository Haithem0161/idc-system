//! Receipt generation (PRD §8.1 step 10-12).
//!
//! Renders two artifacts per locked visit: an A5-equivalent text receipt
//! and a thermal text receipt. Both consume the visit's snapshot block
//! exclusively -- never re-joins to the catalog (§7.17 immutability).
//!
//! The phase-05 plan locks the contract `ReceiptArtifacts { a5_path,
//! thermal_path }`. The MVP renderer emits plain-text UTF-8 files; a
//! future iteration may swap to a true PDF crate without changing the
//! caller. Both files live under `$APPDATA/idc-system/receipts/<yyyy>/<mm>/`.

use std::path::{Path, PathBuf};

use chrono::{Datelike, Utc};
use serde::Serialize;

use crate::domains::visits::domain::entities::{Visit, VisitSnapshots};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
pub struct ReceiptArtifacts {
    pub a5_path: PathBuf,
    pub thermal_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ReceiptRenderOptions {
    pub clinic_name: String,
    pub thermal_width: u32,
    pub arabic_numerals: bool,
    pub currency_symbol: String,
}

impl Default for ReceiptRenderOptions {
    fn default() -> Self {
        Self {
            clinic_name: "IDC".into(),
            thermal_width: 32,
            arabic_numerals: false,
            currency_symbol: "IQD".into(),
        }
    }
}

/// Map a Western digit to its Arabic-Indic equivalent.
fn to_arabic_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '٠',
            '1' => '١',
            '2' => '٢',
            '3' => '٣',
            '4' => '٤',
            '5' => '٥',
            '6' => '٦',
            '7' => '٧',
            '8' => '٨',
            '9' => '٩',
            other => other,
        })
        .collect()
}

fn fmt_amount(amount: i64, opts: &ReceiptRenderOptions) -> String {
    let abs = amount.unsigned_abs();
    let sign = if amount < 0 { "-" } else { "" };
    let raw = format!("{sign}{abs}");
    let mut with_commas = String::new();
    let chars: Vec<char> = raw.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let remaining = chars.len() - i;
        with_commas.push(*c);
        if remaining > 1 && (remaining - 1) % 3 == 0 && *c != '-' {
            with_commas.push(',');
        }
    }
    let body = if opts.arabic_numerals {
        to_arabic_digits(&with_commas)
    } else {
        with_commas
    };
    format!("{body} {}", opts.currency_symbol)
}

fn pad_columns(left: &str, right: &str, width: usize) -> String {
    let total = left.chars().count() + right.chars().count();
    if total >= width {
        return format!("{left} {right}");
    }
    let spaces = width - total;
    format!("{left}{}{right}", " ".repeat(spaces))
}

/// Build the thermal-printer body. Width is configurable (typically 32 or 48).
pub fn build_thermal(visit: &Visit, snap: &VisitSnapshots, opts: &ReceiptRenderOptions) -> String {
    let width = opts.thermal_width.max(20) as usize;
    let mut out = String::new();
    let pushln = |out: &mut String, s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    let sep = |out: &mut String| pushln(out, &"-".repeat(width));

    // Header
    pushln(&mut out, &center(&opts.clinic_name, width));
    pushln(&mut out, &center("RECEIPT", width));
    sep(&mut out);

    pushln(&mut out, &format!("Receipt: {}", visit.id));
    if let Some(at) = visit.locked_at {
        pushln(&mut out, &format!("Date:    {}", at.to_rfc3339()));
    }
    sep(&mut out);

    // Body
    pushln(&mut out, &format!("Patient: {}", snap.patient_name));
    if let Some(doctor) = &snap.doctor_name {
        pushln(&mut out, &format!("Doctor:  {doctor}"));
    } else {
        pushln(&mut out, "Doctor:  (house)");
    }
    pushln(&mut out, &format!("Operator:{}", snap.operator_name));
    pushln(&mut out, &format!("Check:   {}", snap.check_type_name_ar));
    if let Some(sub) = &snap.check_subtype_name_ar {
        pushln(&mut out, &format!("Subtype: {sub}"));
    }
    sep(&mut out);

    // Money block (right-aligned amounts).
    pushln(
        &mut out,
        &pad_columns("Price", &fmt_amount(snap.price_iqd, opts), width),
    );
    if snap.dye_cost_iqd > 0 {
        pushln(
            &mut out,
            &pad_columns("Dye", &fmt_amount(snap.dye_cost_iqd, opts), width),
        );
    }
    // Report is an internal net-side carve-out, not part of the patient bill,
    // so it never appears on the patient receipt.
    sep(&mut out);
    pushln(
        &mut out,
        &pad_columns("TOTAL", &fmt_amount(snap.total_amount_iqd, opts), width),
    );
    sep(&mut out);
    pushln(&mut out, &center("Thank you", width));
    out
}

/// Build the A5 text body. Wider page; no truncation.
pub fn build_a5(visit: &Visit, snap: &VisitSnapshots, opts: &ReceiptRenderOptions) -> String {
    let width = 64;
    let mut out = String::new();
    let pushln = |out: &mut String, s: &str| {
        out.push_str(s);
        out.push('\n');
    };

    pushln(&mut out, &center(&opts.clinic_name, width));
    pushln(&mut out, &center("RECEIPT (A5)", width));
    pushln(&mut out, &"=".repeat(width));
    pushln(&mut out, &format!("Receipt id : {}", visit.id));
    if let Some(at) = visit.locked_at {
        pushln(&mut out, &format!("Issued at  : {}", at.to_rfc3339()));
    }
    pushln(
        &mut out,
        &format!("Receptionist: {}", visit.receptionist_user_id),
    );
    pushln(&mut out, "");
    pushln(&mut out, &format!("Patient    : {}", snap.patient_name));
    pushln(
        &mut out,
        &format!(
            "Doctor     : {}",
            snap.doctor_name.as_deref().unwrap_or("(house)")
        ),
    );
    pushln(&mut out, &format!("Operator   : {}", snap.operator_name));
    pushln(
        &mut out,
        &format!(
            "Check      : {} / {}",
            snap.check_type_name_ar,
            snap.check_type_name_en.as_deref().unwrap_or("-")
        ),
    );
    if let Some(sub) = &snap.check_subtype_name_ar {
        pushln(&mut out, &format!("Subtype    : {sub}"));
    }
    pushln(&mut out, "");
    pushln(
        &mut out,
        &pad_columns("Price", &fmt_amount(snap.price_iqd, opts), width),
    );
    pushln(
        &mut out,
        &pad_columns("Dye cost", &fmt_amount(snap.dye_cost_iqd, opts), width),
    );
    // The receipt shows only patient-facing amounts (price, dye, total). The
    // doctor / operator / report / مندوب cuts are internal net-side accounting
    // figures and never appear on any printed receipt.
    pushln(&mut out, &"-".repeat(width));
    pushln(
        &mut out,
        &pad_columns("TOTAL DUE", &fmt_amount(snap.total_amount_iqd, opts), width),
    );
    pushln(&mut out, "");
    pushln(&mut out, &center("Generated by IDC", width));
    out
}

fn center(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        return s.into();
    }
    let pad = (width - len) / 2;
    format!("{}{s}", " ".repeat(pad))
}

/// Render both artifacts and write them under the receipts root. Returns
/// the resolved paths. Returns an error if the visit has no snapshots
/// (only locked / voided visits can be receipted).
pub fn render(
    visit: &Visit,
    options: &ReceiptRenderOptions,
    receipts_root: &Path,
) -> AppResult<ReceiptArtifacts> {
    let snap = visit.snapshots.as_ref().ok_or_else(|| {
        AppError::Validation("cannot render receipt for visit without snapshots".into())
    })?;
    let at = visit.locked_at.unwrap_or_else(Utc::now);
    let dir = receipts_root
        .join(format!("{:04}", at.year()))
        .join(format!("{:02}", at.month()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(format!("create receipts dir: {e}")))?;
    let a5_path = dir.join(format!("{}.pdf.txt", visit.id));
    let thermal_path = dir.join(format!("{}.thermal.txt", visit.id));

    // Render to memory FIRST (§7.16): avoids long disk I/O while we hold
    // any concurrent caller's WAL guarantee. The caller already commits
    // before writing; we keep the pattern intact.
    let a5_body = build_a5(visit, snap, options);
    let thermal_body = build_thermal(visit, snap, options);

    // Atomic write via temp + rename.
    write_atomic(&a5_path, a5_body.as_bytes())?;
    write_atomic(&thermal_path, thermal_body.as_bytes())?;

    Ok(ReceiptArtifacts {
        a5_path,
        thermal_path,
    })
}

fn write_atomic(target: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = target.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| AppError::Internal(format!("write {:?}: {e}", tmp)))?;
    std::fs::rename(&tmp, target)
        .map_err(|e| AppError::Internal(format!("rename {:?} -> {:?}: {e}", tmp, target)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn snap() -> VisitSnapshots {
        VisitSnapshots {
            price_iqd: 50000,
            dye_cost_iqd: 2000,
            report_amount_iqd: 0,
            report_pct: None,
            reporting_doctor_name: None,
            doctor_cut_iqd: 12500,
            operator_cut_iqd: 5000,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            internal_pct: None,
            total_amount_iqd: 52000,
            amount_paid_override_iqd: None,
            patient_name: "Ahmed".into(),
            doctor_name: Some("Dr. Sara".into()),
            operator_name: "Op One".into(),
            check_type_name_ar: "اختبار".into(),
            check_type_name_en: Some("Test".into()),
            check_subtype_name_ar: None,
            check_subtype_name_en: None,
        }
    }

    fn visit(snap: VisitSnapshots) -> Visit {
        let now = Utc::now();
        Visit {
            id: Uuid::now_v7(),
            patient_id: Uuid::now_v7(),
            status: crate::domains::visits::domain::entities::VisitStatus::Locked,
            receptionist_user_id: Uuid::now_v7(),
            check_type_id: Uuid::now_v7(),
            check_subtype_id: None,
            doctor_id: Some(Uuid::now_v7()),
            operator_id: Some(Uuid::now_v7()),
            mandoub_id: None,
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            price_override_iqd: None,
            locked_at: Some(now),
            voided_at: None,
            voided_by_user_id: None,
            void_reason: None,
            snapshots: Some(snap),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 2,
            dirty: true,
            last_synced_at: None,
            origin_device_id: None,
            entity_id: "t".into(),
        }
    }

    #[test]
    fn thermal_includes_total() {
        let opts = ReceiptRenderOptions::default();
        let v = visit(snap());
        let body = build_thermal(&v, v.snapshots.as_ref().unwrap(), &opts);
        assert!(body.contains("TOTAL"));
        assert!(body.contains("52,000"));
    }

    #[test]
    fn a5_uses_arabic_digits_when_enabled() {
        let opts = ReceiptRenderOptions {
            arabic_numerals: true,
            ..Default::default()
        };
        let v = visit(snap());
        let body = build_a5(&v, v.snapshots.as_ref().unwrap(), &opts);
        assert!(body.contains('٢'));
    }

    /// Receipts show only patient-facing amounts (price / dye / total). The
    /// doctor / operator / report / مندوب cuts are internal accounting figures
    /// and must NEVER appear on any printed receipt.
    #[test]
    fn receipts_never_show_cuts() {
        let opts = ReceiptRenderOptions::default();
        let v = visit(snap());
        let s = v.snapshots.as_ref().unwrap();
        for body in [build_a5(&v, s, &opts), build_thermal(&v, s, &opts)] {
            let lower = body.to_lowercase();
            assert!(
                !lower.contains("cut"),
                "receipt must not print any cut line; got:\n{body}"
            );
            // The doctor-cut and operator-cut amounts (12,500 / 5,000) must not
            // leak onto the receipt either.
            assert!(!body.contains("12,500"), "doctor cut leaked onto receipt");
            assert!(!body.contains("5,000"), "operator cut leaked onto receipt");
        }
    }
}
