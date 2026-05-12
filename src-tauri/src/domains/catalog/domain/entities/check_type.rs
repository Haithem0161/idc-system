//! `CheckType` entity (PRD §6.1.2).
//!
//! Invariants enforced in the constructor:
//! - `name_ar` is non-empty after trim.
//! - `has_subtypes` XOR `base_price_iqd`: exactly one of these is "set".
//! - `base_price_iqd` is non-negative when present.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckType {
    pub id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub has_subtypes: bool,
    pub base_price_iqd: Option<i64>,
    pub dye_supported: bool,
    pub report_supported: bool,
    pub sort_order: i64,
    pub is_active: bool,
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
pub struct CheckTypeNewInput {
    pub name_ar: String,
    pub name_en: Option<String>,
    pub has_subtypes: bool,
    pub base_price_iqd: Option<i64>,
    pub dye_supported: bool,
    pub report_supported: bool,
    pub sort_order: i64,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckTypeUpdate {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub base_price_iqd: Option<Option<i64>>,
    pub dye_supported: Option<bool>,
    pub report_supported: Option<bool>,
    pub sort_order: Option<i64>,
    pub is_active: Option<bool>,
}

impl CheckType {
    pub fn try_new(input: CheckTypeNewInput) -> AppResult<Self> {
        let name_ar = input.name_ar.trim().to_string();
        if name_ar.is_empty() {
            return Err(AppError::Validation("check type name (ar) required".into()));
        }
        let name_en = input
            .name_en
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        validate_xor(input.has_subtypes, input.base_price_iqd)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name_ar,
            name_en,
            has_subtypes: input.has_subtypes,
            base_price_iqd: input.base_price_iqd,
            dye_supported: input.dye_supported,
            report_supported: input.report_supported,
            sort_order: input.sort_order,
            is_active: true,
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

    pub fn with_updated_fields(mut self, patch: CheckTypeUpdate) -> AppResult<Self> {
        if let Some(name_ar) = patch.name_ar {
            let n = name_ar.trim().to_string();
            if n.is_empty() {
                return Err(AppError::Validation("name_ar required".into()));
            }
            self.name_ar = n;
        }
        if let Some(name_en) = patch.name_en {
            self.name_en = name_en
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if let Some(base) = patch.base_price_iqd {
            validate_xor(self.has_subtypes, base)?;
            self.base_price_iqd = base;
        }
        if let Some(d) = patch.dye_supported {
            self.dye_supported = d;
        }
        if let Some(r) = patch.report_supported {
            self.report_supported = r;
        }
        if let Some(s) = patch.sort_order {
            self.sort_order = s;
        }
        if let Some(a) = patch.is_active {
            self.is_active = a;
        }
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    /// Apply the §7.1 toggle: 0 -> 1 zeroes `base_price_iqd`; 1 -> 0 sets it
    /// from the provided value. Cross-row invariants (no live subtypes when
    /// flipping 1 -> 0) are enforced in the service layer.
    pub fn toggled_has_subtypes(
        mut self,
        to_value: bool,
        new_base_price: Option<i64>,
    ) -> AppResult<Self> {
        if to_value {
            self.has_subtypes = true;
            self.base_price_iqd = None;
        } else {
            let price = new_base_price.ok_or_else(|| {
                AppError::Validation(
                    "base_price_iqd required when toggling has_subtypes to false".into(),
                )
            })?;
            if price < 0 {
                return Err(AppError::Validation(
                    "base_price_iqd must be non-negative".into(),
                ));
            }
            self.has_subtypes = false;
            self.base_price_iqd = Some(price);
        }
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn soft_deleted(mut self) -> Self {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.is_active = false;
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        self
    }
}

fn validate_xor(has_subtypes: bool, base: Option<i64>) -> AppResult<()> {
    match (has_subtypes, base) {
        (true, None) => Ok(()),
        (false, Some(n)) if n >= 0 => Ok(()),
        (true, Some(_)) => Err(AppError::Validation(
            "check type with subtypes must not have a flat base_price_iqd".into(),
        )),
        (false, None) => Err(AppError::Validation(
            "check type without subtypes requires base_price_iqd".into(),
        )),
        (false, Some(_)) => Err(AppError::Validation(
            "base_price_iqd must be non-negative".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, has_subtypes: bool, base: Option<i64>) -> CheckTypeNewInput {
        CheckTypeNewInput {
            name_ar: name.into(),
            name_en: None,
            has_subtypes,
            base_price_iqd: base,
            dye_supported: false,
            report_supported: false,
            sort_order: 0,
            entity_id: "unscoped".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn flat_requires_base_price() {
        assert!(CheckType::try_new(input("X", false, None)).is_err());
        assert!(CheckType::try_new(input("X", false, Some(0))).is_ok());
        assert!(CheckType::try_new(input("X", false, Some(-1))).is_err());
    }

    #[test]
    fn subtyped_forbids_base_price() {
        assert!(CheckType::try_new(input("X", true, Some(0))).is_err());
        assert!(CheckType::try_new(input("X", true, None)).is_ok());
    }

    #[test]
    fn empty_name_rejected() {
        assert!(CheckType::try_new(input("   ", false, Some(1000))).is_err());
    }

    #[test]
    fn toggle_subtypes_to_true_clears_price() {
        let ct = CheckType::try_new(input("X", false, Some(5000))).unwrap();
        let after = ct.toggled_has_subtypes(true, None).unwrap();
        assert!(after.has_subtypes);
        assert!(after.base_price_iqd.is_none());
    }

    #[test]
    fn toggle_subtypes_to_false_requires_price() {
        let ct = CheckType::try_new(input("X", true, None)).unwrap();
        assert!(ct.clone().toggled_has_subtypes(false, None).is_err());
        let after = ct.toggled_has_subtypes(false, Some(2000)).unwrap();
        assert!(!after.has_subtypes);
        assert_eq!(after.base_price_iqd, Some(2000));
    }
}
