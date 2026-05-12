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

fn cuts(
    price_iqd: i64,
    operator: &Operator,
    pricing: Option<&DoctorCheckPricing>,
    settings: &MoneySettings,
) -> AppResult<(i64, Option<i64>, i64)> {
    let operator_cut = operator.base_cut_per_check_iqd;

    match pricing {
        Some(p) => {
            let doctor_cut = match p.cut_kind.as_str() {
                "pct" => {
                    if p.cut_value < 0 || p.cut_value > 100 {
                        return Err(AppError::Validation(
                            "doctor cut percentage must be 0..=100".into(),
                        ));
                    }
                    price_iqd * p.cut_value / 100
                }
                "fixed" => p.cut_value.max(0),
                other => {
                    return Err(AppError::Validation(format!("unknown cut_kind: {other}")));
                }
            };
            Ok((doctor_cut, None, operator_cut))
        }
        None => {
            // House mode: doctor_cut snapshot is the absolute share earned by
            // the clinic-employed doctor, expressed via `internal_doctor_pct`.
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
}
