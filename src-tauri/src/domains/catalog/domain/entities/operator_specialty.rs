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
