//! Port: audit log persistence.

use async_trait::async_trait;

use crate::db::Tx;
use crate::domains::sync::domain::entities::AuditEntry;
use crate::error::AppResult;

#[async_trait]
pub trait AuditRepo: Send + Sync {
    /// Append an audit row inside an open transaction. Audit-first ordering
    /// is enforced by `AuditWriter::with_audit`.
    async fn append(&self, tx: &mut Tx<'_>, entry: &AuditEntry) -> AppResult<()>;

    /// Tenant-scoped fetch by descending `at`. Used by the (Phase-8) audit
    /// query screen and the soak harness.
    async fn list_by_tenant(
        &self,
        entity_id_tenant: &str,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AuditEntry>>;
}
