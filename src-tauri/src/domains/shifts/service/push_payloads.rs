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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::shifts::domain::entities::operator_shift::OperatorShiftOpenInput;
    use uuid::Uuid;

    fn sample() -> OperatorShift {
        OperatorShift::open(OperatorShiftOpenInput {
            operator_id: Uuid::now_v7(),
            by_user_id: Uuid::now_v7(),
            note: Some("am".into()),
            entity_id: "tenant-x".into(),
            origin_device_id: Some("dev-1".into()),
        })
        .unwrap()
    }

    #[test]
    fn payload_carries_string_ids_and_rfc3339_timestamps() {
        let s = sample();
        let p = OperatorShiftPushPayload::from(&s);
        assert_eq!(p.id, s.id.to_string());
        assert_eq!(p.operator_id, s.operator_id.to_string());
        assert_eq!(p.entity_id, s.entity_id);
        assert!(p.check_in_at.contains('T'));
        assert!(p.check_out_at.is_none());
        assert_eq!(p.origin_device_id.as_deref(), Some("dev-1"));
        assert_eq!(p.version, s.version);
        assert!(p.deleted_at.is_none());
    }

    #[test]
    fn payload_clock_out_round_trips_through_messagepack() {
        let by = Uuid::now_v7();
        let closed = sample()
            .close(by, chrono::Utc::now() + chrono::Duration::minutes(5))
            .unwrap();
        let payload = OperatorShiftPushPayload::from(&closed);
        let bytes = rmp_serde::encode::to_vec_named(&payload).unwrap();
        let back: serde_json::Value = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(
            back.get("id").and_then(|v| v.as_str()),
            Some(closed.id.to_string()).as_deref()
        );
        assert!(back.get("check_out_at").and_then(|v| v.as_str()).is_some());
        assert_eq!(
            back.get("version").and_then(|v| v.as_i64()),
            Some(closed.version)
        );
    }

    #[test]
    fn payload_soft_delete_carries_deleted_at_string_not_tombstone_flag() {
        let deleted = sample().soft_deleted();
        let payload = OperatorShiftPushPayload::from(&deleted);
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("deleted_at").and_then(|v| v.as_str()).is_some());
        // Additive-only contract: no `tombstone` field on the wire.
        assert!(json.get("tombstone").is_none());
    }

    #[test]
    fn payload_serde_field_set_is_stable() {
        let s = sample();
        let payload = OperatorShiftPushPayload::from(&s);
        let json = serde_json::to_value(&payload).unwrap();
        let obj = json.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "check_in_at",
                "check_in_by_user_id",
                "check_out_at",
                "check_out_by_user_id",
                "created_at",
                "deleted_at",
                "entity_id",
                "id",
                "note",
                "operator_id",
                "origin_device_id",
                "updated_at",
                "version",
            ]
        );
    }
}
