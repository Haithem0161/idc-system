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
