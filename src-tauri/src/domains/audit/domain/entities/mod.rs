//! Audit DTOs returned to the IPC boundary.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// One row in the audit table (frontend `<AuditTable>`).
///
/// Carries `dirty` so the Pending-sync column from phase-05 §7.29 can render
/// without a second fetch (phase-08 §7.15).
#[derive(Debug, Clone, Serialize)]
pub struct AuditRowDto {
    pub id: String,
    pub at: DateTime<Utc>,
    pub actor_user_id: String,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub delta: serde_json::Value,
    pub device_id: String,
    pub version: i64,
    pub dirty: bool,
    pub source: AuditSource,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditSource {
    Local,
    Server,
}

/// Page returned by `audit::query`. `mode` tells the UI whether to render
/// the `<ServerBackedBadge>` (phase-08 §3 Frontend, §7.25).
#[derive(Debug, Clone, Serialize)]
pub struct AuditPage {
    pub rows: Vec<AuditRowDto>,
    pub mode: AuditQueryMode,
    pub next_offset: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditQueryMode {
    Local,
    Server,
    Merged,
}

/// `diagnostics::summary` payload (phase-08 §7.17).
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiagnosticsSummaryDto {
    pub lock_latency_p95_ms: Option<i64>,
    pub outbox_depth: u32,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub conflict_count_7d: u32,
    pub receipt_print_success_rate_30d: Option<f64>,
}
