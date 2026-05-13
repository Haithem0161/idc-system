//! Port: audit log persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::db::Tx;
use crate::domains::sync::domain::entities::AuditEntry;
use crate::error::AppResult;

/// Audit query filter (phase-08 §3 Tauri, §7.6 server-side mirror).
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub entity_id_tenant: String,
    pub actor_user_id: Option<String>,
    pub action: Option<String>,
    pub entity: Option<String>,
    pub entity_id_prefix: Option<String>,
    pub from_utc: Option<DateTime<Utc>>,
    pub to_utc: Option<DateTime<Utc>>,
    pub free_text: Option<String>,
    /// Hard cap [1, 100], default 50. Mirrors `/audit/query` server schema.
    pub limit: i64,
    pub offset: i64,
}

impl AuditFilter {
    pub fn clamp(mut self) -> Self {
        if self.limit <= 0 {
            self.limit = 50;
        }
        if self.limit > 100 {
            self.limit = 100;
        }
        if self.offset < 0 {
            self.offset = 0;
        }
        self
    }
}

#[async_trait]
pub trait AuditRepo: Send + Sync {
    /// Append an audit row inside an open transaction. Audit-first ordering
    /// is enforced by `AuditWriter::with_audit`.
    async fn append(&self, tx: &mut Tx<'_>, entry: &AuditEntry) -> AppResult<()>;

    /// Tenant-scoped fetch by descending `at`. Used by the soak harness; the
    /// audit-query UI uses `query` below for filtered fetches.
    async fn list_by_tenant(
        &self,
        entity_id_tenant: &str,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AuditEntry>>;

    /// Filtered query for the audit page (phase-08 §3 Tauri).
    /// Returns rows ordered by `(at DESC, id DESC)`.
    async fn query(&self, filter: &AuditFilter) -> AppResult<Vec<AuditEntry>>;

    /// Soft-delete-then-prune sweep for synced rows older than `cutoff`.
    /// SAFE: only deletes rows where `dirty = 0 AND deleted_at IS NULL`,
    /// preserving any locally-pending audit entries. The predicate is
    /// encoded at the type level (phase-08 §7.1) so callers can't bypass
    /// the dirty check.
    async fn vacuum_unsynced_safe(&self, cutoff: DateTime<Utc>) -> AppResult<u64>;

    /// Oldest audit row's `at`, used by the merge-paginator to decide
    /// whether to fan out to the server (phase-08 §7.4).
    async fn oldest_at(&self, entity_id_tenant: &str) -> AppResult<Option<DateTime<Utc>>>;
}
