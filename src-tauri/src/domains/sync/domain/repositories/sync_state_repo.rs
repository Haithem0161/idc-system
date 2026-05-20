//! Port: singleton sync_state row.

use async_trait::async_trait;

use crate::domains::sync::domain::entities::SyncState;
use crate::error::AppResult;

#[async_trait]
pub trait SyncStateRepo: Send + Sync {
    /// Read the singleton row.
    async fn get(&self) -> AppResult<SyncState>;

    /// Persist a pull cursor (atomic with the changes that produced it lives
    /// in the engine, not the repo).
    async fn put_pull_cursor(&self, cursor: &str) -> AppResult<()>;

    /// Phase-01 §4 pull-step 3 + DEF-002 fix: persist the pull cursor on
    /// the SAME connection as the caller's apply transaction. Required by
    /// the puller, which holds a write tx and would deadlock against a
    /// second-connection write on a real-world file SQLite. The standalone
    /// `put_pull_cursor` remains for non-transactional callers.
    async fn put_pull_cursor_in_tx(
        &self,
        tx: &mut crate::db::Tx<'_>,
        cursor: &str,
    ) -> AppResult<()>;

    /// Stamp the last pushed-at moment.
    async fn mark_pushed(&self) -> AppResult<()>;

    /// Ensure a row exists; create it with a fresh `device_id` if not. Returns
    /// the canonical device_id.
    async fn ensure_device_id(&self, device_id: &str) -> AppResult<String>;

    /// Stamp `last_audit_vacuum_at = now` after a successful audit-vacuum run.
    async fn mark_audit_vacuumed(&self, at: chrono::DateTime<chrono::Utc>) -> AppResult<()>;

    /// Read the persisted sync server URL (migration 010). `None` when the
    /// user has not finished first-launch setup yet.
    async fn get_server_url(&self) -> AppResult<Option<String>>;

    /// Persist the sync server URL. Called by
    /// `config_set_sync_server_url_impl` so the setting survives a restart.
    async fn put_server_url(&self, url: &str) -> AppResult<()>;
}
