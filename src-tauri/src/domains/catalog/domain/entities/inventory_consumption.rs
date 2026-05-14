//! `InventoryConsumptionMap` entity (PRD §6.1.13).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryConsumptionMap {
    pub id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub item_id: Uuid,
    pub quantity_per_check: i64,
    pub on_dye_only: bool,
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
pub struct ConsumptionMapNewInput {
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub item_id: Uuid,
    pub quantity_per_check: i64,
    pub on_dye_only: bool,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

impl InventoryConsumptionMap {
    pub fn try_new(input: ConsumptionMapNewInput) -> AppResult<Self> {
        if input.quantity_per_check <= 0 {
            return Err(AppError::Validation(
                "quantity_per_check must be > 0".into(),
            ));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            check_type_id: input.check_type_id,
            check_subtype_id: input.check_subtype_id,
            item_id: input.item_id,
            quantity_per_check: input.quantity_per_check,
            on_dye_only: input.on_dye_only,
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

    pub fn updated_with(mut self, quantity_per_check: i64, on_dye_only: bool) -> AppResult<Self> {
        if quantity_per_check <= 0 {
            return Err(AppError::Validation(
                "quantity_per_check must be > 0".into(),
            ));
        }
        self.quantity_per_check = quantity_per_check;
        self.on_dye_only = on_dye_only;
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

    fn input(qty: i64, on_dye_only: bool) -> ConsumptionMapNewInput {
        ConsumptionMapNewInput {
            check_type_id: Uuid::now_v7(),
            check_subtype_id: None,
            item_id: Uuid::now_v7(),
            quantity_per_check: qty,
            on_dye_only,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_rejects_zero_or_negative_quantity() {
        assert!(InventoryConsumptionMap::try_new(input(0, false)).is_err());
        assert!(InventoryConsumptionMap::try_new(input(-5, false)).is_err());
        assert!(InventoryConsumptionMap::try_new(input(1, false)).is_ok());
    }

    #[test]
    fn try_new_carries_on_dye_only_flag_and_optional_subtype() {
        let mut i = input(5, true);
        i.check_subtype_id = Some(Uuid::now_v7());
        let c = InventoryConsumptionMap::try_new(i).unwrap();
        assert!(c.on_dye_only);
        assert!(c.check_subtype_id.is_some());
    }

    #[test]
    fn try_new_seeds_sync_columns() {
        let c = InventoryConsumptionMap::try_new(input(5, false)).unwrap();
        assert_eq!(c.version, 1);
        assert!(c.dirty);
        assert!(c.deleted_at.is_none());
        assert_eq!(c.id.get_version_num(), 7);
    }

    #[test]
    fn updated_with_rejects_non_positive_quantity() {
        let c = InventoryConsumptionMap::try_new(input(5, false)).unwrap();
        assert!(c.clone().updated_with(0, false).is_err());
        assert!(c.updated_with(-1, false).is_err());
    }

    #[test]
    fn updated_with_bumps_version_and_dirty() {
        let c = InventoryConsumptionMap::try_new(input(5, false)).unwrap();
        let v0 = c.version;
        let after = c.updated_with(7, true).unwrap();
        assert_eq!(after.quantity_per_check, 7);
        assert!(after.on_dye_only);
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn soft_deleted_marks_tombstone() {
        let c = InventoryConsumptionMap::try_new(input(5, false)).unwrap();
        let v0 = c.version;
        let after = c.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }
}
