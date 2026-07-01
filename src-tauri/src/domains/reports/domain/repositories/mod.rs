//! Repository port (trait) for the reports bounded context. Pure trait, no
//! sqlx imports here.

mod frozen_close;
pub use frozen_close::FrozenCloseRepo;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;

use super::entities::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisitsAggregate {
    pub visits: i64,
    /// Billed revenue: SUM(total_amount_iqd_snapshot) = price + dye.
    pub revenue_iqd: i64,
    /// Cash actually collected: SUM(COALESCE(amount_paid_override_iqd,
    /// total_amount_iqd_snapshot)). Equals `revenue_iqd` when no visit was
    /// overridden; less than it when receptionists collected less than billed.
    pub collected_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
    /// Report carve-out owed to the reporting doctor:
    /// SUM(report_amount_snapshot_iqd). Subtracted from every net derived here.
    pub report_iqd: i64,
    /// مندوب carve-out owed to representatives:
    /// SUM(mandoub_cut_snapshot_iqd). Subtracted from every net derived here,
    /// after the report carve-out.
    pub mandoub_cut_iqd: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoidedAggregate {
    pub count: i64,
    pub value_iqd: i64,
}

/// Read-model repository. One trait covers every aggregation the reports
/// need; the sqlx impl uses partial indexes from migration 007.
#[async_trait]
pub trait ReportsReadModel: Send + Sync {
    /// Sum of locked / voided visits' snapshots in `[from, to)`.
    async fn aggregate_visits(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<VisitsAggregate>;

    /// Row-per-visit list (Visits Report mode=rows).
    async fn list_visit_rows(&self, filters: &VisitsReportFilters) -> AppResult<Vec<VisitRow>>;

    /// Aggregated rows for groupBy modes (§7.14).
    async fn list_visit_groups(
        &self,
        filters: &VisitsReportFilters,
    ) -> AppResult<Vec<VisitsReportGroup>>;

    /// Per-doctor earnings aggregate over `[from, to)`. Always includes the
    /// "house" pseudo-row (doctor_id IS NULL).
    async fn aggregate_doctor_earnings(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<DoctorEarningsRow>>;

    /// Per-check + per-subtype breakdown for one doctor (or house when `None`).
    async fn doctor_per_check(
        &self,
        entity_id: &str,
        doctor_id: Option<Uuid>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<DoctorPerCheckRow>>;

    /// Visit rows attributed to one doctor (or house when `None`).
    async fn doctor_source_visits(
        &self,
        entity_id: &str,
        doctor_id: Option<Uuid>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<VisitRow>>;

    async fn aggregate_operator_earnings(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<OperatorEarningsRow>>;

    /// Per-مندوب earnings aggregate over `[from, to)`. Only visits carrying a
    /// مندوب (mandoub_id IS NOT NULL) contribute. Uses `visits_locked_mandoub_idx`.
    async fn aggregate_mandoub_earnings(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<MandoubEarningsRow>>;

    /// Visit rows attributed to one مندوب (backs the مندوب drilldown).
    async fn mandoub_source_visits(
        &self,
        entity_id: &str,
        mandoub_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<VisitRow>>;

    async fn operator_shifts_window(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<OperatorShiftRow>>;

    async fn operator_source_visits(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<VisitRow>>;

    /// Daily aggregations used by Daily Close (§7.9).
    async fn daily_per_doctor(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<DoctorDailyRow>>;

    async fn daily_per_operator(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<OperatorDailyRow>>;

    async fn daily_per_mandoub(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<MandoubDailyRow>>;

    async fn daily_per_check_type(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<CheckTypeDailyRow>>;

    /// Inventory consumption value in `[from, to)`: SUM(-delta) over
    /// `consume_visit` adjustments (delta is negative for consumption).
    async fn inventory_consumption_value(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<i64>;

    /// `outbox` row count (used as Daily Close `pendingSync` watermark).
    async fn outbox_count(&self) -> AppResult<i64>;

    /// Voided in `[from, to)` (counts + voided value) -- used by Daily Close.
    async fn voided_aggregate(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<VoidedAggregate>;

    /// Stable list of all visit ids contributing to `[from, to)` -- used as
    /// part of the daily-close `input_hash` so the hash is deterministic
    /// across re-runs.
    async fn daily_visit_ids(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<Uuid>>;
}
