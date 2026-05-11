//! Port: outbox queue persistence.

use async_trait::async_trait;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::error::AppResult;

#[async_trait]
pub trait OutboxRepo: Send + Sync {
    /// Insert an outbox row inside an open transaction (called from
    /// `AuditWriter::with_audit`).
    async fn enqueue(&self, tx: &mut Tx<'_>, op: &OutboxOp) -> AppResult<()>;

    /// Select up to `limit` rows whose `next_attempt_at <= now`, `attempts < 10`,
    /// `parked = 0`. Returned in `next_attempt_at` order ascending.
    async fn next_batch(&self, limit: usize) -> AppResult<Vec<OutboxOp>>;

    /// Total pending count (`attempts < 10` and not parked).
    async fn pending_count(&self) -> AppResult<u32>;

    /// Mark a transient failure: bump `attempts`, set `last_error`, and schedule
    /// the next attempt at `now + backoff`.
    async fn mark_failure(&self, op_id: Uuid, error: &str, backoff_secs: u64) -> AppResult<()>;

    /// Mark a row as parked (conflict landed; do not retry until the resolver
    /// flips `parked` back to 0).
    async fn park(&self, op_id: Uuid) -> AppResult<()>;

    /// Server acknowledged the listed ops: delete them from the local outbox.
    async fn delete_acked(&self, op_ids: &[Uuid]) -> AppResult<()>;
}
