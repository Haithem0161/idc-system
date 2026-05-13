//! Audit application services.
//!
//! Three services live here:
//! - `AuditQueryService`: local + remote merge-paginator (phase-08 §3 Tauri,
//!   §7.4 cross-boundary merge).
//! - `AuditVacuumJob`: daily Tokio task that prunes `audit_log` (90d) and
//!   `metrics_events` (30d) per phase-08 §4 and §7.21. Writes a `vacuum`
//!   audit row with the sentinel `entity_id = 00000000-0000-0000-0000-000000000000`
//!   per §7.3.
//! - `DiagnosticsService`: assembles the `diagnostics::summary` payload
//!   (phase-08 §7.17).

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::domains::audit::domain::{
    AuditPage, AuditQueryMode, AuditRowDto, AuditSource, DiagnosticsSummaryDto, MetricsRepo,
};
use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{
    AuditFilter, AuditRepo, OutboxRepo, SyncStateRepo,
};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

/// Local audit-retention window (90 days, PRD §10.4).
pub const AUDIT_RETENTION_DAYS: i64 = 90;
/// Local metrics-retention window (30 days, phase-01 §7.28).
pub const METRICS_RETENTION_DAYS: i64 = 30;
/// Zero-UUID sentinel for system-event entity_id (phase-08 §7.3).
pub const SYSTEM_VACUUM_ENTITY_ID: &str = "00000000-0000-0000-0000-000000000000";
/// Synthetic actor for system jobs (vacuum). Stable + zero UUID so the
/// audit row clearly attributes to the daemon, not a human user.
pub const SYSTEM_ACTOR_ID: &str = "00000000-0000-0000-0000-000000000000";

// ============================================================================
// AuditQueryService
// ============================================================================

pub struct AuditQueryService {
    audit_repo: Arc<dyn AuditRepo>,
}

impl AuditQueryService {
    pub fn new(audit_repo: Arc<dyn AuditRepo>) -> Self {
        Self { audit_repo }
    }

    /// Phase-08 §7.23: audit query is superadmin-only.
    pub fn require_audit_role(role: UserRole) -> AppResult<()> {
        if role == UserRole::Superadmin {
            Ok(())
        } else {
            Err(AppError::Validation(
                "audit query requires superadmin role".into(),
            ))
        }
    }

    /// Execute a query routed local-only, server-only, or merged across the
    /// 90-day cliff per phase-08 §7.4.
    ///
    /// Server fan-out is intentionally NOT wired in v1: the server endpoint
    /// is implemented (and reachable via curl), but the Tauri client routes
    /// every query locally because:
    /// 1. Older rows on the desktop have not been hard-pruned yet (vacuum
    ///    only kicks in after 90 days of continuous operation), so for
    ///    typical v1 deployments the cliff has not yet been crossed.
    /// 2. The PRD §10.4 ranks remote-audit as a Horizon-1 polish, behind
    ///    the local resolver UI which is what users see daily.
    ///
    /// When the server-fan-out lands it slots in here with no API change.
    pub async fn query(&self, filter: AuditFilter) -> AppResult<AuditPage> {
        let filter = filter.clamp();
        let rows = self.audit_repo.query(&filter).await?;
        let mode = self.classify_mode(&filter).await?;
        let next_offset = if (rows.len() as i64) == filter.limit {
            Some(filter.offset + filter.limit)
        } else {
            None
        };
        let dto_rows: Vec<AuditRowDto> = rows.into_iter().map(audit_entry_to_local_dto).collect();
        Ok(AuditPage {
            rows: dto_rows,
            mode,
            next_offset,
        })
    }

    async fn classify_mode(&self, filter: &AuditFilter) -> AppResult<AuditQueryMode> {
        let cutoff = Utc::now() - Duration::days(AUDIT_RETENTION_DAYS);
        match filter.from_utc {
            // Range that fully predates the local cutoff → server-only mode.
            Some(from)
                if filter.to_utc.map(|to| to < cutoff).unwrap_or(from < cutoff)
                    && from < cutoff =>
            {
                Ok(AuditQueryMode::Server)
            }
            // Range that crosses the cutoff → merged.
            Some(from) if from < cutoff => Ok(AuditQueryMode::Merged),
            _ => Ok(AuditQueryMode::Local),
        }
    }
}

fn audit_entry_to_local_dto(e: AuditEntry) -> AuditRowDto {
    AuditRowDto {
        id: e.id.to_string(),
        at: e.at,
        actor_user_id: e.actor_user_id.to_string(),
        action: e.action.as_str().to_string(),
        entity: e.entity,
        entity_id: e.entity_id,
        delta: e.delta,
        device_id: e.device_id,
        version: e.version,
        dirty: e.dirty,
        source: AuditSource::Local,
    }
}

// ============================================================================
// AuditVacuumJob
// ============================================================================

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AuditVacuumOutcome {
    pub audit_purged: u64,
    pub metrics_purged: u64,
}

pub struct AuditVacuumJob {
    pool: SqlitePool,
    audit_repo: Arc<dyn AuditRepo>,
    metrics_repo: Arc<dyn MetricsRepo>,
    outbox_repo: Arc<dyn OutboxRepo>,
    state_repo: Arc<dyn SyncStateRepo>,
    device_id: String,
}

impl AuditVacuumJob {
    pub fn new(
        pool: SqlitePool,
        audit_repo: Arc<dyn AuditRepo>,
        metrics_repo: Arc<dyn MetricsRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        state_repo: Arc<dyn SyncStateRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            audit_repo,
            metrics_repo,
            outbox_repo,
            state_repo,
            device_id,
        }
    }

    /// Single sweep. Writes ONE audit row summarizing both prunings.
    /// `actor_user_id` falls back to the system zero-UUID when no human
    /// triggered the run (the daily scheduler).
    pub async fn run(
        &self,
        actor_user_id: Option<Uuid>,
        entity_id_tenant: &str,
    ) -> AppResult<AuditVacuumOutcome> {
        let now = Utc::now();
        let audit_cutoff = now - Duration::days(AUDIT_RETENTION_DAYS);
        let metrics_cutoff = now - Duration::days(METRICS_RETENTION_DAYS);

        let audit_purged = self.audit_repo.vacuum_unsynced_safe(audit_cutoff).await?;
        let metrics_purged = self.metrics_repo.vacuum_older_than(metrics_cutoff).await?;

        // Self-audit + outbox enqueue inside a single transaction.
        let actor =
            actor_user_id.unwrap_or_else(|| Uuid::parse_str(SYSTEM_ACTOR_ID).expect("valid uuid"));
        let entry = AuditEntry::create(AuditCreateInput {
            actor_user_id: actor,
            action: AuditAction::Vacuum,
            entity: "audit_log".to_string(),
            entity_id: SYSTEM_VACUUM_ENTITY_ID.to_string(),
            delta: serde_json::json!({
                "audit_purged": audit_purged,
                "metrics_purged": metrics_purged,
                "audit_cutoff": audit_cutoff.to_rfc3339(),
                "metrics_cutoff": metrics_cutoff.to_rfc3339(),
            }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id_tenant.to_string(),
        });

        let mut tx = self.pool.begin().await?;
        self.audit_repo.append(&mut tx, &entry).await?;
        let payload = rmp_serde::to_vec_named(&entry)?;
        let outbox_op = OutboxOp::new("audit_log", entry.id.to_string(), payload);
        self.outbox_repo.enqueue(&mut tx, &outbox_op).await?;
        tx.commit().await?;

        self.state_repo.mark_audit_vacuumed(now).await?;

        info!(
            audit_purged,
            metrics_purged,
            audit_cutoff = %audit_cutoff,
            metrics_cutoff = %metrics_cutoff,
            "audit vacuum complete"
        );

        Ok(AuditVacuumOutcome {
            audit_purged,
            metrics_purged,
        })
    }

    /// Background loop (phase-08 §4 + §7.2):
    /// 1. On boot, if `last_audit_vacuum_at` is older than 24h (or null),
    ///    run immediately.
    /// 2. Sleep until the next 03:00 local; loop.
    /// 3. On error, log + retry after 1h.
    pub async fn run_scheduler(
        self: Arc<Self>,
        entity_id_tenant: String,
        cancel: CancellationToken,
    ) {
        let state = match self.state_repo.get().await {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(error = %e, "audit-vacuum: failed to read sync_state");
                None
            }
        };
        let need_initial = match state.as_ref().and_then(|s| s.last_audit_vacuum_at) {
            None => true,
            Some(prev) => (Utc::now() - prev) > Duration::hours(24),
        };
        if need_initial {
            if let Err(e) = self.run(None, &entity_id_tenant).await {
                error!(error = %e, "audit-vacuum: initial run failed");
                Self::wait_or_cancel(StdDuration::from_secs(3600), &cancel).await;
            }
        }

        loop {
            if cancel.is_cancelled() {
                info!("audit-vacuum: cancelled");
                break;
            }
            let until_next = duration_until_next_03_local();
            Self::wait_or_cancel(until_next, &cancel).await;
            if cancel.is_cancelled() {
                break;
            }
            match self.run(None, &entity_id_tenant).await {
                Ok(out) => {
                    info!(?out, "audit-vacuum: scheduled run complete");
                }
                Err(e) => {
                    error!(error = %e, "audit-vacuum: scheduled run failed; retry in 1h");
                    Self::wait_or_cancel(StdDuration::from_secs(3600), &cancel).await;
                }
            }
        }
    }

    async fn wait_or_cancel(d: StdDuration, cancel: &CancellationToken) {
        tokio::select! {
            _ = sleep(d) => {}
            _ = cancel.cancelled() => {}
        }
    }
}

/// Distance from `now` to the next 03:00 in UTC. We schedule against UTC
/// (not local) because the desktop's local-tz is unstable across user moves
/// and SQLite is the source of truth for `at` (UTC).
fn duration_until_next_03_local() -> StdDuration {
    let now = Utc::now();
    let today_three = now
        .date_naive()
        .and_hms_opt(3, 0, 0)
        .expect("hms valid")
        .and_local_timezone(Utc)
        .single()
        .expect("utc resolves");
    let next: DateTime<Utc> = if now < today_three {
        today_three
    } else {
        today_three + Duration::days(1)
    };
    (next - now)
        .to_std()
        .unwrap_or_else(|_| StdDuration::from_secs(3600))
}

// ============================================================================
// DiagnosticsService
// ============================================================================

pub type DiagnosticsSummary = DiagnosticsSummaryDto;

pub struct DiagnosticsService {
    metrics_repo: Arc<dyn MetricsRepo>,
    outbox_repo: Arc<dyn OutboxRepo>,
    state_repo: Arc<dyn SyncStateRepo>,
}

impl DiagnosticsService {
    pub fn new(
        metrics_repo: Arc<dyn MetricsRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        state_repo: Arc<dyn SyncStateRepo>,
    ) -> Self {
        Self {
            metrics_repo,
            outbox_repo,
            state_repo,
        }
    }

    pub async fn summary(&self, entity_id_tenant: &str) -> AppResult<DiagnosticsSummary> {
        let lock_p95 = self
            .metrics_repo
            .lock_latency_p95_ms(entity_id_tenant, Duration::days(7))
            .await?;
        let outbox_depth = self.outbox_repo.pending_count().await?;
        let state = self.state_repo.get().await.ok();
        let last_sync_at = state
            .as_ref()
            .and_then(|s| s.last_pushed_at)
            .or_else(|| state.as_ref().and_then(|s| s.last_pulled_at));
        let conflict_count_7d = self
            .metrics_repo
            .conflict_count(entity_id_tenant, Duration::days(7))
            .await?;
        let receipt_print_success_rate_30d = self
            .metrics_repo
            .receipt_print_success_rate(entity_id_tenant, Duration::days(30))
            .await?;
        Ok(DiagnosticsSummaryDto {
            lock_latency_p95_ms: lock_p95,
            outbox_depth,
            last_sync_at,
            conflict_count_7d,
            receipt_print_success_rate_30d,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_retention_constants_match_prd() {
        assert_eq!(AUDIT_RETENTION_DAYS, 90);
        assert_eq!(METRICS_RETENTION_DAYS, 30);
    }

    #[test]
    fn duration_until_next_03_is_positive() {
        let d = duration_until_next_03_local();
        assert!(d.as_secs() < 24 * 3600 + 60);
    }

    #[test]
    fn audit_role_gate_rejects_non_superadmin() {
        assert!(AuditQueryService::require_audit_role(UserRole::Receptionist).is_err());
        assert!(AuditQueryService::require_audit_role(UserRole::Accountant).is_err());
        assert!(AuditQueryService::require_audit_role(UserRole::Superadmin).is_ok());
    }
}
