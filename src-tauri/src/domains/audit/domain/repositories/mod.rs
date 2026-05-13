//! Audit domain repository ports.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::AppResult;

/// Reads `metrics_events` for diagnostics and prunes 30-day-old rows.
/// Phase-08 §7.21 extends `AuditVacuumJob` to call `vacuum_older_than`.
#[async_trait]
pub trait MetricsRepo: Send + Sync {
    /// Hard delete (metrics are local-only, non-syncable per phase-01 §7.28).
    async fn vacuum_older_than(&self, cutoff: DateTime<Utc>) -> AppResult<u64>;

    /// `lock_end - lock_start` p95 across rows in the window. Returns `None`
    /// when fewer than 5 paired samples exist (insufficient signal).
    async fn lock_latency_p95_ms(
        &self,
        entity_id_tenant: &str,
        window: chrono::Duration,
    ) -> AppResult<Option<i64>>;

    /// `receipt_print_ok / (ok + fail)` rounded to 4 decimals. `None` when no
    /// receipts printed in the window.
    async fn receipt_print_success_rate(
        &self,
        entity_id_tenant: &str,
        window: chrono::Duration,
    ) -> AppResult<Option<f64>>;

    /// Total `sync_conflict` rows in the window.
    async fn conflict_count(
        &self,
        entity_id_tenant: &str,
        window: chrono::Duration,
    ) -> AppResult<u32>;
}
