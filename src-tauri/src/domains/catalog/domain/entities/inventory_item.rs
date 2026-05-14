//! `InventoryItem` entity (PRD §6.1.12).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub unit: String,
    pub quantity_on_hand: i64,
    pub low_stock_threshold: i64,
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
pub struct InventoryItemNewInput {
    pub name_ar: String,
    pub name_en: Option<String>,
    pub unit: String,
    pub low_stock_threshold: i64,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InventoryItemUpdate {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub unit: Option<String>,
    pub low_stock_threshold: Option<i64>,
    pub is_active: Option<bool>,
}

fn clean_optional(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

impl InventoryItem {
    pub fn try_new(input: InventoryItemNewInput) -> AppResult<Self> {
        let name_ar = input.name_ar.trim().to_string();
        if name_ar.is_empty() {
            return Err(AppError::Validation("item name (ar) required".into()));
        }
        let unit = input.unit.trim().to_string();
        if unit.is_empty() {
            return Err(AppError::Validation("unit required".into()));
        }
        if input.low_stock_threshold < 0 {
            return Err(AppError::Validation(
                "low_stock_threshold must be non-negative".into(),
            ));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name_ar,
            name_en: clean_optional(input.name_en),
            unit,
            quantity_on_hand: 0,
            low_stock_threshold: input.low_stock_threshold,
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

    pub fn with_updated_fields(mut self, patch: InventoryItemUpdate) -> AppResult<Self> {
        if let Some(name_ar) = patch.name_ar {
            let n = name_ar.trim().to_string();
            if n.is_empty() {
                return Err(AppError::Validation("name_ar required".into()));
            }
            self.name_ar = n;
        }
        if let Some(name_en) = patch.name_en {
            self.name_en = clean_optional(name_en);
        }
        if let Some(unit) = patch.unit {
            let u = unit.trim().to_string();
            if u.is_empty() {
                return Err(AppError::Validation("unit required".into()));
            }
            self.unit = u;
        }
        if let Some(t) = patch.low_stock_threshold {
            if t < 0 {
                return Err(AppError::Validation(
                    "low_stock_threshold must be non-negative".into(),
                ));
            }
            self.low_stock_threshold = t;
        }
        if let Some(a) = patch.is_active {
            self.is_active = a;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, unit: &str, threshold: i64) -> InventoryItemNewInput {
        InventoryItemNewInput {
            name_ar: name.into(),
            name_en: None,
            unit: unit.into(),
            low_stock_threshold: threshold,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_requires_name_ar() {
        assert!(InventoryItem::try_new(input("", "ml", 0)).is_err());
        assert!(InventoryItem::try_new(input("   ", "ml", 0)).is_err());
    }

    #[test]
    fn try_new_requires_non_empty_unit_after_trim() {
        assert!(InventoryItem::try_new(input("Gel", "", 0)).is_err());
        assert!(InventoryItem::try_new(input("Gel", "   ", 0)).is_err());
        assert!(InventoryItem::try_new(input("Gel", "\t\t", 0)).is_err());
        assert!(InventoryItem::try_new(input("Gel", "\n", 0)).is_err());
    }

    #[test]
    fn try_new_trims_unit_before_persist() {
        let i = InventoryItem::try_new(input("Gel", "  ml  ", 0)).unwrap();
        assert_eq!(i.unit, "ml");
    }

    #[test]
    fn try_new_rejects_negative_low_stock_threshold() {
        assert!(InventoryItem::try_new(input("Gel", "ml", -1)).is_err());
        assert!(InventoryItem::try_new(input("Gel", "ml", 0)).is_ok());
    }

    #[test]
    fn try_new_seeds_quantity_on_hand_to_zero() {
        let i = InventoryItem::try_new(input("Gel", "ml", 100)).unwrap();
        assert_eq!(i.quantity_on_hand, 0);
    }

    #[test]
    fn try_new_seeds_sync_columns_and_active() {
        let i = InventoryItem::try_new(input("Gel", "ml", 0)).unwrap();
        assert_eq!(i.version, 1);
        assert!(i.dirty);
        assert!(i.is_active);
        assert!(i.deleted_at.is_none());
        assert_eq!(i.id.get_version_num(), 7);
    }

    #[test]
    fn with_updated_fields_revalidates_unit_and_threshold() {
        let i = InventoryItem::try_new(input("Gel", "ml", 100)).unwrap();
        let res = i.clone().with_updated_fields(InventoryItemUpdate {
            unit: Some("   ".into()),
            ..Default::default()
        });
        assert!(res.is_err());
        let res = i.with_updated_fields(InventoryItemUpdate {
            low_stock_threshold: Some(-1),
            ..Default::default()
        });
        assert!(res.is_err());
    }

    #[test]
    fn with_updated_fields_bumps_version_and_dirty() {
        let i = InventoryItem::try_new(input("Gel", "ml", 0)).unwrap();
        let v0 = i.version;
        let after = i
            .with_updated_fields(InventoryItemUpdate {
                name_ar: Some("Gel-2".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(after.name_ar, "Gel-2");
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn soft_deleted_marks_inactive_and_tombstone() {
        let i = InventoryItem::try_new(input("Gel", "ml", 0)).unwrap();
        let after = i.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert!(!after.is_active);
        assert!(after.dirty);
    }
}
