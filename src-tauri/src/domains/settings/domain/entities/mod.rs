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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_local_constructs_with_dirty_and_version_one() {
        let s = Setting::new_local(
            "dye_cost_iqd",
            SettingValue::Int(10_000),
            "tenant-1",
            Some("dev-A".into()),
        )
        .unwrap();
        assert_eq!(s.key, "dye_cost_iqd");
        assert_eq!(s.value, SettingValue::Int(10_000));
        assert_eq!(s.entity_id, "tenant-1");
        assert_eq!(s.version, 1);
        assert!(s.dirty);
        assert!(s.deleted_at.is_none());
        assert_eq!(s.origin_device_id.as_deref(), Some("dev-A"));
    }

    #[test]
    fn new_local_trims_key_and_rejects_empty() {
        let s =
            Setting::new_local("  arabic_numerals  ", SettingValue::Bool(true), "t", None).unwrap();
        assert_eq!(s.key, "arabic_numerals");

        let err = Setting::new_local("   ", SettingValue::Bool(true), "t", None).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn updated_with_bumps_version_and_marks_dirty() {
        let s = Setting::new_local(
            "currency_symbol",
            SettingValue::Text("د.ع".into()),
            "t",
            None,
        )
        .unwrap();
        let v0 = s.version;
        let t0 = s.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let s2 = s.updated_with(SettingValue::Text("IQD".into()));
        assert_eq!(s2.value, SettingValue::Text("IQD".into()));
        assert_eq!(s2.version, v0 + 1);
        assert!(s2.updated_at > t0);
        assert!(s2.dirty);
    }
}
