//! `OperatorSpecialty` entity (PRD §6.1.7).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorSpecialty {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub check_type_id: Uuid,
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
pub struct OperatorSpecialtyNewInput {
    pub operator_id: Uuid,
    pub check_type_id: Uuid,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

impl OperatorSpecialty {
    pub fn try_new(input: OperatorSpecialtyNewInput) -> AppResult<Self> {
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            operator_id: input.operator_id,
            check_type_id: input.check_type_id,
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

    #[test]
    fn try_new_seeds_sync_columns_and_uuid_v7() {
        let s = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
            operator_id: Uuid::now_v7(),
            check_type_id: Uuid::now_v7(),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert_eq!(s.version, 1);
        assert!(s.dirty);
        assert!(s.deleted_at.is_none());
        assert_eq!(s.id.get_version_num(), 7);
    }

    #[test]
    fn soft_deleted_marks_tombstone_and_bumps_version() {
        let s = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
            operator_id: Uuid::now_v7(),
            check_type_id: Uuid::now_v7(),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        let v0 = s.version;
        let after = s.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }
}
