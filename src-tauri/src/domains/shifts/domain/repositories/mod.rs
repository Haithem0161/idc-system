//! Repository port for the shifts bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::Tx;
use crate::error::AppResult;

use super::entities::OperatorShift;

/// Two shifts for the same operator whose [check_in_at..check_out_at) ranges
/// overlap. Open shifts (`check_out_at = None`) are treated as extending to
/// "now" for overlap detection.
#[derive(Debug, Clone)]
pub struct OverlapPair {
    pub left: OperatorShift,
    pub right: OperatorShift,
}

#[async_trait]
pub trait OperatorShiftRepo: Send + Sync {
    /// Upsert by `id`. Implementations MUST use `INSERT ... ON CONFLICT(id)
    /// DO UPDATE` so that pull-side LWW survives without race-y deletes.
    async fn upsert(&self, tx: &mut Tx<'_>, shift: &OperatorShift) -> AppResult<()>;

    /// Fetch a single shift by id (returns soft-deleted rows -- callers that
    /// need to exclude tombstones must filter).
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<OperatorShift>>;

    /// Open shifts tenant-wide (sorted by `check_in_at ASC`). Excludes
    /// soft-deleted rows.
    async fn list_open(&self, entity_id: &str) -> AppResult<Vec<OperatorShift>>;

    /// Today's shifts (both open and closed) in the given tenant, sorted by
    /// `check_in_at ASC`. `today_start` and `today_end` are caller-provided
    /// so the timezone boundary lives in the application layer.
    async fn history_today(
        &self,
        entity_id: &str,
        today_start: DateTime<Utc>,
        today_end: DateTime<Utc>,
    ) -> AppResult<Vec<OperatorShift>>;

    /// Does this operator currently have an open shift? Excludes the
    /// optional `except_id` so retroactive edits don't false-positive.
    async fn has_open_for_operator(
        &self,
        operator_id: Uuid,
        except_id: Option<Uuid>,
    ) -> AppResult<bool>;

    /// Return any pairs of non-deleted shifts for `operator_id` whose
    /// time ranges overlap. Open shifts extend to `now`.
    async fn list_overlaps_for_operator(
        &self,
        operator_id: Uuid,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<OverlapPair>>;

    /// All non-deleted shifts for `operator_id` in chronological order.
    /// Used by `ShiftService::edit` to compute overlap against a candidate
    /// window. Excludes the optional `except_id` so the editing row never
    /// collides with itself.
    async fn list_for_operator(
        &self,
        operator_id: Uuid,
        except_id: Option<Uuid>,
    ) -> AppResult<Vec<OperatorShift>>;

    /// Return ALL overlaps across the tenant (used for the conflict banner).
    async fn list_overlaps(
        &self,
        entity_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<OverlapPair>>;
}
