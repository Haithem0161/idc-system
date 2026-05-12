//! Repository ports for the auth bounded context.

use async_trait::async_trait;
use uuid::Uuid;

use crate::db::Tx;
use crate::error::AppResult;

use super::entities::User;

#[derive(Debug, Clone, Default)]
pub struct UserListFilter {
    pub include_inactive: bool,
    pub entity_id: Option<String>,
}

#[async_trait]
pub trait UserRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, user: &User) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<User>>;
    async fn get_by_email(&self, email: &str, entity_id: &str) -> AppResult<Option<User>>;
    async fn list(&self, filter: UserListFilter) -> AppResult<Vec<User>>;
    async fn count(&self) -> AppResult<u32>;
}
