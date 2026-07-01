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
}
