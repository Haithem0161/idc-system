//! Outbox queue entry. One row = one pending push for a single domain row.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::value_objects::OutboxAction;
use crate::error::{AppError, AppResult};

/// Phase-01 §7.15: outbox payload upper bound. Mirrors the server-side
/// `SyncPushBodySchema` maxItems / item-size envelope cap. A row larger
/// than this is rejected at construction time so it never reaches the
/// network layer.
pub const PAYLOAD_MAX_BYTES: usize = 8 * 1024 * 1024;

/// A row in the local `outbox` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxOp {
    pub op_id: Uuid,
    pub entity: String,
    pub entity_id: String,
    pub op: OutboxAction,
    pub payload: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub parked: bool,
}

impl OutboxOp {
    /// Build a new outbox row whose op-id is a fresh UUID v7.
    ///
    /// Convenience constructor for trusted callsites (existing repository
    /// reconstitution paths and tests). Prefer `try_new` for callers that
    /// originate from untrusted input or that need to surface validation
    /// failures.
    pub fn new(entity: impl Into<String>, entity_id: impl Into<String>, payload: Vec<u8>) -> Self {
        let now = Utc::now();
        Self {
            op_id: Uuid::now_v7(),
            entity: entity.into(),
            entity_id: entity_id.into(),
            op: OutboxAction::Upsert,
            payload,
            created_at: now,
            attempts: 0,
            next_attempt_at: now,
            last_error: None,
            parked: false,
        }
    }

    /// Phase-01 §7.15 validated constructor. Rejects:
    /// - empty `entity`
    /// - empty `entity_id`
    /// - payloads larger than `PAYLOAD_MAX_BYTES` (8 MiB)
    ///
    /// The `op` field is forced to `Upsert`; phase-01 wire format does not
    /// carry `delete` ops.
    pub fn try_new(
        entity: impl Into<String>,
        entity_id: impl Into<String>,
        payload: Vec<u8>,
    ) -> AppResult<Self> {
        let entity = entity.into();
        let entity_id = entity_id.into();
        if entity.is_empty() {
            return Err(AppError::Validation("outbox: entity is empty".into()));
        }
        if entity_id.is_empty() {
            return Err(AppError::Validation("outbox: entity_id is empty".into()));
        }
        if payload.len() > PAYLOAD_MAX_BYTES {
            return Err(AppError::Validation(format!(
                "outbox: payload too large ({} bytes > {} bytes)",
                payload.len(),
                PAYLOAD_MAX_BYTES
            )));
        }
        Ok(Self::new(entity, entity_id, payload))
    }

    /// Reconstitute from a stored row (no validation -- the DB is trusted).
    #[allow(clippy::too_many_arguments)]
    pub fn reconstitute(
        op_id: Uuid,
        entity: String,
        entity_id: String,
        op: OutboxAction,
        payload: Vec<u8>,
        created_at: DateTime<Utc>,
        attempts: i32,
        next_attempt_at: DateTime<Utc>,
        last_error: Option<String>,
        parked: bool,
    ) -> Self {
        Self {
            op_id,
            entity,
            entity_id,
            op,
            payload,
            created_at,
            attempts,
            next_attempt_at,
            last_error,
            parked,
        }
    }

    /// Exponential backoff: `2^attempts` minutes, capped at 60 min.
    pub fn next_backoff(attempts: i32) -> Duration {
        let minutes = 1u64 << attempts.clamp(0, 6).unsigned_abs();
        Duration::from_secs(minutes.min(60) * 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_produces_op_with_uuid_v7_id_and_defaults() {
        let op = OutboxOp::new("visits", "v-1", b"payload".to_vec());
        // UUID v7 layout: version nibble is 7 in the time_hi_and_version field.
        assert_eq!(op.op_id.get_version_num(), 7);
        assert_eq!(op.entity, "visits");
        assert_eq!(op.entity_id, "v-1");
        assert_eq!(op.op, OutboxAction::Upsert);
        assert_eq!(op.attempts, 0);
        assert_eq!(op.next_attempt_at, op.created_at);
        assert!(op.last_error.is_none());
        assert!(!op.parked);
    }

    #[test]
    fn new_two_ops_get_distinct_op_ids() {
        let a = OutboxOp::new("visits", "v-1", b"a".to_vec());
        let b = OutboxOp::new("visits", "v-1", b"a".to_vec());
        assert_ne!(a.op_id, b.op_id);
    }

    #[test]
    fn new_defaults_op_kind_to_upsert_in_v1() {
        // Phase-01 §7.15: only `upsert` ops in v1; `delete` is reserved for
        // Horizon-2. The default constructor must never produce anything else.
        let op = OutboxOp::new("any_entity", "any_id", vec![]);
        assert_eq!(op.op, OutboxAction::Upsert);
    }

    #[test]
    fn reconstitute_round_trips_fields_without_validation() {
        let op_id = Uuid::now_v7();
        let now = Utc::now();
        let op = OutboxOp::reconstitute(
            op_id,
            "visits".into(),
            "v-1".into(),
            OutboxAction::Upsert,
            b"payload".to_vec(),
            now,
            3,
            now,
            Some("boom".into()),
            true,
        );
        assert_eq!(op.op_id, op_id);
        assert_eq!(op.attempts, 3);
        assert_eq!(op.last_error.as_deref(), Some("boom"));
        assert!(op.parked);
    }

    #[test]
    fn next_backoff_exponential_in_minutes() {
        // attempts=0 -> 2^0 = 1 minute = 60 seconds.
        assert_eq!(OutboxOp::next_backoff(0), Duration::from_secs(60));
        // attempts=1 -> 2^1 = 2 minutes = 120 seconds.
        assert_eq!(OutboxOp::next_backoff(1), Duration::from_secs(120));
        // attempts=2 -> 2^2 = 4 minutes = 240 seconds.
        assert_eq!(OutboxOp::next_backoff(2), Duration::from_secs(240));
        // attempts=5 -> 2^5 = 32 minutes = 1920 seconds.
        assert_eq!(OutboxOp::next_backoff(5), Duration::from_secs(1920));
    }

    #[test]
    fn next_backoff_caps_at_sixty_minutes() {
        // Phase-01 §4 SyncEngine push-step 5: backoff capped at 60 minutes.
        // attempts=6 -> 2^6 = 64 minutes, but clamp(0,6) means shift=6 and
        // min(60) caps the wait. attempts beyond 6 are also clamped.
        assert_eq!(OutboxOp::next_backoff(6), Duration::from_secs(60 * 60));
        assert_eq!(OutboxOp::next_backoff(10), Duration::from_secs(60 * 60));
        assert_eq!(OutboxOp::next_backoff(100), Duration::from_secs(60 * 60));
    }

    #[test]
    fn next_backoff_handles_negative_attempts_safely() {
        // clamp(0, 6) prevents underflow on absurd inputs.
        let backoff = OutboxOp::next_backoff(-1);
        assert_eq!(backoff, Duration::from_secs(60));
    }

    #[test]
    fn outbox_op_serializes_op_action_as_lowercase_string() {
        // Wire format: `op` field is `"upsert"` (matches SQL CHECK + envelope).
        let op = OutboxOp::new("visits", "v-1", vec![]);
        let json = serde_json::to_value(&op).unwrap();
        assert_eq!(json["op"], serde_json::json!("upsert"));
    }

    // try_new validation -------------------------------------------------

    #[test]
    fn try_new_returns_ok_on_valid_input() {
        let op = OutboxOp::try_new("visits", "v-1", b"payload".to_vec())
            .expect("valid input should construct");
        assert_eq!(op.entity, "visits");
        assert_eq!(op.entity_id, "v-1");
        assert_eq!(op.op, OutboxAction::Upsert);
    }

    #[test]
    fn try_new_rejects_empty_entity() {
        let err = OutboxOp::try_new("", "v-1", vec![]).expect_err("empty entity rejected");
        assert!(matches!(err, crate::error::AppError::Validation(_)));
    }

    #[test]
    fn try_new_rejects_empty_entity_id() {
        let err =
            OutboxOp::try_new("visits", "", vec![]).expect_err("empty entity_id rejected");
        assert!(matches!(err, crate::error::AppError::Validation(_)));
    }

    #[test]
    fn try_new_rejects_payload_above_eight_mib() {
        let too_big = vec![0u8; PAYLOAD_MAX_BYTES + 1];
        let err = OutboxOp::try_new("visits", "v-1", too_big).expect_err("oversized rejected");
        let msg = err.to_string();
        assert!(msg.contains("payload too large"), "got: {msg}");
    }

    #[test]
    fn try_new_accepts_payload_at_exactly_eight_mib() {
        let exact = vec![0u8; PAYLOAD_MAX_BYTES];
        let op = OutboxOp::try_new("visits", "v-1", exact).expect("boundary should succeed");
        assert_eq!(op.payload.len(), PAYLOAD_MAX_BYTES);
    }
}
