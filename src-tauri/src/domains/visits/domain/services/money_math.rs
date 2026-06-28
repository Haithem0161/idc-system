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

#[derive(Debug, Clone, Copy)]
pub struct MoneySettings {
    pub dye_cost_iqd: i64,
    pub report_cost_iqd: i64,
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
    pub settings: MoneySettings,
}

/// Compute the snapshot block. Caller has already validated subtype /
/// dye / report consistency at the entity level; this function is strict
/// about money invariants only.
pub fn compute(inputs: &MoneyMathInputs<'_>) -> AppResult<VisitSnapshots> {
    if inputs.dye && !inputs.check_type.dye_supported {
        return Err(AppError::Validation(
            "check type does not support dye".into(),
        ));
    }
    if inputs.report && !inputs.check_type.report_supported {
        return Err(AppError::Validation(
            "check type does not support report".into(),
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

    let base_price = base_price(inputs)?;
    let price_iqd = effective_price(base_price, inputs.doctor_pricing);

    let dye_cost = if inputs.dye {
        inputs.settings.dye_cost_iqd
    } else {
        0
    };
    let report_cost = if inputs.report {
        inputs.settings.report_cost_iqd
    } else {
        0
    };

    let (doctor_cut, internal_pct, operator_cut) = cuts(
        price_iqd,
        inputs.operator,
        inputs.doctor,
        inputs.doctor_pricing,
        &inputs.settings,
    )?;

    let total = price_iqd + dye_cost + report_cost;

    Ok(VisitSnapshots {
        price_iqd,
        dye_cost_iqd: dye_cost,
        report_cost_iqd: report_cost,
        doctor_cut_iqd: doctor_cut,
        operator_cut_iqd: operator_cut,
        internal_pct,
        total_amount_iqd: total,
        // The money engine only ever produces the billed snapshot. A collected
        // override is a receptionist decision applied after compute(), so it is
        // never set here.
        amount_paid_override_iqd: None,
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

fn effective_price(base: i64, pricing: Option<&DoctorCheckPricing>) -> i64 {
    match pricing {
        Some(p) => p.price_override_iqd.unwrap_or(base),
        None => base,
    }
}

/// Resolve a doctor cut from a `(kind, value)` pair against the visit price.
/// Shared by the per-check `DoctorCheckPricing` override and the doctor-level
/// default cut so the pct/fixed math never drifts between the two.
fn cut_from_kind_value(price_iqd: i64, kind: &str, value: i64) -> AppResult<i64> {
    match kind {
        "pct" => {
            if !(0..=100).contains(&value) {
                return Err(AppError::Validation(
                    "doctor cut percentage must be 0..=100".into(),
                ));
            }
            Ok(price_iqd * value / 100)
        }
        "fixed" => Ok(value.max(0)),
        other => Err(AppError::Validation(format!("unknown cut_kind: {other}"))),
    }
}

fn cuts(
    price_iqd: i64,
    operator: &Operator,
    doctor: Option<&Doctor>,
    pricing: Option<&DoctorCheckPricing>,
    settings: &MoneySettings,
) -> AppResult<(i64, Option<i64>, i64)> {
    let operator_cut = operator.base_cut_per_check_iqd;

    match (doctor, pricing) {
        (_, Some(p)) => {
            // Per-check override wins over everything: explicit cut for this
            // exact doctor + check (+ subtype).
            let doctor_cut = cut_from_kind_value(price_iqd, p.cut_kind.as_str(), p.cut_value)?;
            Ok((doctor_cut, None, operator_cut))
        }
        (Some(d), None) => {
            // Referring doctor selected but no per-check DoctorCheckPricing row.
            // Fall back to the doctor's negotiated DEFAULT cut when configured;
            // otherwise the cut is zero (the historical behaviour). `internal_pct`
            // MUST stay None -- it is the house-mode marker and Visit::lock
            // rejects a doctor visit that carries one (invariant 6).
            let doctor_cut = match (d.default_cut_kind.as_deref(), d.default_cut_value) {
                (Some(kind), Some(value)) => cut_from_kind_value(price_iqd, kind, value)?,
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
            let doctor_cut = price_iqd * settings.internal_doctor_pct / 100;
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
        let now = Utc::now();
        CheckType {
            id: Uuid::now_v7(),
            name_ar: "اختبار".into(),
            name_en: Some("Test".into()),
            has_subtypes,
            base_price_iqd: base,
            dye_supported: true,
            report_supported: true,
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
            dye_cost_iqd: 2000,
            report_cost_iqd: 3000,
            internal_doctor_pct: 40,
        }
    }

    #[test]
    fn flat_house_with_dye() {
        let ct = ct(false, Some(50000));
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 50000);
        assert_eq!(snap.dye_cost_iqd, 2000);
        assert_eq!(snap.report_cost_iqd, 0);
        assert_eq!(snap.doctor_cut_iqd, 20000); // 40% of 50000
        assert_eq!(snap.internal_pct, Some(40));
        assert_eq!(snap.operator_cut_iqd, 5000);
        assert_eq!(snap.total_amount_iqd, 52000);
    }

    #[test]
    fn total_equals_sum_invariant() {
        let ct = ct(false, Some(75000));
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(
            snap.total_amount_iqd,
            snap.price_iqd + snap.dye_cost_iqd + snap.report_cost_iqd
        );
    }

    #[test]
    fn rejects_dye_when_unsupported() {
        let mut t = ct(false, Some(50000));
        t.dye_supported = false;
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
            settings: settings(),
        });
        assert!(err.is_err());
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
        let now = Utc::now();
        CheckSubtype {
            id: Uuid::now_v7(),
            check_type_id,
            name_ar: "فرعي".into(),
            name_en: Some("Sub".into()),
            price_iqd: price,
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 50_000);
        assert_eq!(snap.dye_cost_iqd, 0);
        assert_eq!(snap.report_cost_iqd, 0);
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.price_iqd, 200_000);
        assert_eq!(snap.doctor_cut_iqd, 20_000);
    }

    #[test]
    fn doctor_without_pricing_row_keeps_internal_pct_none_and_zero_cut() {
        // Referring doctor selected but no DoctorCheckPricing configured: price
        // falls back to base, doctor cut is zero, and internal_pct must be None
        // so Visit::lock (invariant 6) accepts the snapshot.
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
        // No DoctorCheckPricing row, but the doctor has a default 20% cut: the
        // engine falls back to it instead of zero. internal_pct stays None
        // (still doctor mode, not house mode).
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 7_000);
        assert_eq!(snap.internal_pct, None);
    }

    #[test]
    fn per_check_pricing_overrides_doctor_default_cut() {
        // A doctor with a default cut still defers to an explicit per-check
        // DoctorCheckPricing row when one exists.
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(off.dye_cost_iqd, 0);
    }

    #[test]
    fn dye_unsupported_rejects_with_validation_err() {
        let mut t = ct(false, Some(50_000));
        t.dye_supported = false;
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
            settings: settings(),
        });
        match err {
            Err(AppError::Validation(m)) => assert!(m.contains("dye")),
            _ => panic!("expected Validation"),
        }
    }

    #[test]
    fn report_cost_added_when_report_true_zero_otherwise_and_rejects_when_unsupported() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let on = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(on.report_cost_iqd, 3000);

        let mut t = ct.clone();
        t.report_supported = false;
        let err = compute(&MoneyMathInputs {
            check_type: &t,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: &op,
            patient_name: "p",
            dye: false,
            report: true,
            settings: settings(),
        });
        assert!(err.is_err());
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
            settings: settings(),
        });
        assert!(err.is_err());
    }

    #[test]
    fn rejects_internal_pct_out_of_range_in_house_mode() {
        let ct = ct(false, Some(50_000));
        let op = operator();
        let bad = MoneySettings {
            dye_cost_iqd: 0,
            report_cost_iqd: 0,
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
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.patient_name, "John Doe");
        assert_eq!(snap.doctor_name.as_deref(), Some("Dr Sara"));
        assert_eq!(snap.operator_name, op.name);
        assert_eq!(snap.check_type_name_ar, ct.name_ar);
        assert_eq!(snap.check_subtype_name_ar.as_deref(), Some("فرعي"));
    }
}
