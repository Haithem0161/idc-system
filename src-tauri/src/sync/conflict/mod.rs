//! Per-entity conflict resolution policies and the shared conflict envelope.
//!
//! Policies: `last-write-wins`, `field-merge`, `additive-only`, `manual`.
//! Phase-01 ships only `additive-only` (`audit_log`); subsequent phases add
//! entries here as they introduce syncable entities.

use serde::{Deserialize, Serialize};

/// Frontend-facing conflict envelope. Camel-case for direct JSON binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub op_id: String,
    pub entity: String,
    pub entity_id: String,
    pub server_payload: serde_json::Value,
    pub local_payload: serde_json::Value,
    pub reason: String,
}

impl From<crate::domains::sync::infrastructure::ServerConflict> for Conflict {
    fn from(c: crate::domains::sync::infrastructure::ServerConflict) -> Self {
        Self {
            op_id: c.op_id,
            entity: c.entity,
            entity_id: c.entity_id,
            server_payload: c.server_payload,
            local_payload: c.local_payload,
            reason: c.reason,
        }
    }
}

/// Stable policy name -- mirrored in `phase-XX.md` declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    LastWriteWins,
    AdditiveOnly,
    Manual,
}

/// Look up the policy for a given entity name. Unknown entities default to
/// `Manual` (safer: 409 surfaces the issue rather than overwriting blindly).
pub fn policy_for(entity: &str) -> Policy {
    match entity {
        "audit_log" => Policy::AdditiveOnly,
        _ => Policy::Manual,
    }
}
