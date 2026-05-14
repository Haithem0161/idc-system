//! Audit log entry. Append-only; mirrors `audit_log` schema.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

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
    /// Phase-01 §7.7 validated constructor.
    ///
    /// Rejects:
    /// - `delta` that is not a JSON object (audit rows must carry structured
    ///   diffs, never bare scalars or `null`).
    /// - `action = Update` paired with an empty delta (a no-op update would
    ///   produce an audit row with nothing to record -- caller should skip
    ///   via `AuditWriter::skip_if_no_change` instead).
    pub fn try_new(input: AuditCreateInput) -> AppResult<Self> {
        if !input.delta.is_object() {
            return Err(AppError::Validation(
                "audit: delta must be a JSON object".into(),
            ));
        }
        if matches!(input.action, AuditAction::Update)
            && input.delta.as_object().map(|m| m.is_empty()).unwrap_or(true)
        {
            return Err(AppError::Validation(
                "audit: update delta must contain at least one changed field".into(),
            ));
        }
        Ok(Self::create(input))
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_input() -> AuditCreateInput {
        AuditCreateInput {
            actor_user_id: Uuid::now_v7(),
            action: AuditAction::Create,
            entity: "visits".into(),
            entity_id: "v-1".into(),
            delta: json!({ "status": { "from": null, "to": "draft" } }),
            ip: Some("127.0.0.1".into()),
            device_id: "device-abc".into(),
            entity_id_tenant: "tenant-1::v-1".into(),
        }
    }

    #[test]
    fn create_produces_audit_with_uuid_v7_id() {
        let audit = AuditEntry::create(sample_input());
        assert_eq!(audit.id.get_version_num(), 7);
    }

    #[test]
    fn create_stamps_at_created_updated_to_same_instant() {
        let audit = AuditEntry::create(sample_input());
        assert_eq!(audit.at, audit.created_at);
        assert_eq!(audit.created_at, audit.updated_at);
    }

    #[test]
    fn create_marks_row_dirty_and_unsynced() {
        // Phase-01 offline-first invariant: brand-new audit rows are dirty=1
        // and `last_synced_at` is None until the sync engine pushes them.
        let audit = AuditEntry::create(sample_input());
        assert!(audit.dirty);
        assert!(audit.last_synced_at.is_none());
        assert!(audit.deleted_at.is_none());
        assert_eq!(audit.version, 1);
    }

    #[test]
    fn create_records_origin_device_id_from_input() {
        let audit = AuditEntry::create(sample_input());
        assert_eq!(audit.origin_device_id.as_deref(), Some("device-abc"));
        assert_eq!(audit.device_id, "device-abc");
    }

    #[test]
    fn create_preserves_delta_payload_as_supplied() {
        let mut input = sample_input();
        input.delta = json!({ "field": { "from": 1, "to": 2 } });
        let audit = AuditEntry::create(input);
        assert_eq!(audit.delta["field"]["from"], json!(1));
        assert_eq!(audit.delta["field"]["to"], json!(2));
    }

    #[test]
    fn create_preserves_entity_id_tenant_for_index_lookup() {
        // Phase-01 §7.9: audit queries use `entity_id_tenant` for the
        // composite index; the field must round-trip through `create()` so
        // the index hits the row written by the audit writer.
        let audit = AuditEntry::create(sample_input());
        assert_eq!(audit.entity_id_tenant, "tenant-1::v-1");
    }

    #[test]
    fn create_two_audits_get_distinct_ids() {
        let a = AuditEntry::create(sample_input());
        let b = AuditEntry::create(sample_input());
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn audit_serializes_action_as_snake_case() {
        // Wire format invariant for /sync/push: action is the snake_case enum.
        let mut input = sample_input();
        input.action = AuditAction::SoftDelete;
        let audit = AuditEntry::create(input);
        let json = serde_json::to_value(&audit).unwrap();
        assert_eq!(json["action"], json!("soft_delete"));
    }

    // try_new validation -------------------------------------------------

    #[test]
    fn try_new_succeeds_on_valid_object_delta() {
        let audit = AuditEntry::try_new(sample_input()).expect("valid input constructs");
        assert_eq!(audit.entity, "visits");
    }

    #[test]
    fn try_new_rejects_null_delta() {
        let mut input = sample_input();
        input.delta = json!(null);
        let err = AuditEntry::try_new(input).expect_err("null delta rejected");
        assert!(err.to_string().contains("must be a JSON object"));
    }

    #[test]
    fn try_new_rejects_scalar_delta() {
        let mut input = sample_input();
        input.delta = json!("status changed");
        let err = AuditEntry::try_new(input).expect_err("string delta rejected");
        assert!(err.to_string().contains("must be a JSON object"));
    }

    #[test]
    fn try_new_rejects_update_with_empty_delta() {
        // An Update action with an empty delta is a no-op write; the caller
        // should use AuditWriter::skip_if_no_change instead of constructing
        // an audit row.
        let mut input = sample_input();
        input.action = AuditAction::Update;
        input.delta = json!({});
        let err = AuditEntry::try_new(input).expect_err("empty update delta rejected");
        assert!(err.to_string().contains("at least one changed field"));
    }

    #[test]
    fn try_new_accepts_create_with_empty_delta() {
        // Phase-01 §7.7: Create / SoftDelete may carry an empty delta when
        // the row's prior state is null (delta wildcard handled at the
        // writer layer). Only Update must reject empty.
        let mut input = sample_input();
        input.action = AuditAction::Create;
        input.delta = json!({});
        let audit = AuditEntry::try_new(input).expect("create with empty delta is valid");
        assert_eq!(audit.action, AuditAction::Create);
    }

    #[test]
    fn try_new_accepts_soft_delete_with_empty_delta() {
        let mut input = sample_input();
        input.action = AuditAction::SoftDelete;
        input.delta = json!({});
        let audit =
            AuditEntry::try_new(input).expect("soft_delete with empty delta is valid");
        assert_eq!(audit.action, AuditAction::SoftDelete);
    }
}
