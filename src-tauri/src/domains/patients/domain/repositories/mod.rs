use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::db::Tx;
use crate::error::AppResult;

use super::entities::Patient;

/// Sort order for the archive list. Parsed from a string at the command
/// boundary so user input never reaches `ORDER BY` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatientSort {
    NameAsc,
    NameDesc,
    CreatedDesc,
    UpdatedDesc,
}

impl PatientSort {
    /// Map to a fixed, safe `ORDER BY` clause body (no user interpolation).
    pub fn order_by(self) -> &'static str {
        match self {
            PatientSort::NameAsc => "name COLLATE NOCASE ASC",
            PatientSort::NameDesc => "name COLLATE NOCASE DESC",
            PatientSort::CreatedDesc => "created_at DESC",
            PatientSort::UpdatedDesc => "updated_at DESC",
        }
    }

    /// Parse the wire string; unknown/empty defaults to `UpdatedDesc`.
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("name_asc") => PatientSort::NameAsc,
            Some("name_desc") => PatientSort::NameDesc,
            Some("created_desc") => PatientSort::CreatedDesc,
            _ => PatientSort::UpdatedDesc,
        }
    }
}

/// Filter for the paginated archive list.
#[derive(Debug, Clone)]
pub struct PatientListFilter {
    pub entity_id: String,
    pub query: Option<String>,
    pub include_deleted: bool,
    pub sort: PatientSort,
    pub limit: i64,
    pub offset: i64,
}

/// A single visit in a patient's history table. All fields are read directly
/// from the visit's immutable `*_snapshot` columns -- no JOIN to catalog
/// tables, so a renamed/deleted check type still renders the visit's name as
/// it was at lock time.
#[derive(Debug, Clone, Serialize)]
pub struct VisitSummary {
    pub id: Uuid,
    pub status: String,
    pub locked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub total_amount_iqd: Option<i64>,
    pub check_type_name_ar: Option<String>,
    pub check_type_name_en: Option<String>,
    pub doctor_name: Option<String>,
    pub void_reason: Option<String>,
}

/// Aggregate stats for a patient's detail header.
#[derive(Debug, Clone, Serialize)]
pub struct PatientStats {
    pub total_visits: i64,
    /// Sum of `total_amount_iqd_snapshot` over LOCKED visits only.
    pub total_spent_iqd: i64,
    pub last_visit_at: Option<DateTime<Utc>>,
    pub draft_count: i64,
    pub voided_count: i64,
}

/// A group of patients that look like duplicates of each other.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// `"name"` or `"phone"` -- what they collide on.
    pub kind: String,
    /// The normalized key they share (display hint).
    pub key: String,
    pub patient_ids: Vec<Uuid>,
}

#[async_trait]
pub trait PatientRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, p: &Patient) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Patient>>;
    async fn list_recent(&self, entity_id: &str, limit: i64) -> AppResult<Vec<Patient>>;
    async fn search(&self, entity_id: &str, query: &str, limit: i64) -> AppResult<Vec<Patient>>;
    async fn count_live_visits(&self, patient_id: Uuid) -> AppResult<i64>;

    /// Every row across ALL tenants, including tombstoned (`deleted_at`) and
    /// already-synced (`dirty = 0`) rows. Used only by the sync resync sweep
    /// (`sync_resync_local`) to re-enqueue the full local dataset; never gated
    /// by `entity_id`/`deleted_at`/`dirty`.
    async fn list_all_for_resync(&self) -> AppResult<Vec<Patient>>;

    /// Paginated, sortable, optionally-searched archive list. When
    /// `filter.query` is non-empty the FTS index narrows by name; otherwise a
    /// plain scan. Tombstones excluded unless `include_deleted`.
    async fn list(&self, filter: &PatientListFilter) -> AppResult<Vec<Patient>>;

    /// A patient's visit history, newest first, read from visit snapshots.
    async fn list_visits_by_patient(
        &self,
        patient_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<VisitSummary>>;

    /// Aggregate stats for the detail header.
    async fn patient_stats(&self, patient_id: Uuid) -> AppResult<PatientStats>;

    /// Candidate duplicate groups (same normalized name OR same normalized
    /// phone), live rows only.
    async fn find_duplicates(&self, entity_id: &str) -> AppResult<Vec<DuplicateGroup>>;

    /// Ids of every live visit currently attributed to `patient_id`, read
    /// inside the caller's transaction. The merge flow loads each visit,
    /// re-attributes it to the survivor via the visit entity, and re-upserts
    /// it (so each carries a correct push payload) -- this method only finds
    /// the work set.
    async fn live_visit_ids_for_patient(
        &self,
        tx: &mut Tx<'_>,
        patient_id: Uuid,
    ) -> AppResult<Vec<Uuid>>;
}
