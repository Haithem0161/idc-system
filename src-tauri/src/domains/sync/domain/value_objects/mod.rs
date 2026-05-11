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
