//! `CheckSubtype` entity (PRD §6.1.3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSubtype {
    pub id: Uuid,
    pub check_type_id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub price_iqd: i64,
    pub dye_price_iqd: Option<i64>,
    pub sort_order: i64,
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
pub struct CheckSubtypeNewInput {
    pub check_type_id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub price_iqd: i64,
    pub dye_price_iqd: Option<i64>,
    pub sort_order: i64,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckSubtypeUpdate {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub price_iqd: Option<i64>,
    pub dye_price_iqd: Option<Option<i64>>,
    pub sort_order: Option<i64>,
}

impl CheckSubtype {
    pub fn try_new(input: CheckSubtypeNewInput) -> AppResult<Self> {
        let name_ar = input.name_ar.trim().to_string();
        if name_ar.is_empty() {
            return Err(AppError::Validation("subtype name (ar) required".into()));
        }
        if input.price_iqd < 0 {
            return Err(AppError::Validation(
                "price_iqd must be non-negative".into(),
            ));
        }
        if let Some(n) = input.dye_price_iqd {
            if n < 0 {
                return Err(AppError::Validation("dye_price_iqd must be >= 0".into()));
            }
        }
        let name_en = input
            .name_en
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            check_type_id: input.check_type_id,
            name_ar,
            name_en,
            price_iqd: input.price_iqd,
            dye_price_iqd: input.dye_price_iqd,
            sort_order: input.sort_order,
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

    pub fn with_updated_fields(mut self, patch: CheckSubtypeUpdate) -> AppResult<Self> {
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
        if let Some(price) = patch.price_iqd {
            if price < 0 {
                return Err(AppError::Validation(
                    "price_iqd must be non-negative".into(),
                ));
            }
            self.price_iqd = price;
        }
        if let Some(dye) = patch.dye_price_iqd {
            if let Some(n) = dye {
                if n < 0 {
                    return Err(AppError::Validation("dye_price_iqd must be >= 0".into()));
                }
            }
            self.dye_price_iqd = dye;
        }
        if let Some(s) = patch.sort_order {
            self.sort_order = s;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, price: i64) -> CheckSubtypeNewInput {
        CheckSubtypeNewInput {
            check_type_id: Uuid::now_v7(),
            name_ar: name.into(),
            name_en: None,
            price_iqd: price,
            dye_price_iqd: None,
            sort_order: 0,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_requires_non_empty_name_ar() {
        assert!(CheckSubtype::try_new(input("", 1000)).is_err());
        assert!(CheckSubtype::try_new(input("  ", 1000)).is_err());
        assert!(CheckSubtype::try_new(input("X", 1000)).is_ok());
    }

    #[test]
    fn try_new_rejects_negative_price() {
        assert!(CheckSubtype::try_new(input("X", -1)).is_err());
        assert!(CheckSubtype::try_new(input("X", 0)).is_ok());
    }

    #[test]
    fn try_new_trims_names_and_drops_empty_name_en() {
        let s = CheckSubtype::try_new(CheckSubtypeNewInput {
            check_type_id: Uuid::now_v7(),
            name_ar: "  Brain  ".into(),
            name_en: Some("   ".into()),
            price_iqd: 50_000,
            dye_price_iqd: None,
            sort_order: 0,
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert_eq!(s.name_ar, "Brain");
        assert!(s.name_en.is_none());
    }

    #[test]
    fn try_new_seeds_sync_columns() {
        let s = CheckSubtype::try_new(input("X", 0)).unwrap();
        assert_eq!(s.version, 1);
        assert!(s.dirty);
        assert!(s.deleted_at.is_none());
        assert!(s.last_synced_at.is_none());
    }

    #[test]
    fn try_new_assigns_uuid_v7_id() {
        let s = CheckSubtype::try_new(input("X", 0)).unwrap();
        assert_eq!(s.id.get_version_num(), 7);
    }

    #[test]
    fn with_updated_fields_bumps_version_and_dirty() {
        let s = CheckSubtype::try_new(input("X", 1000)).unwrap();
        let v0 = s.version;
        let after = s
            .with_updated_fields(CheckSubtypeUpdate {
                price_iqd: Some(2000),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(after.price_iqd, 2000);
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn with_updated_fields_rejects_negative_price() {
        let s = CheckSubtype::try_new(input("X", 1000)).unwrap();
        let res = s.with_updated_fields(CheckSubtypeUpdate {
            price_iqd: Some(-1),
            ..Default::default()
        });
        assert!(res.is_err());
    }

    #[test]
    fn with_updated_fields_rejects_empty_name() {
        let s = CheckSubtype::try_new(input("X", 1000)).unwrap();
        let res = s.with_updated_fields(CheckSubtypeUpdate {
            name_ar: Some("  ".into()),
            ..Default::default()
        });
        assert!(res.is_err());
    }

    #[test]
    fn soft_deleted_marks_tombstone_and_bumps_version() {
        let s = CheckSubtype::try_new(input("X", 1000)).unwrap();
        let v0 = s.version;
        let after = s.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert!(after.dirty);
        assert_eq!(after.version, v0 + 1);
    }

    #[test]
    fn try_new_accepts_optional_dye_price() {
        let s = CheckSubtype::try_new({
            let mut i = input("X", 1000);
            i.dye_price_iqd = Some(4_000);
            i
        })
        .unwrap();
        assert_eq!(s.dye_price_iqd, Some(4_000));
        assert_eq!(
            CheckSubtype::try_new(input("Y", 1000))
                .unwrap()
                .dye_price_iqd,
            None
        );
    }

    #[test]
    fn try_new_rejects_negative_dye_price() {
        assert!(CheckSubtype::try_new({
            let mut i = input("X", 1000);
            i.dye_price_iqd = Some(-1);
            i
        })
        .is_err());
    }

    #[test]
    fn update_sets_and_clears_dye_price() {
        let s = CheckSubtype::try_new(input("X", 1000)).unwrap();
        let set = s
            .with_updated_fields(CheckSubtypeUpdate {
                dye_price_iqd: Some(Some(2_500)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(set.dye_price_iqd, Some(2_500));
        let cleared = set
            .with_updated_fields(CheckSubtypeUpdate {
                dye_price_iqd: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(cleared.dye_price_iqd, None);
    }
}
