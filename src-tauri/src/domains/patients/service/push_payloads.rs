//! Sync push wire format for `patients`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domains::patients::domain::entities::Patient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientPushPayload {
    pub id: Uuid,
    pub name: String,
    // Optional demographics (added with the patient archive). The sync server's
    // Patient model + push/pull schema must carry these same nullable columns
    // or they are dropped on the round-trip -- see the sync-server follow-up.
    pub phone: Option<String>,
    pub sex: Option<String>,
    pub birth_date: Option<String>,
    pub file_no: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

impl From<&Patient> for PatientPushPayload {
    fn from(p: &Patient) -> Self {
        Self {
            id: p.id,
            name: p.name.clone(),
            phone: p.phone.clone(),
            sex: p.sex.clone(),
            birth_date: p.birth_date.clone(),
            file_no: p.file_no.clone(),
            notes: p.notes.clone(),
            created_at: p.created_at,
            updated_at: p.updated_at,
            deleted_at: p.deleted_at,
            version: p.version,
            origin_device_id: p.origin_device_id.clone(),
            entity_id: p.entity_id.clone(),
        }
    }
}
