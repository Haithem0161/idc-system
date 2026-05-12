//! Settings entity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domains::settings::domain::value_objects::SettingValue;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub id: Uuid,
    pub key: String,
    pub value: SettingValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

impl Setting {
    pub fn new_local(
        key: &str,
        value: SettingValue,
        entity_id: &str,
        origin_device_id: Option<String>,
    ) -> AppResult<Self> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::Validation("setting key required".into()));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            key: key.to_string(),
            value,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: true,
            last_synced_at: None,
            origin_device_id,
            entity_id: entity_id.to_string(),
        })
    }

    pub fn updated_with(mut self, value: SettingValue) -> Self {
        self.value = value;
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        self
    }
}
