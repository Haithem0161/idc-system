//! Repository port for the signed/frozen daily close. Pure trait, no sqlx here.
//!
//! The frozen close is additive-only: `insert` materializes a new in-force close
//! (idempotent on id), `reopen` tombstones the in-force close for a day, and the
//! read methods back the immutability guard (`find_in_force_for_date`) and the
//! month overview / detail UI.

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::db::Tx;
use crate::error::AppResult;

use super::super::entities::FrozenClose;

#[async_trait]
pub trait FrozenCloseRepo: Send + Sync {
    /// Insert a freshly signed close inside an existing transaction. Idempotent
    /// on `id` (INSERT OR IGNORE) so a retried push/pull cannot duplicate.
    async fn insert(&self, tx: &mut Tx<'_>, close: &FrozenClose) -> AppResult<()>;

    /// Persist a reopen (the version/tombstone columns) inside a transaction.
    async fn save_reopen(&self, tx: &mut Tx<'_>, close: &FrozenClose) -> AppResult<()>;

    /// The in-force (not reopened) close for a given local day, if any. Backs the
    /// immutability guard and the "is this day frozen?" check.
    async fn find_in_force_for_date(
        &self,
        entity_id: &str,
        target_date: NaiveDate,
    ) -> AppResult<Option<FrozenClose>>;

    /// A single close by id (used by reopen to load-then-mutate).
    async fn find_by_id(&self, entity_id: &str, id: uuid::Uuid) -> AppResult<Option<FrozenClose>>;

    /// All closes (in-force and reopened) overlapping a date range, newest day
    /// first. Backs the month overview.
    async fn list_in_range(
        &self,
        entity_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> AppResult<Vec<FrozenClose>>;

    /// Every daily-close row across ALL tenants (in-force and reopened). Used
    /// only by the sync resync sweep (`sync_resync_local`) to re-enqueue the
    /// full local dataset; never gated by `entity_id` or date range.
    async fn list_all_for_resync(&self) -> AppResult<Vec<FrozenClose>>;
}
