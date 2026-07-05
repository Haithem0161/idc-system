//! Setting repository port.

use async_trait::async_trait;

use crate::db::Tx;
use crate::domains::settings::domain::entities::Setting;
use crate::error::AppResult;

#[async_trait]
pub trait SettingRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()>;
    async fn get_by_key(&self, key: &str, entity_id: &str) -> AppResult<Option<Setting>>;
    async fn list(&self, entity_id: &str) -> AppResult<Vec<Setting>>;

    /// Every setting row across ALL tenants, including tombstoned
    /// (`deleted_at`) and already-synced (`dirty = 0`) rows. Used only by the
    /// sync resync sweep (`sync_resync_local`); never gated by
    /// `entity_id`/`deleted_at`/`dirty`.
    async fn list_all_for_resync(&self) -> AppResult<Vec<Setting>>;

    /// All live (`deleted_at IS NULL`) rows for a scope, ordered by `key`.
    /// Used by the login-time scope reconcile to enumerate `'unscoped'` rows.
    async fn list_live_by_entity(&self, entity_id: &str) -> AppResult<Vec<Setting>>;

    /// True iff a live (`deleted_at IS NULL`) row exists for `(entity_id, key)`.
    async fn has_live_key(&self, key: &str, entity_id: &str) -> AppResult<bool>;

    /// Rewrite every mutable column of an EXISTING row, matched by `id`. Unlike
    /// `upsert`, this does not use the `(entity_id, key)` conflict path, so it
    /// safely applies a tombstone (sets `deleted_at`) or a re-point (changes
    /// `entity_id`). Used only by the scope reconcile.
    async fn update_row_by_id(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()>;
}
