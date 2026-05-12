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
    pub sort_order: i64,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckSubtypeUpdate {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub price_iqd: Option<i64>,
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
