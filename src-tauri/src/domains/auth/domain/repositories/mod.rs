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
    /// Resolve a user by email across ALL tenants. Returns the row only when
    /// the email is unambiguous (exactly one non-deleted match); `None` when no
    /// match OR when it is ambiguous across tenants. Used by offline login,
    /// which has no tenant hint to scope on -- mirroring the server's
    /// tenant-from-user resolution.
    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>>;
    async fn list(&self, filter: UserListFilter) -> AppResult<Vec<User>>;
    async fn count(&self) -> AppResult<u32>;

    /// Every user row across ALL tenants, including tombstoned (`deleted_at`)
    /// and already-synced (`dirty = 0`) rows. The returned entities carry the
    /// `password_hash` so the resync push can restore credentials on the
    /// server (the normal create path also pushes the hash). Used only by the
    /// sync resync sweep (`sync_resync_local`); never gated by
    /// `entity_id`/`deleted_at`/`dirty`.
    async fn list_all_for_resync(&self) -> AppResult<Vec<User>>;
}
