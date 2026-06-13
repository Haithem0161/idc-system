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

/// Look up the declared conflict-resolution policy for an entity. The mapping
/// mirrors the per-entity declarations in the phase files and
/// `docs/idc-system/status.md` (6 policies in use):
///
/// - `additive-only` -> `audit_log`, `operator_shifts`, `inventory_adjustments`
/// - `last-write-wins` -> `users`, all 8 catalog entities, `inventory_items`,
///   `patients`
/// - `manual` -> `settings`, `visits`
///
/// Unknown entities default to `Manual` (safer: a server 409 surfaces the gap
/// rather than overwriting blindly).
///
/// NOTE: the registry is authoritative for what EACH entity's policy is, but
/// the engine's pull-apply and push-parking paths currently enforce these
/// inline (the LWW `version` gate in `puller_entities`, additive `INSERT OR
/// IGNORE`, and the server-side manual parking for settings/visits) rather than
/// dispatching through `policy_for`. Wiring a single dispatch site through this
/// registry is a tracked follow-up; until then this function is the truthful
/// source of the declared policy for any caller that needs it.
pub fn policy_for(entity: &str) -> Policy {
    match entity {
        // additive-only: append-only logs / ledgers; both writes survive.
        "audit_log" | "operator_shifts" | "inventory_adjustments" => Policy::AdditiveOnly,
        // last-write-wins: users + the 8 catalog entities + inventory_items + patients.
        "users"
        | "check_types"
        | "check_subtypes"
        | "doctors"
        | "doctor_check_pricing"
        | "operators"
        | "operator_specialties"
        | "inventory_items"
        | "inventory_consumption_map"
        | "patients" => Policy::LastWriteWins,
        // manual: domain-critical, must reconcile via the resolver UI.
        "settings" | "visits" => Policy::Manual,
        _ => Policy::Manual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additive_only_entities_map_correctly() {
        for e in ["audit_log", "operator_shifts", "inventory_adjustments"] {
            assert_eq!(policy_for(e), Policy::AdditiveOnly, "entity {e}");
        }
    }

    #[test]
    fn last_write_wins_entities_map_correctly() {
        for e in [
            "users",
            "check_types",
            "check_subtypes",
            "doctors",
            "doctor_check_pricing",
            "operators",
            "operator_specialties",
            "inventory_items",
            "inventory_consumption_map",
            "patients",
        ] {
            assert_eq!(policy_for(e), Policy::LastWriteWins, "entity {e}");
        }
    }

    #[test]
    fn manual_entities_map_correctly() {
        for e in ["settings", "visits"] {
            assert_eq!(policy_for(e), Policy::Manual, "entity {e}");
        }
    }

    #[test]
    fn unknown_entity_defaults_to_manual() {
        assert_eq!(policy_for("totally_unknown_entity"), Policy::Manual);
    }
}
