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

    /// Stamp the last pushed-at moment.
    async fn mark_pushed(&self) -> AppResult<()>;

    /// Ensure a row exists; create it with a fresh `device_id` if not. Returns
    /// the canonical device_id.
    async fn ensure_device_id(&self, device_id: &str) -> AppResult<String>;
}
