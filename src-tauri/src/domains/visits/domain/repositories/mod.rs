use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::Tx;
use crate::error::AppResult;

use super::entities::{InventoryAdjustment, Visit, VisitStatus};

#[derive(Debug, Clone, Default)]
pub struct WorkspaceFilters {
    pub statuses: Vec<VisitStatus>,
    pub doctor_ids: Vec<Uuid>,
    pub subtype_ids: Vec<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait VisitRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, v: &Visit) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Visit>>;
    /// Load a visit on the caller's transaction connection. Use this from
    /// inside a `with_audit` write so it does not deadlock by acquiring a
    /// second pool connection (critical on a single-connection pool).
    async fn get_by_id_tx(&self, tx: &mut Tx<'_>, id: Uuid) -> AppResult<Option<Visit>>;
    async fn list_today_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> AppResult<Vec<Visit>>;
    async fn list_drafts_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
    ) -> AppResult<Vec<Visit>>;
    async fn list_workspace(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        filters: &WorkspaceFilters,
        limit: i64,
    ) -> AppResult<Vec<Visit>>;
    async fn count_today_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> AppResult<i64>;
    async fn lines_run_today_by_operator(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> AppResult<i64>;
    /// Every row across ALL tenants, including tombstoned (`deleted_at`) and
    /// already-synced (`dirty = 0`) rows. Used only by the sync resync sweep
    /// (`sync_resync_local`); never gated by `entity_id`/`deleted_at`/`dirty`.
    async fn list_all_for_resync(&self) -> AppResult<Vec<Visit>>;
}

#[async_trait]
pub trait InventoryAdjustmentRepo: Send + Sync {
    async fn append(&self, tx: &mut Tx<'_>, adj: &InventoryAdjustment) -> AppResult<()>;
    /// See `VisitRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<InventoryAdjustment>>;
    async fn list_consume_for_visit(&self, visit_id: Uuid) -> AppResult<Vec<InventoryAdjustment>>;
    async fn list_by_item(
        &self,
        entity_id: &str,
        item_id: Uuid,
        limit: i64,
    ) -> AppResult<Vec<InventoryAdjustment>>;
    /// Recompute `inventory_items.quantity_on_hand` for the given item by
    /// summing all non-deleted adjustments. Returns the new total. The
    /// caller must invoke inside the same tx as the adjustment append so
    /// the recompute reads the just-appended row.
    async fn recompute_item_quantity(&self, tx: &mut Tx<'_>, item_id: Uuid) -> AppResult<i64>;
}
