//! `DoctorCheckPricing` entity (PRD §6.1.5).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domains::catalog::domain::value_objects::CutKind;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheckPricing {
    pub id: Uuid,
    pub doctor_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub price_override_iqd: Option<i64>,
    pub cut_kind: CutKind,
    pub cut_value: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

#[derive(Debug, Clone)]
pub struct DoctorPricingNewInput {
    pub doctor_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub price_override_iqd: Option<i64>,
    pub cut_kind: CutKind,
    pub cut_value: i64,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

impl DoctorCheckPricing {
    pub fn try_new(input: DoctorPricingNewInput) -> AppResult<Self> {
        validate_cut(input.cut_kind, input.cut_value)?;
        if let Some(p) = input.price_override_iqd {
            if p < 0 {
                return Err(AppError::Validation(
                    "price_override_iqd must be non-negative".into(),
                ));
            }
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            doctor_id: input.doctor_id,
            check_type_id: input.check_type_id,
            check_subtype_id: input.check_subtype_id,
            price_override_iqd: input.price_override_iqd,
            cut_kind: input.cut_kind,
            cut_value: input.cut_value,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: true,
            last_synced_at: None,
            origin_device_id: input.origin_device_id,
            entity_id: input.entity_id,
        })
    }

    pub fn updated_with(
        mut self,
        price_override_iqd: Option<i64>,
        cut_kind: CutKind,
        cut_value: i64,
    ) -> AppResult<Self> {
        validate_cut(cut_kind, cut_value)?;
        if let Some(p) = price_override_iqd {
            if p < 0 {
                return Err(AppError::Validation(
                    "price_override_iqd must be non-negative".into(),
                ));
            }
        }
        self.price_override_iqd = price_override_iqd;
        self.cut_kind = cut_kind;
        self.cut_value = cut_value;
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn soft_deleted(mut self) -> Self {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        self
    }
}

pub fn validate_cut(cut_kind: CutKind, cut_value: i64) -> AppResult<()> {
    if cut_value < 0 {
        return Err(AppError::Validation(
            "cut_value must be non-negative".into(),
        ));
    }
    if cut_kind == CutKind::Pct && cut_value > 100 {
        return Err(AppError::Validation(
            "cut_value must be <= 100 for pct cut_kind".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_bounded_0_to_100() {
        assert!(validate_cut(CutKind::Pct, 0).is_ok());
        assert!(validate_cut(CutKind::Pct, 100).is_ok());
        assert!(validate_cut(CutKind::Pct, 101).is_err());
        assert!(validate_cut(CutKind::Pct, -1).is_err());
    }

    #[test]
    fn fixed_must_be_non_negative() {
        assert!(validate_cut(CutKind::Fixed, 0).is_ok());
        assert!(validate_cut(CutKind::Fixed, 1_000_000).is_ok());
        assert!(validate_cut(CutKind::Fixed, -1).is_err());
    }

    fn input(cut_kind: CutKind, cut_value: i64) -> DoctorPricingNewInput {
        DoctorPricingNewInput {
            doctor_id: Uuid::now_v7(),
            check_type_id: Uuid::now_v7(),
            check_subtype_id: None,
            price_override_iqd: None,
            cut_kind,
            cut_value,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_pct_in_range_succeeds() {
        let p = DoctorCheckPricing::try_new(input(CutKind::Pct, 25)).unwrap();
        assert_eq!(p.cut_value, 25);
        assert_eq!(p.cut_kind, CutKind::Pct);
    }

    #[test]
    fn try_new_pct_above_100_rejected() {
        assert!(DoctorCheckPricing::try_new(input(CutKind::Pct, 101)).is_err());
        assert!(DoctorCheckPricing::try_new(input(CutKind::Pct, 1000)).is_err());
    }

    #[test]
    fn try_new_fixed_negative_rejected() {
        assert!(DoctorCheckPricing::try_new(input(CutKind::Fixed, -1)).is_err());
    }

    #[test]
    fn try_new_rejects_negative_price_override() {
        let mut i = input(CutKind::Pct, 25);
        i.price_override_iqd = Some(-1);
        assert!(DoctorCheckPricing::try_new(i).is_err());
    }

    #[test]
    fn try_new_accepts_zero_price_override() {
        let mut i = input(CutKind::Pct, 25);
        i.price_override_iqd = Some(0);
        assert!(DoctorCheckPricing::try_new(i).is_ok());
    }

    #[test]
    fn try_new_seeds_sync_columns() {
        let p = DoctorCheckPricing::try_new(input(CutKind::Pct, 25)).unwrap();
        assert_eq!(p.version, 1);
        assert!(p.dirty);
        assert!(p.deleted_at.is_none());
        assert_eq!(p.id.get_version_num(), 7);
    }

    #[test]
    fn updated_with_changes_cut_and_bumps_version() {
        let p = DoctorCheckPricing::try_new(input(CutKind::Pct, 25)).unwrap();
        let v0 = p.version;
        let after = p.updated_with(None, CutKind::Fixed, 5_000).unwrap();
        assert_eq!(after.cut_kind, CutKind::Fixed);
        assert_eq!(after.cut_value, 5_000);
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn updated_with_revalidates_pct_range() {
        let p = DoctorCheckPricing::try_new(input(CutKind::Pct, 25)).unwrap();
        assert!(p.updated_with(None, CutKind::Pct, 200).is_err());
    }

    #[test]
    fn updated_with_revalidates_negative_override() {
        let p = DoctorCheckPricing::try_new(input(CutKind::Pct, 25)).unwrap();
        assert!(p.updated_with(Some(-1), CutKind::Pct, 25).is_err());
    }

    #[test]
    fn soft_deleted_marks_tombstone_and_bumps_version() {
        let p = DoctorCheckPricing::try_new(input(CutKind::Pct, 25)).unwrap();
        let v0 = p.version;
        let after = p.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn cut_kind_round_trips_as_lowercase_string() {
        assert_eq!(CutKind::Pct.as_str(), "pct");
        assert_eq!(CutKind::Fixed.as_str(), "fixed");
        assert_eq!(CutKind::parse("pct"), Some(CutKind::Pct));
        assert_eq!(CutKind::parse("fixed"), Some(CutKind::Fixed));
        assert_eq!(CutKind::parse("Pct"), None);
        assert_eq!(CutKind::parse("percent"), None);
    }
}
