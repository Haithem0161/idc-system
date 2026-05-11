//! Audit log entry. Append-only; mirrors `audit_log` schema.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::value_objects::AuditAction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub actor_user_id: Uuid,
    pub action: AuditAction,
    pub entity: String,
    pub entity_id: String,
    pub delta: serde_json::Value,
    pub ip: Option<String>,
    pub device_id: String,
    pub at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub origin_device_id: Option<String>,
    pub entity_id_tenant: String,
}

#[derive(Debug, Clone)]
pub struct AuditCreateInput {
    pub actor_user_id: Uuid,
    pub action: AuditAction,
    pub entity: String,
    pub entity_id: String,
    pub delta: serde_json::Value,
    pub ip: Option<String>,
    pub device_id: String,
    pub entity_id_tenant: String,
}

impl AuditEntry {
    /// Build a new audit row with a UUID v7 id and `now` timestamps.
    pub fn create(input: AuditCreateInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            actor_user_id: input.actor_user_id,
            action: input.action,
            entity: input.entity,
            entity_id: input.entity_id,
            delta: input.delta,
            ip: input.ip,
            device_id: input.device_id.clone(),
            at: now,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: true,
            last_synced_at: None,
            origin_device_id: Some(input.device_id),
            entity_id_tenant: input.entity_id_tenant,
        }
    }
}
