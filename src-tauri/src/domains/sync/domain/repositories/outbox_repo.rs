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
    /// the next attempt at `now + backoff`. Use for SERVER-side failures that
    /// are op-specific and may eventually exhaust the retry cap.
    async fn mark_failure(&self, op_id: Uuid, error: &str, backoff_secs: u64) -> AppResult<()>;

    /// Reschedule after a TRANSPORT failure (network down, server unreachable)
    /// WITHOUT bumping `attempts`. A device that is merely offline must not burn
    /// its retry cap and strand every queued op once connectivity returns.
    async fn reschedule_transient(
        &self,
        op_id: Uuid,
        error: &str,
        backoff_secs: u64,
    ) -> AppResult<()>;

    /// Mark a row as parked (conflict landed; do not retry until the resolver
    /// flips `parked` back to 0).
    async fn park(&self, op_id: Uuid) -> AppResult<()>;

    /// Park a row AND record why (server rejected it per-op). Parked rows are
    /// excluded from `next_batch` so one poison op never blocks the queue, and
    /// surface via `list_stuck` for manual inspection/recovery.
    async fn park_with_error(&self, op_id: Uuid, error: &str) -> AppResult<()>;

    /// Count ops that can no longer make progress on their own: parked, or
    /// having reached the attempts cap. These are surfaced to the UI so the
    /// user knows work is stranded instead of it vanishing silently.
    async fn stuck_count(&self) -> AppResult<u32>;

    /// List stuck ops (parked or attempts-capped) for inspection/recovery.
    async fn list_stuck(&self) -> AppResult<Vec<OutboxOp>>;

    /// Requeue a stuck op for another push attempt: reset `attempts` to 0,
    /// clear `parked`, and schedule it immediately. Returns the number of rows
    /// affected (0 if the op_id was unknown).
    async fn requeue_stuck(&self, op_id: Uuid) -> AppResult<u64>;

    /// Server acknowledged the listed ops: delete them from the local outbox.
    async fn delete_acked(&self, op_ids: &[Uuid]) -> AppResult<()>;

    /// Mark the business rows behind acknowledged ops as clean
    /// (`dirty = 0`, `last_synced_at = now`). Without this the source rows
    /// stay `dirty = 1` forever after a successful push, so the dirty flag is
    /// meaningless and the audit-retention vacuum (which only purges
    /// `dirty = 0` rows) can never reclaim own-device rows. Pairs are
    /// `(entity_table, entity_id)`; unknown tables are skipped.
    async fn mark_entities_synced(&self, entities: &[(String, String)]) -> AppResult<()>;
}
