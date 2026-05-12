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
