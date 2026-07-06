//! Money-math for visit pricing (PRD §4.1 + §6.1.5 inv 5).
//!
//! Computes a `VisitSnapshots` block for either house-mode (no doctor) or
//! doctor-mode (doctor_id set + DoctorCheckPricing row). Pure logic with no
//! I/O; the caller resolves all references and feeds them in.

use crate::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, Operator,
};
use crate::domains::visits::domain::entities::VisitSnapshots;
use crate::error::{AppError, AppResult};

/// The دلال (dalal) money mode takes a built-in flat doctor cut: it is a
/// doctor substitute that never resolves to a `doctors` row, so the cut is a
/// fixed constant rather than a pct/fixed negotiation.
const DALAL_CUT_IQD: i64 = 10_000;

#[derive(Debug, Clone)]
pub struct MoneySettings {
    /// Percentage (0..=100) carved out of the price-after-doctor-cut and paid
    /// to the internal reporting doctor when the visit's `report` flag is on.
    pub report_pct: i64,
    /// Name of the single internal reporting doctor who receives every report
    /// amount. Captured into the snapshot when report is on (and non-empty).
    pub reporting_doctor_name: String,
    pub internal_doctor_pct: i64,
}

pub struct MoneyMathInputs<'a> {
    pub check_type: &'a CheckType,
    pub check_subtype: Option<&'a CheckSubtype>,
    pub doctor: Option<&'a Doctor>,
    pub doctor_pricing: Option<&'a DoctorCheckPricing>,
    pub operator: &'a Operator,
    pub patient_name: &'a str,
    pub dye: bool,
    pub report: bool,
    pub dalal: bool,
    /// Discount mode: when true AND a real referring doctor is present, the
    /// doctor's cut for this visit is forced to 0. It is the only thing the flag
    /// changes -- the operator cut, the مندوب cut, and the patient total are
    /// untouched. The report carve-out, being a pct of `price - doctor_cut`,
    /// naturally widens to `price` since the doctor cut is now 0.
    pub discount: bool,
    /// مندوب (representative) per-visit cut, chosen on the visit: 500 or 1000
    /// when a مندوب is referenced, 0 otherwise. PURE PASSTHROUGH -- it is copied
    /// straight into the snapshot and does NOT route through `cuts()`; it never
    /// changes the doctor cut, the operator cut, or the report base. The
    /// net-side subtraction happens later in the reports read-model.
    pub mandoub_cut_iqd: i64,
    /// مندوب name, copied into the snapshot alongside the cut. `Some(name)` when
    /// a مندوب is referenced, `None` otherwise.
    pub mandoub_name: Option<&'a str>,
    /// Receptionist-editable price for this visit. `Some(p)` overrides the
    /// catalog/subtype/pricing price; `None` uses the resolved catalog price.
    /// Becomes the snapshot `price_iqd` and the default "collected" basis.
    pub price_override_iqd: Option<i64>,
    /// Cash actually collected. `Some(c)` (incl. 0) is the collected amount;
    /// `None` means the patient paid the full price. The doctor-side cuts scale
    /// off `max(0, collected - dye)`.
    pub amount_paid_override_iqd: Option<i64>,
    pub settings: MoneySettings,
}

/// Compute the snapshot block. Caller has already validated subtype /
/// dye / report consistency at the entity level; this function is strict
/// about money invariants only.
pub fn compute(inputs: &MoneyMathInputs<'_>) -> AppResult<VisitSnapshots> {
    if inputs.dye && dye_price(inputs).is_none() {
        return Err(AppError::Validation(
            "dye not available for this check".into(),
        ));
    }
    if inputs.check_type.has_subtypes && inputs.check_subtype.is_none() {
        return Err(AppError::Validation(
            "check type has subtypes; subtype id required".into(),
        ));
    }
    if !inputs.check_type.has_subtypes && inputs.check_subtype.is_some() {
        return Err(AppError::Validation(
            "check type does not allow a subtype".into(),
        ));
    }

    // مندوب coherence guard: a non-zero مندوب cut implies a real referring
    // doctor (the مندوب is referenced only when a doctor is selected). The cut
    // itself is pure passthrough below -- this guard only rejects an
    // impossible combination before the snapshot is built.
    if inputs.mandoub_cut_iqd != 0 && inputs.doctor.is_none() {
        return Err(AppError::Validation(
            "mandoub cut requires a referring doctor".into(),
        ));
    }

    // Discount coherence guard: the discount zeroes the referring doctor's cut,
    // so it only makes sense when a real referring doctor is present.
    if inputs.discount && inputs.doctor.is_none() {
        return Err(AppError::Validation(
            "discount requires a referring doctor".into(),
        ));
    }

    let base_price = base_price(inputs)?;
    let catalog_price = effective_price(base_price, inputs.doctor_pricing);
    // Receptionist-editable price wins over the catalog/pricing price.
    let price_iqd = inputs.price_override_iqd.unwrap_or(catalog_price);
    if price_iqd < 0 {
        return Err(AppError::Validation(
            "price_override_iqd must be >= 0".into(),
        ));
    }

    let dye_cost = if inputs.dye {
        dye_price(inputs)
            .ok_or_else(|| AppError::Validation("dye not available for this check".into()))?
    } else {
        0
    };

    // Collected cash defaults to the (editable) price when no override is set.
    let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd);
    if collected < 0 {
        return Err(AppError::Validation(
            "amount_paid_override_iqd must be >= 0".into(),
        ));
    }
    // Cut base: collected minus dye (dye is a material cost, covered first).
    // When this hits 0, EVERY cut zeroes -- fixed and scaled alike.
    let base = cut_base(price_iqd, collected, dye_cost);

    // Compute every cut off the paid-net-of-dye base. When the base is 0
    // (patient did not cover the dye) the zero-guard fires: nobody is paid,
    // not even fixed entities, but internal_pct still marks house mode so the
    // lock invariant that a house visit carries an internal_pct is preserved.
    let (doctor_cut, internal_pct, operator_cut, mandoub_cut, report_amount) = if base == 0 {
        let internal_pct = if inputs.doctor.is_none() && !inputs.dalal {
            if !(0..=100).contains(&inputs.settings.internal_doctor_pct) {
                return Err(AppError::Validation(
                    "internal_doctor_pct must be 0..=100".into(),
                ));
            }
            Some(inputs.settings.internal_doctor_pct)
        } else {
            None
        };
        (0, internal_pct, 0, 0, 0)
    } else {
        let (computed_doctor_cut, internal_pct, operator_cut) = cuts(
            base,
            inputs.operator,
            inputs.doctor,
            inputs.dalal,
            inputs.doctor_pricing,
            &inputs.settings,
        )?;

        // Discount forces the referring doctor's cut to 0 for this visit.
        // Applied here, BEFORE the report base, so the report carve-out (a pct
        // of `cut_base - doctor_cut`) sees the zeroed cut and widens
        // accordingly. The discount is only valid with a real referring doctor
        // (guarded above and at the entity level), so `internal_pct` is
        // necessarily None here.
        let doctor_cut = if inputs.discount && inputs.doctor.is_some() {
            0
        } else {
            computed_doctor_cut
        };

        // Report is a net-side carve-out, not part of the patient bill. It is a
        // percentage of the cut base AFTER the doctor cut (excluding dye and
        // the operator cut) paid to the internal reporting doctor.
        let report_amount = if inputs.report {
            if !(0..=100).contains(&inputs.settings.report_pct) {
                return Err(AppError::Validation("report_pct must be 0..=100".into()));
            }
            (base - doctor_cut).max(0) * inputs.settings.report_pct / 100
        } else {
            0
        };

        (
            doctor_cut,
            internal_pct,
            operator_cut,
            inputs.mandoub_cut_iqd,
            report_amount,
        )
    };
    let report_pct = inputs.report.then_some(inputs.settings.report_pct);
    let reporting_doctor_name =
        if inputs.report && !inputs.settings.reporting_doctor_name.trim().is_empty() {
            Some(inputs.settings.reporting_doctor_name.clone())
        } else {
            None
        };

    // Patient total no longer includes report.
    let total = price_iqd + dye_cost;

    Ok(VisitSnapshots {
        price_iqd,
        dye_cost_iqd: dye_cost,
        report_amount_iqd: report_amount,
        report_pct,
        reporting_doctor_name,
        doctor_cut_iqd: doctor_cut,
        operator_cut_iqd: operator_cut,
        // مندوب cut + name are PURE PASSTHROUGH: copied straight from the
        // visit-chosen inputs into the snapshot. They never went through
        // cuts() and never perturbed the doctor/operator cut or the report base.
        // The cut is zeroed by the zero-guard when the cut base is 0; the name
        // still follows the existing rule so a referenced مندوب is captured.
        mandoub_cut_iqd: mandoub_cut,
        mandoub_name: inputs.mandoub_name.map(|s| s.to_string()),
        internal_pct,
        total_amount_iqd: total,
        // The collected amount now flows through the engine: it drives the cut
        // base and is recorded on the snapshot so the lock read-back agrees.
        amount_paid_override_iqd: inputs.amount_paid_override_iqd,
        patient_name: inputs.patient_name.to_string(),
        doctor_name: inputs.doctor.map(|d| d.name.clone()),
        operator_name: inputs.operator.name.clone(),
        check_type_name_ar: inputs.check_type.name_ar.clone(),
        check_type_name_en: inputs.check_type.name_en.clone(),
        check_subtype_name_ar: inputs.check_subtype.map(|s| s.name_ar.clone()),
        check_subtype_name_en: inputs.check_subtype.and_then(|s| s.name_en.clone()),
    })
}

fn base_price(inputs: &MoneyMathInputs<'_>) -> AppResult<i64> {
    if let Some(sub) = inputs.check_subtype {
        return Ok(sub.price_iqd);
    }
    inputs.check_type.base_price_iqd.ok_or_else(|| {
        AppError::Validation("check type has no base price; subtype required".into())
    })
}

/// Resolve the catalog dye price: the subtype's `dye_price_iqd` when a
/// subtype is chosen, else the check type's. `None` is a legal "no dye
/// offered for this check" answer, not an error -- callers decide what to do
/// with it (reject dye-on, or treat as unavailable).
fn dye_price(inputs: &MoneyMathInputs<'_>) -> Option<i64> {
    match inputs.check_subtype {
        Some(sub) => sub.dye_price_iqd,
        None => inputs.check_type.dye_price_iqd,
    }
}

fn effective_price(base: i64, pricing: Option<&DoctorCheckPricing>) -> i64 {
    match pricing {
        Some(p) => p.price_override_iqd.unwrap_or(base),
        None => base,
    }
}

/// The base every cut is measured against: collected cash net of dye, floored
/// at zero. When it is zero, no cut (fixed or scaled) is paid. `price_iqd` is
/// accepted for signature symmetry with the design spec but is not needed for
/// the computation (the base is purely collected - dye).
fn cut_base(_price_iqd: i64, collected: i64, dye_cost: i64) -> i64 {
    (collected - dye_cost).max(0)
}

/// Resolve a doctor cut from a `(kind, value)` pair against the cut base.
/// Shared by the per-check `DoctorCheckPricing` override and the doctor-level
/// default cut so the pct/fixed math never drifts between the two.
fn cut_from_kind_value(cut_base: i64, kind: &str, value: i64) -> AppResult<i64> {
    match kind {
        "pct" => {
            if !(0..=100).contains(&value) {
                return Err(AppError::Validation(
                    "doctor cut percentage must be 0..=100".into(),
                ));
            }
            Ok(cut_base * value / 100)
        }
        "fixed" => Ok(value.max(0)),
        other => Err(AppError::Validation(format!("unknown cut_kind: {other}"))),
    }
}

fn cuts(
    cut_base: i64,
    operator: &Operator,
    doctor: Option<&Doctor>,
    dalal: bool,
    pricing: Option<&DoctorCheckPricing>,
    settings: &MoneySettings,
) -> AppResult<(i64, Option<i64>, i64)> {
    let operator_cut = operator.base_cut_per_check_iqd;

    // دلال is a doctor substitute and is mutually exclusive with a referring
    // doctor; a dalal visit always has `doctor_id` None. Reject the impossible
    // combo defensively before dispatching on the money mode.
    if dalal && doctor.is_some() {
        return Err(AppError::Validation(
            "dalal cannot coexist with a referring doctor".into(),
        ));
    }

    // دلال takes precedence over house mode: a flat built-in cut, no
    // internal_pct (it is not the house-employed doctor).
    if dalal {
        return Ok((DALAL_CUT_IQD, None, operator_cut));
    }

    match (doctor, pricing) {
        (_, Some(p)) => {
            // Per-check override wins over everything: explicit cut for this
            // exact doctor + check (+ subtype).
            let doctor_cut = cut_from_kind_value(cut_base, p.cut_kind.as_str(), p.cut_value)?;
            Ok((doctor_cut, None, operator_cut))
        }
        (Some(d), None) => {
            // Referring doctor selected but no per-check DoctorCheckPricing row.
            // Fall back to the doctor's negotiated DEFAULT cut when configured;
            // otherwise the cut is zero (the historical behaviour). `internal_pct`
            // MUST stay None -- it is the house-mode marker and Visit::lock
            // rejects a doctor visit that carries one (invariant 6).
            let doctor_cut = match (d.default_cut_kind.as_deref(), d.default_cut_value) {
                (Some(kind), Some(value)) => cut_from_kind_value(cut_base, kind, value)?,
                _ => 0,
            };
            Ok((doctor_cut, None, operator_cut))
        }
        (None, None) => {
            // House / internal mode: doctor_cut snapshot is the absolute share
            // earned by the clinic-employed doctor, expressed via
            // `internal_doctor_pct`.
            if settings.internal_doctor_pct < 0 || settings.internal_doctor_pct > 100 {
                return Err(AppError::Validation(
                    "internal_doctor_pct must be 0..=100".into(),
                ));
            }
            let doctor_cut = cut_base * settings.internal_doctor_pct / 100;
            Ok((doctor_cut, Some(settings.internal_doctor_pct), operator_cut))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn ct(has_subtypes: bool, base: Option<i64>) -> CheckType {
        ct_dye(has_subtypes, base, Some(2_000))
    }

    /// Like `ct`, but with an explicit catalog dye price (`None` = dye not
    /// offered for this check type).
    fn ct_dye(has_subtypes: bool, base: Option<i64>, dye_price_iqd: Option<i64>) -> CheckType {
        let now = Utc::now();
        CheckType {
            id: Uuid::now_v7(),
            name_ar: "اختبار".into(),
            name_en: Some("Test".into()),
            has_subtypes,
            base_price_iqd: base,
            dye_price_iqd,
            sort_order: 0,
            is_active: true,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: None,
            origin_device_id: None,
            entity_id: "t".into(),
        }
    }

    fn operator() -> Operator {
        let now = Utc::now();
        Operator {
            id: Uuid::now_v7(),
            name: "Op".into(),
            phone: None,
            base_cut_per_check_iqd: 5000,
            is_active: true,
            notes: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: None,
            origin_device_id: None,
            entity_id: "t".into(),
        }
    }

    fn settings() -> MoneySettings {
        MoneySettings {
            report_pct: 20,
            reporting_doctor_name: "Dr Report".into(),
            internal_doctor_pct: 40,
        }
    }

    // ---- Phase 05 plan §1.1: money_math coverage matrix ------------------

    use crate::domains::catalog::domain::value_objects::CutKind;

    fn doctor() -> Doctor {
        let now = Utc::now();
        Doctor {
            id: Uuid::now_v7(),
            name: "Dr Sara".into(),
            specialty: None,
            phone: None,
            notes: None,
            default_cut_kind: None,
            default_cut_value: None,
            is_active: true,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: None,
            origin_device_id: None,
            entity_id: "t".into(),
        }
    }

    fn doctor_with_default_cut(kind: &str, value: i64) -> Doctor {
        Doctor {
            default_cut_kind: Some(kind.into()),
            default_cut_value: Some(value),
            ..doctor()
        }
    }

    fn sub(check_type_id: Uuid, price: i64) -> CheckSubtype {
        sub_dye(check_type_id, price, None)
    }

    /// Like `sub`, but with an explicit subtype-level dye price override.
    fn sub_dye(check_type_id: Uuid, price: i64, dye_price_iqd: Option<i64>) -> CheckSubtype {
        let now = Utc::now();
        CheckSubtype {
            id: Uuid::now_v7(),
            check_type_id,
            name_ar: "فرعي".into(),
            name_en: Some("Sub".into()),
            price_iqd: price,
            dye_price_iqd,
            sort_order: 0,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: None,
            origin_device_id: None,
            entity_id: "t".into(),
        }
    }

    fn pricing(
        doctor_id: Uuid,
        check_type_id: Uuid,
        kind: CutKind,
        value: i64,
        override_price: Option<i64>,
    ) -> DoctorCheckPricing {
        let now = Utc::now();
        DoctorCheckPricing {
            id: Uuid::now_v7(),
            doctor_id,
            check_type_id,
            check_subtype_id: None,
            price_override_iqd: override_price,
            cut_kind: kind,
            cut_value: value,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: None,
            origin_device_id: None,
            entity_id: "t".into(),
        }
    }

    // ---- house / doctor / dye coverage (preserved under the new model) ---

    #[test]
    fn flat_house_with_dye() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 50_000);
        assert_eq!(snap.dye_cost_iqd, 2_000);
        // Report off: no carve-out, no pct/name snapshots.
        assert_eq!(snap.report_amount_iqd, 0);
        assert_eq!(snap.report_pct, None);
        assert_eq!(snap.reporting_doctor_name, None);
        // Paid basis: dye now reduces the cut base, so cut_base = 50000 - 2000
        // = 48000 and doctor_cut = 48000*40/100 = 19200 (was 20000 off price).
        assert_eq!(snap.doctor_cut_iqd, 19_200);
        assert_eq!(snap.internal_pct, Some(40));
        assert_eq!(snap.operator_cut_iqd, 5_000);
        // Patient total = price + dye only.
        assert_eq!(snap.total_amount_iqd, 52_000);
    }

    #[test]
    fn total_equals_price_plus_dye_invariant_excludes_report() {
        // Even with report on, the patient total is price + dye; the report
        // amount is a separate net-side carve-out.
        let ct = ct(false, Some(75_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: true,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.total_amount_iqd, snap.price_iqd + snap.dye_cost_iqd);
        assert!(snap.report_amount_iqd > 0);
        // The report amount is NOT added to the patient total.
        assert_ne!(
            snap.total_amount_iqd,
            snap.price_iqd + snap.dye_cost_iqd + snap.report_amount_iqd
        );
    }

    #[test]
    fn rejects_dye_when_unsupported() {
        let mut t = ct(false, Some(50_000));
        t.dye_price_iqd = None;
        let op = operator();
        let err = compute(&MoneyMathInputs {
            check_type: &t,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err.is_err());
    }

    // ---- report carve-out tests (new model) ------------------------------

    #[test]
    fn report_amount_is_pct_of_price_after_doctor_cut_in_house_mode() {
        // House mode: doctor_cut = 40% of 50000 = 20000.
        // report base = price - doctor_cut = 30000; 20% = 6000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 20_000);
        assert_eq!(snap.report_amount_iqd, 6_000);
        assert_eq!(snap.report_pct, Some(20));
        assert_eq!(snap.reporting_doctor_name.as_deref(), Some("Dr Report"));
    }

    #[test]
    fn report_off_zeroes_amount_and_nulls_pct_and_name() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.report_amount_iqd, 0);
        assert_eq!(snap.report_pct, None);
        assert_eq!(snap.reporting_doctor_name, None);
    }

    #[test]
    fn report_base_uses_cut_base_which_dye_reduces() {
        // Paid basis: dye now comes out of the collected cash first, so it
        // reduces the cut base and therefore the report base too.
        //   with dye:    cut_base = 50000-2000 = 48000; doctor_cut = 19200;
        //                report = 20% * (48000-19200) = 20% * 28800 = 5760.
        //   without dye: cut_base = 50000; doctor_cut = 20000;
        //                report = 20% * (50000-20000) = 6000.
        // The two DIVERGE (the legacy invariant that they matched is gone).
        let ct = ct(false, Some(50_000));
        let op = operator();
        let with_dye = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        let without_dye = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(with_dye.report_amount_iqd, 5_760);
        assert_eq!(without_dye.report_amount_iqd, 6_000);
        assert_ne!(with_dye.report_amount_iqd, without_dye.report_amount_iqd);
    }

    #[test]
    fn report_base_uses_doctor_cut_not_operator_cut() {
        // Doctor mode with a per-check fixed cut: report base = price - doctor_cut
        // and ignores the operator cut entirely.
        // price=80000, doctor_cut=12000, base=68000, 20% = 13600.
        let ct = ct(false, Some(80_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 12_000);
        assert_eq!(snap.report_amount_iqd, 13_600);
    }

    #[test]
    fn report_name_snapshot_omitted_when_setting_is_empty() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let mut s = settings();
        s.reporting_doctor_name = "   ".into();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: s,
        })
        .unwrap();
        // pct is still recorded, but the name snapshot stays None.
        assert_eq!(snap.report_pct, Some(20));
        assert_eq!(snap.reporting_doctor_name, None);
    }

    #[test]
    fn report_pct_zero_yields_zero_amount_but_pct_some() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let mut s = settings();
        s.report_pct = 0;
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: s,
        })
        .unwrap();
        assert_eq!(snap.report_amount_iqd, 0);
        assert_eq!(snap.report_pct, Some(0));
    }

    #[test]
    fn rejects_report_pct_out_of_range_when_report_on() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let mut s = settings();
        s.report_pct = 150;
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: s,
        });
        match err {
            Err(AppError::Validation(m)) => assert!(m.contains("report_pct")),
            _ => panic!("expected Validation"),
        }
    }

    #[test]
    fn report_truncates_integer_division() {
        // price=50001, house doctor_cut = 50001*40/100 = 20000 (trunc),
        // base = 30001, 20% = 6000 (6000.2 truncated).
        let ct = ct(false, Some(50_001));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 20_000);
        assert_eq!(snap.report_amount_iqd, 6_000);
    }

    // ---- dalal (دلال) mode tests ----------------------------------------

    #[test]
    fn dalal_takes_flat_cut_and_no_internal_pct() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: true,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 10_000);
        assert_eq!(snap.internal_pct, None);
        assert_eq!(snap.total_amount_iqd, 50_000);
    }

    #[test]
    fn dalal_with_report_uses_flat_cut_as_report_base() {
        // dalal doctor_cut = 10000; report base = 50000 - 10000 = 40000; 20% = 8000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: true,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 10_000);
        assert_eq!(snap.report_amount_iqd, 8_000);
        assert_eq!(snap.report_pct, Some(20));
    }

    #[test]
    fn dalal_with_doctor_present_is_rejected() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor();
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: true,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        match err {
            Err(AppError::Validation(m)) => assert!(m.contains("dalal")),
            _ => panic!("expected Validation"),
        }
    }

    // ---- existing coverage matrix (threaded with dalal: false) ----------

    #[test]
    fn flat_pricing_check_with_no_subtype_no_doctor() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 50_000);
        assert_eq!(snap.dye_cost_iqd, 0);
        assert_eq!(snap.report_amount_iqd, 0);
        assert_eq!(snap.internal_pct, Some(40));
        assert_eq!(snap.operator_cut_iqd, op.base_cut_per_check_iqd);
        assert_eq!(snap.total_amount_iqd, 50_000);
    }

    #[test]
    fn subtype_price_overrides_check_when_has_subtypes() {
        let ct = ct(true, None);
        let op = operator();
        let s = sub(ct.id, 70_000);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: Some(&s),
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 70_000);
    }

    #[test]
    fn doctor_override_replaces_internal_pct_via_flat_cut() {
        let ct = ct(false, Some(80_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 12_000);
        assert_eq!(snap.internal_pct, None);
        assert_eq!(snap.total_amount_iqd, 80_000);
    }

    #[test]
    fn doctor_override_replaces_internal_pct_via_percentage_cut() {
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 25_000);
        assert_eq!(snap.internal_pct, None);
    }

    #[test]
    fn doctor_pricing_price_override_replaces_base() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 10, Some(200_000));
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 200_000);
        assert_eq!(snap.doctor_cut_iqd, 20_000);
    }

    #[test]
    fn doctor_without_pricing_row_keeps_internal_pct_none_and_zero_cut() {
        let ct = ct(false, Some(15_000));
        let op = operator();
        let doc = doctor();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 15_000);
        assert_eq!(snap.doctor_cut_iqd, 0);
        assert_eq!(snap.internal_pct, None);
        assert_eq!(snap.total_amount_iqd, 15_000);
    }

    #[test]
    fn doctor_default_pct_cut_applies_when_no_per_check_row() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor_with_default_cut("pct", 20);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 10_000); // 20% of 50000
        assert_eq!(snap.internal_pct, None);
    }

    #[test]
    fn doctor_default_fixed_cut_applies_when_no_per_check_row() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor_with_default_cut("fixed", 7_000);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 7_000);
        assert_eq!(snap.internal_pct, None);
    }

    #[test]
    fn per_check_pricing_overrides_doctor_default_cut() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor_with_default_cut("pct", 20);
        let row = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&row),
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 12_000); // per-check fixed cut, not the 20% default
    }

    #[test]
    fn house_doctor_keeps_internal_pct_set_when_doctor_id_is_none() {
        let ct = ct(false, Some(60_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "Pat",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.internal_pct, Some(40));
        assert_eq!(snap.doctor_cut_iqd, 24_000); // 40% of 60000
    }

    #[test]
    fn dye_cost_added_when_dye_true_and_supported_zero_otherwise() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let on = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(on.dye_cost_iqd, 2000);
        let off = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(off.dye_cost_iqd, 0);
    }

    #[test]
    fn dye_unsupported_rejects_with_validation_err() {
        let mut t = ct(false, Some(50_000));
        t.dye_price_iqd = None;
        let op = operator();
        let err = compute(&MoneyMathInputs {
            check_type: &t,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        match err {
            Err(AppError::Validation(m)) => assert!(m.contains("dye")),
            _ => panic!("expected Validation"),
        }
    }

    // ---- dye price resolution from the catalog (check type / subtype) ----

    #[test]
    fn dye_cost_comes_from_check_type_price_when_flat() {
        let ct = ct_dye(false, Some(50_000), Some(3_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.dye_cost_iqd, 3_000);
        assert_eq!(snap.total_amount_iqd, snap.price_iqd + 3_000);
    }

    #[test]
    fn dye_cost_comes_from_subtype_price_when_subtyped() {
        // Subtyped check type: the subtype's own dye price wins, not the
        // check type's (which here is deliberately different / absent).
        let ct = ct_dye(true, None, None);
        let s = sub_dye(ct.id, 70_000, Some(4_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: Some(&s),
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.dye_cost_iqd, 4_000);
    }

    #[test]
    fn dye_price_zero_is_free_dye_not_unavailable() {
        let ct = ct_dye(false, Some(50_000), Some(0));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.dye_cost_iqd, 0);
        assert_eq!(snap.total_amount_iqd, snap.price_iqd);
    }

    #[test]
    fn dye_on_without_resolvable_price_errors() {
        let ct = ct_dye(false, Some(50_000), None);
        let op = operator();
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: true,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn dye_off_ignores_price() {
        let ct = ct_dye(false, Some(50_000), Some(5_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.dye_cost_iqd, 0);
    }

    #[test]
    fn requires_subtype_when_check_has_subtypes_and_rejects_subtype_when_disallowed() {
        let ct_with = ct(true, None);
        let ct_without = ct(false, Some(40_000));
        let op = operator();
        // missing subtype
        let err = compute(&MoneyMathInputs {
            check_type: &ct_with,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err.is_err());
        // disallowed subtype
        let s = sub(ct_without.id, 70_000);
        let err2 = compute(&MoneyMathInputs {
            check_type: &ct_without,
            check_subtype: Some(&s),
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err2.is_err());
    }

    #[test]
    fn operator_cut_uses_operator_base_cut() {
        let ct = ct(false, Some(50_000));
        let mut op = operator();
        op.base_cut_per_check_iqd = 7_777;
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.operator_cut_iqd, 7_777);
    }

    #[test]
    fn percentage_rounds_consistently_no_float_drift_across_100_runs() {
        let ct = ct(false, Some(1_000_037));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let mut prev: Option<i64> = None;
        for _ in 0..100 {
            let snap = compute(&MoneyMathInputs {
                check_type: &ct,
                check_subtype: None,
                doctor: Some(&doc),
                doctor_pricing: Some(&pr),
                operator: &op,
                patient_name: "p",
                dye: false,
                report: false,
                dalal: false,
                discount: false,
                mandoub_cut_iqd: 0,
                mandoub_name: None,
                price_override_iqd: None,
                amount_paid_override_iqd: None,
                settings: settings(),
            })
            .unwrap();
            if let Some(p) = prev {
                assert_eq!(p, snap.doctor_cut_iqd);
            }
            prev = Some(snap.doctor_cut_iqd);
        }
        // 1_000_037 * 25 / 100 = 250_009 (integer truncation)
        assert_eq!(prev, Some(250_009));
    }

    #[test]
    fn rejects_doctor_percentage_out_of_range() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 150, None);
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err.is_err());
    }

    #[test]
    fn rejects_internal_pct_out_of_range_in_house_mode() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let bad = MoneySettings {
            report_pct: 0,
            reporting_doctor_name: String::new(),
            internal_doctor_pct: 250,
        };
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: bad,
        });
        assert!(err.is_err());
    }

    #[test]
    fn rejects_check_type_without_base_when_no_subtype() {
        let ct = ct(false, None);
        let op = operator();
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err.is_err());
    }

    #[test]
    fn snapshot_carries_all_name_fields_when_provided() {
        let ct = ct(true, None);
        let s = sub(ct.id, 90_000);
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 10, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: Some(&s),
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "John Doe",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.patient_name, "John Doe");
        assert_eq!(snap.doctor_name.as_deref(), Some("Dr Sara"));
        assert_eq!(snap.operator_name, op.name);
        assert_eq!(snap.check_type_name_ar, ct.name_ar);
        assert_eq!(snap.check_subtype_name_ar.as_deref(), Some("فرعي"));
    }

    #[test]
    fn mandoub_cut_and_name_pass_through_without_changing_other_cuts() {
        // With a doctor + per-check pricing, the doctor/operator cut is fixed;
        // adding a مندوب cut+name must copy them straight into the snapshot
        // WITHOUT perturbing any of the computed cuts or the patient total.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);

        let baseline = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();

        let with_mandoub = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 1_000,
            mandoub_name: Some("Rep"),
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();

        // Passthrough captured.
        assert_eq!(with_mandoub.mandoub_cut_iqd, 1_000);
        assert_eq!(with_mandoub.mandoub_name.as_deref(), Some("Rep"));
        // Baseline carries no مندوب.
        assert_eq!(baseline.mandoub_cut_iqd, 0);
        assert!(baseline.mandoub_name.is_none());
        // Nothing else moved: doctor cut, operator cut, and patient total are
        // identical with and without the مندوب cut.
        assert_eq!(with_mandoub.doctor_cut_iqd, baseline.doctor_cut_iqd);
        assert_eq!(with_mandoub.operator_cut_iqd, baseline.operator_cut_iqd);
        assert_eq!(with_mandoub.total_amount_iqd, baseline.total_amount_iqd);
        assert_eq!(with_mandoub.report_amount_iqd, baseline.report_amount_iqd);
    }

    // ---- discount tests --------------------------------------------------

    #[test]
    fn discount_zeroes_referring_doctor_cut_only() {
        // Doctor with a 25% per-check cut on a 100k price -> 25k cut normally.
        // With discount on, the doctor cut is 0; operator cut and total are
        // untouched.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);

        let baseline = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(baseline.doctor_cut_iqd, 25_000);

        let discounted = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: true,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(discounted.doctor_cut_iqd, 0);
        // Everything else identical: price, operator cut, total, internal_pct.
        assert_eq!(discounted.price_iqd, baseline.price_iqd);
        assert_eq!(discounted.operator_cut_iqd, baseline.operator_cut_iqd);
        assert_eq!(discounted.total_amount_iqd, baseline.total_amount_iqd);
        assert_eq!(discounted.internal_pct, baseline.internal_pct);
    }

    #[test]
    fn discount_widens_report_base_to_full_price() {
        // With discount the doctor cut is 0, so the report base = price - 0.
        // price=100k, report_pct=20 -> report = 20k (vs 15k with the 25% cut).
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            dalal: false,
            discount: true,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 0);
        assert_eq!(snap.report_amount_iqd, 20_000);
    }

    #[test]
    fn discount_without_doctor_is_rejected() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: true,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err.is_err());
    }

    #[test]
    fn discount_preserves_mandoub_passthrough() {
        // Discount zeroes the doctor cut but never the مندوب cut: both coexist.
        let ct = ct(false, Some(80_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 30, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: true,
            mandoub_cut_iqd: 1_000,
            mandoub_name: Some("Rep"),
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 0);
        assert_eq!(snap.mandoub_cut_iqd, 1_000);
        assert_eq!(snap.mandoub_name.as_deref(), Some("Rep"));
    }

    #[test]
    fn mandoub_cut_without_doctor_is_rejected() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let err = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 500,
            mandoub_name: Some("Rep"),
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        });
        assert!(err.is_err());
    }

    // ---- paid-basis cut tests (feature: cuts off paid amount) ------------

    fn inputs_house<'a>(
        ct: &'a CheckType,
        op: &'a Operator,
        price_override: Option<i64>,
        paid: Option<i64>,
        dye: bool,
        report: bool,
    ) -> MoneyMathInputs<'a> {
        MoneyMathInputs {
            check_type: ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: op,
            patient_name: "p",
            dye,
            report,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: price_override,
            amount_paid_override_iqd: paid,
            settings: settings(),
        }
    }

    #[test]
    fn house_underpaid_scales_internal_cut_off_collected() {
        // price 50k, no override on price, collected 30k, internal 40%.
        // cut_base = 30000; doctor_cut = 30000*40/100 = 12000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&inputs_house(&ct, &op, None, Some(30_000), false, false)).unwrap();
        assert_eq!(snap.price_iqd, 50_000);
        assert_eq!(snap.doctor_cut_iqd, 12_000);
        assert_eq!(snap.operator_cut_iqd, 5_000); // fixed, unchanged
        assert_eq!(snap.amount_paid_override_iqd, Some(30_000));
        assert_eq!(snap.total_amount_iqd, 50_000); // price + dye(0), unchanged
    }

    #[test]
    fn editable_price_override_replaces_catalog_price() {
        // catalog 50k but receptionist sets price 80k, paid in full.
        // cut_base = 80000; internal 40% = 32000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&inputs_house(&ct, &op, Some(80_000), None, false, false)).unwrap();
        assert_eq!(snap.price_iqd, 80_000);
        assert_eq!(snap.doctor_cut_iqd, 32_000);
        assert_eq!(snap.total_amount_iqd, 80_000);
    }

    #[test]
    fn external_doctor_pct_scales_off_collected() {
        // price 100k, doctor pct 25, collected 60k -> cut_base 60k -> 15000.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            amount_paid_override_iqd: Some(60_000),
            ..inputs_house(&ct, &op, None, None, false, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 15_000);
        assert_eq!(snap.internal_pct, None);
    }

    #[test]
    fn external_doctor_fixed_cut_does_not_scale() {
        // price 100k, doctor FIXED 12k, collected 60k. Fixed stays 12000.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            amount_paid_override_iqd: Some(60_000),
            ..inputs_house(&ct, &op, None, None, false, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 12_000);
    }

    #[test]
    fn report_base_uses_cut_base_after_doctor_cut() {
        // price 100k, doctor pct 25, collected 60k, report 20%.
        // cut_base 60k, doctor_cut 15k, report = 20% * (60000-15000) = 9000.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            amount_paid_override_iqd: Some(60_000),
            ..inputs_house(&ct, &op, None, None, false, true)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 15_000);
        assert_eq!(snap.report_amount_iqd, 9_000);
    }

    #[test]
    fn zero_cut_base_zeroes_every_cut_including_fixed() {
        // price 50k, dye 2000, collected 5000 -> cut_base = max(0, 5000-2000)=3000?
        // NO: choose collected below dye. collected 1500, dye 2000 -> base 0.
        // Everyone (operator fixed 5000, mandoub, doctor) zeroes.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            dye: true,
            amount_paid_override_iqd: Some(1_500),
            mandoub_cut_iqd: 1_000,
            mandoub_name: Some("Rep"),
            ..inputs_house(&ct, &op, None, None, true, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 0);
        assert_eq!(snap.operator_cut_iqd, 0);
        assert_eq!(snap.mandoub_cut_iqd, 0);
        assert_eq!(snap.report_amount_iqd, 0);
        // Patient total is still price + dye regardless of the zero cuts.
        assert_eq!(snap.total_amount_iqd, 52_000);
    }

    #[test]
    fn paid_full_default_matches_price_when_no_overrides() {
        // No price override, no paid override -> collected = price, cut_base = price.
        // Identical to the legacy house behaviour: internal 40% of 50000 = 20000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&inputs_house(&ct, &op, None, None, false, false)).unwrap();
        assert_eq!(snap.doctor_cut_iqd, 20_000);
        assert_eq!(snap.amount_paid_override_iqd, None);
    }

    #[test]
    fn dalal_flat_cut_survives_partial_payment_when_base_positive() {
        // dalal flat 10k; collected 40k (>0 after dye 0) -> base 40k, dalal stays 10k.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            dalal: true,
            amount_paid_override_iqd: Some(40_000),
            ..inputs_house(&ct, &op, None, None, false, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 10_000);
        assert!(snap.internal_pct.is_none());
    }
}
