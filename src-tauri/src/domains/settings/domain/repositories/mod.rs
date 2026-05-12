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
}
