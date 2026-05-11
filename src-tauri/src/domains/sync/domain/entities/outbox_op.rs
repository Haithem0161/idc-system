//! Outbox queue entry. One row = one pending push for a single domain row.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::value_objects::OutboxAction;

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
