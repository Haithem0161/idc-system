//! JSON payload pushed to the sync server. Mirrors the
//! `OperatorShiftSyncRecord` shape on the server side.

use serde::Serialize;

use crate::domains::shifts::domain::entities::OperatorShift;

#[derive(Serialize)]
pub struct OperatorShiftPushPayload {
    pub id: String,
    pub operator_id: String,
    pub check_in_at: String,
    pub check_out_at: Option<String>,
    pub check_in_by_user_id: String,
    pub check_out_by_user_id: Option<String>,
    pub note: Option<String>,
    pub entity_id: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&OperatorShift> for OperatorShiftPushPayload {
    fn from(s: &OperatorShift) -> Self {
        Self {
            id: s.id.to_string(),
            operator_id: s.operator_id.to_string(),
            check_in_at: s.check_in_at.to_rfc3339(),
            check_out_at: s.check_out_at.map(|d| d.to_rfc3339()),
            check_in_by_user_id: s.check_in_by_user_id.to_string(),
            check_out_by_user_id: s.check_out_by_user_id.map(|u| u.to_string()),
            note: s.note.clone(),
            entity_id: s.entity_id.clone(),
            version: s.version,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            deleted_at: s.deleted_at.map(|d| d.to_rfc3339()),
            origin_device_id: s.origin_device_id.clone(),
        }
    }
}
