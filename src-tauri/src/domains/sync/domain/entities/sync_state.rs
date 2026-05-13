//! Singleton row in the `sync_state` table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub pull_cursor: Option<String>,
    pub last_pulled_at: Option<DateTime<Utc>>,
    pub last_pushed_at: Option<DateTime<Utc>>,
    pub device_id: String,
    /// Last successful audit-vacuum sweep. Read on app start by
    /// `AuditVacuumJob` to decide whether to run immediately (missed-run
    /// recovery, phase-08 §7.2). Stamped after every successful run.
    pub last_audit_vacuum_at: Option<DateTime<Utc>>,
}
