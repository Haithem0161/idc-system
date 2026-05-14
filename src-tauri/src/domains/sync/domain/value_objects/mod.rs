//! Value objects for the sync bounded context.

use serde::{Deserialize, Serialize};

/// The operation kind carried by an outbox row.
///
/// Phase-01 only supports `Upsert`. The `Delete` variant is reserved for the
/// Horizon-2 PII purge (see phase-01 §7.15); the server rejects `delete` ops
/// with `422 UNSUPPORTED_OP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboxAction {
    Upsert,
}

impl OutboxAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
        }
    }
}

impl std::fmt::Display for OutboxAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The high-level audit event kinds the application emits.
///
/// Phase-01 owns the union; subsequent phases extend it by adding variants.
/// The local SQLite column is unconstrained -- this enum is the source of
/// truth (see phase-01 §7.8, §7.36).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Create,
    Update,
    SoftDelete,
    Lock,
    Void,
    Discard,
    ClockIn,
    ClockOut,
    PasswordChange,
    Login,
    Logout,
    ConflictResolve,
    Vacuum,
    DailyCloseRun,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::SoftDelete => "soft_delete",
            Self::Lock => "lock",
            Self::Void => "void",
            Self::Discard => "discard",
            Self::ClockIn => "clock_in",
            Self::ClockOut => "clock_out",
            Self::PasswordChange => "password_change",
            Self::Login => "login",
            Self::Logout => "logout",
            Self::ConflictResolve => "conflict_resolve",
            Self::Vacuum => "vacuum",
            Self::DailyCloseRun => "daily_close_run",
        }
    }
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Engine-visible sync status. Mirrors the frontend `SyncStatus` enum and
/// the five-state pill in PRD §10.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    Idle,
    Pushing,
    Pulling,
    Offline,
    Error,
}

impl SyncStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Pushing => "pushing",
            Self::Pulling => "pulling",
            Self::Offline => "offline",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned by `SyncStatus::transition` when the requested edge is not
/// part of the legal state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: SyncStatus,
    pub to: SyncStatus,
}

impl std::fmt::Display for IllegalTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "illegal sync status transition: {} -> {}",
            self.from, self.to
        )
    }
}

impl std::error::Error for IllegalTransition {}

impl SyncStatus {
    /// Phase-01 §1.1 five-state sync pill state machine.
    ///
    /// Legal edges:
    /// - From `Idle`: any state (`Pushing` / `Pulling` / `Offline` / `Error`).
    /// - From `Pushing` / `Pulling`: back to `Idle` (clean drain) OR `Offline`
    ///   (network dropped mid-batch) OR `Error` (server returned a conflict
    ///   that requires the resolver UI).
    /// - From `Offline`: back to `Idle` (network restored) OR `Error`.
    /// - From `Error`: back to `Idle` (resolver succeeded) OR `Offline`
    ///   (network dropped while the user was resolving). Every state is also
    ///   allowed to no-op back to itself (idempotency).
    pub fn can_transition_to(self, to: SyncStatus) -> bool {
        if self == to {
            return true;
        }
        use SyncStatus::*;
        matches!(
            (self, to),
            (Idle, Pushing)
                | (Idle, Pulling)
                | (Idle, Offline)
                | (Idle, Error)
                | (Pushing, Idle)
                | (Pushing, Offline)
                | (Pushing, Error)
                | (Pulling, Idle)
                | (Pulling, Offline)
                | (Pulling, Error)
                | (Offline, Idle)
                | (Offline, Error)
                | (Error, Idle)
                | (Error, Offline)
        )
    }

    /// Result-typed equivalent of `can_transition_to`; returns the target
    /// state on success or `IllegalTransition` on a forbidden edge.
    pub fn transition(self, to: SyncStatus) -> Result<SyncStatus, IllegalTransition> {
        if self.can_transition_to(to) {
            Ok(to)
        } else {
            Err(IllegalTransition { from: self, to })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // OutboxAction --------------------------------------------------------

    #[test]
    fn outbox_action_renders_as_lowercase_upsert() {
        assert_eq!(OutboxAction::Upsert.as_str(), "upsert");
        assert_eq!(format!("{}", OutboxAction::Upsert), "upsert");
    }

    #[test]
    fn outbox_action_round_trips_via_serde() {
        let value = serde_json::to_value(OutboxAction::Upsert).unwrap();
        assert_eq!(value, json!("upsert"));
        let back: OutboxAction = serde_json::from_value(value).unwrap();
        assert_eq!(back, OutboxAction::Upsert);
    }

    #[test]
    fn outbox_action_rejects_unknown_strings() {
        // Phase-01 §7.15 invariant: the wire format only carries `upsert` in v1.
        let err = serde_json::from_value::<OutboxAction>(json!("delete"));
        assert!(err.is_err(), "delete must not deserialize in v1");
        let err = serde_json::from_value::<OutboxAction>(json!("create"));
        assert!(err.is_err(), "create must not deserialize");
    }

    // AuditAction ---------------------------------------------------------

    #[test]
    fn audit_action_enumerates_all_fourteen_phase01_to_phase07_variants() {
        // Phase-01 §7.36 final enum -- this list is load-bearing for the
        // server-side CHECK and the audit-vacuum tracker. If a variant is
        // added or renamed, this test fails on purpose.
        let all: Vec<(&str, AuditAction)> = vec![
            ("create", AuditAction::Create),
            ("update", AuditAction::Update),
            ("soft_delete", AuditAction::SoftDelete),
            ("lock", AuditAction::Lock),
            ("void", AuditAction::Void),
            ("discard", AuditAction::Discard),
            ("clock_in", AuditAction::ClockIn),
            ("clock_out", AuditAction::ClockOut),
            ("password_change", AuditAction::PasswordChange),
            ("login", AuditAction::Login),
            ("logout", AuditAction::Logout),
            ("conflict_resolve", AuditAction::ConflictResolve),
            ("vacuum", AuditAction::Vacuum),
            ("daily_close_run", AuditAction::DailyCloseRun),
        ];
        assert_eq!(all.len(), 14);
        for (expected, action) in all {
            assert_eq!(action.as_str(), expected);
            assert_eq!(format!("{}", action), expected);
        }
    }

    #[test]
    fn audit_action_round_trips_each_variant_via_serde() {
        for action in [
            AuditAction::Create,
            AuditAction::Update,
            AuditAction::SoftDelete,
            AuditAction::Lock,
            AuditAction::Void,
            AuditAction::Discard,
            AuditAction::ClockIn,
            AuditAction::ClockOut,
            AuditAction::PasswordChange,
            AuditAction::Login,
            AuditAction::Logout,
            AuditAction::ConflictResolve,
            AuditAction::Vacuum,
            AuditAction::DailyCloseRun,
        ] {
            let json = serde_json::to_value(action).unwrap();
            let back: AuditAction = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(back, action, "round-trip failed for {}", action);
            assert_eq!(json, serde_json::json!(action.as_str()));
        }
    }

    #[test]
    fn audit_action_rejects_unknown_string() {
        let err = serde_json::from_value::<AuditAction>(json!("renamed_variant"));
        assert!(err.is_err());
    }

    // SyncStatus ----------------------------------------------------------

    #[test]
    fn sync_status_serializes_lowercase_for_all_five_states() {
        // Phase-01 §7.30 wire contract: the pill emits lowercase strings.
        let pairs = [
            (SyncStatus::Idle, "idle"),
            (SyncStatus::Pushing, "pushing"),
            (SyncStatus::Pulling, "pulling"),
            (SyncStatus::Offline, "offline"),
            (SyncStatus::Error, "error"),
        ];
        for (status, expected) in pairs {
            let json = serde_json::to_value(status).unwrap();
            assert_eq!(json, serde_json::json!(expected));
            let back: SyncStatus = serde_json::from_value(json).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn sync_status_rejects_typo_string() {
        let err = serde_json::from_value::<SyncStatus>(json!("syncing"));
        assert!(err.is_err());
    }

    // SyncStatus::transition state machine -------------------------------

    #[test]
    fn sync_status_allows_idle_to_any_state() {
        for to in [
            SyncStatus::Pushing,
            SyncStatus::Pulling,
            SyncStatus::Offline,
            SyncStatus::Error,
            SyncStatus::Idle,
        ] {
            assert!(
                SyncStatus::Idle.can_transition_to(to),
                "Idle -> {to} must be legal"
            );
        }
    }

    #[test]
    fn sync_status_pushing_returns_to_idle_offline_or_error() {
        assert!(SyncStatus::Pushing.can_transition_to(SyncStatus::Idle));
        assert!(SyncStatus::Pushing.can_transition_to(SyncStatus::Offline));
        assert!(SyncStatus::Pushing.can_transition_to(SyncStatus::Error));
        // Cannot pivot directly from Pushing to Pulling without passing
        // through Idle -- the engine drains one loop at a time.
        assert!(!SyncStatus::Pushing.can_transition_to(SyncStatus::Pulling));
    }

    #[test]
    fn sync_status_pulling_returns_to_idle_offline_or_error() {
        assert!(SyncStatus::Pulling.can_transition_to(SyncStatus::Idle));
        assert!(SyncStatus::Pulling.can_transition_to(SyncStatus::Offline));
        assert!(SyncStatus::Pulling.can_transition_to(SyncStatus::Error));
        assert!(!SyncStatus::Pulling.can_transition_to(SyncStatus::Pushing));
    }

    #[test]
    fn sync_status_offline_returns_only_to_idle_or_error() {
        assert!(SyncStatus::Offline.can_transition_to(SyncStatus::Idle));
        assert!(SyncStatus::Offline.can_transition_to(SyncStatus::Error));
        // Cannot start pushing / pulling without first reaching Idle.
        assert!(!SyncStatus::Offline.can_transition_to(SyncStatus::Pushing));
        assert!(!SyncStatus::Offline.can_transition_to(SyncStatus::Pulling));
    }

    #[test]
    fn sync_status_error_to_offline_allowed_when_network_drops() {
        // Phase-01 §1.1: parked-conflict + network drop mid-resolution
        // transitions Error -> Offline. The resolver UI surfaces both.
        assert!(SyncStatus::Error.can_transition_to(SyncStatus::Offline));
        assert!(SyncStatus::Error.can_transition_to(SyncStatus::Idle));
        assert!(!SyncStatus::Error.can_transition_to(SyncStatus::Pushing));
        assert!(!SyncStatus::Error.can_transition_to(SyncStatus::Pulling));
    }

    #[test]
    fn sync_status_self_transition_is_idempotent() {
        for s in [
            SyncStatus::Idle,
            SyncStatus::Pushing,
            SyncStatus::Pulling,
            SyncStatus::Offline,
            SyncStatus::Error,
        ] {
            assert!(s.can_transition_to(s), "{s} -> {s} must be a no-op");
            assert_eq!(s.transition(s), Ok(s));
        }
    }

    #[test]
    fn sync_status_transition_returns_illegal_transition_error() {
        let err = SyncStatus::Offline
            .transition(SyncStatus::Pushing)
            .expect_err("Offline -> Pushing must be rejected");
        assert_eq!(err.from, SyncStatus::Offline);
        assert_eq!(err.to, SyncStatus::Pushing);
        assert!(err.to_string().contains("offline -> pushing"), "got: {err}");
    }
}
