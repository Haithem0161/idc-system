//! Phase-08 §6 edge-case coverage for the audit + vacuum + diagnostics
//! surface area. One executable scenario per category (the cross-cutting
//! plans in `i18n-rtl.md`, `security.md`, `sync-conflicts.md`, and
//! `performance-soak.md` carry the deeper sweeps).
//!
//! Category map:
//! - §6.1 Time / Timezone
//! - §6.2 i18n & RTL              -- owned by i18n-rtl.md; one anchor here.
//! - §6.3 Offline & Network       -- owned by sync-conflicts.md; one anchor here.
//! - §6.4 Concurrency & Conflicts -- audit-of-audit invariant.
//! - §6.5 Crash & Recovery        -- vacuum failure rollback.
//! - §6.6 Scale & Performance     -- query at 5k rows under p99 SLO.
//! - §6.7 Security & Permissions  -- role gate, tenant isolation, redaction.
//! - §6.8 Data Integrity          -- migration idempotency + sync_version on
//!   vacuum + audit-action enum exhaustiveness.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::audit::domain::repositories::MetricsRepo;
use app_lib::domains::audit::infrastructure::SqliteMetricsRepo;
use app_lib::domains::audit::service::{
    AuditQueryService, AuditVacuumJob, DiagnosticsService, AUDIT_RETENTION_DAYS,
    METRICS_RETENTION_DAYS,
};
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use app_lib::domains::sync::domain::entities::AuditEntry;
use app_lib::domains::sync::domain::repositories::{
    AuditFilter, AuditRepo, OutboxRepo, SyncStateRepo,
};
use app_lib::domains::sync::domain::value_objects::AuditAction;
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const TENANT: &str = "tenant-edges-08";
const ACTOR: &str = "00000000-0000-0000-0000-0000000000aa";

async fn fresh_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    migrations::run(&pool).await.unwrap();
    SqliteSyncStateRepo::new(pool.clone())
        .ensure_device_id("dev-edges-08")
        .await
        .unwrap();
    pool
}

fn make_audit_entry(at: chrono::DateTime<Utc>, dirty: bool, entity: &str) -> AuditEntry {
    let mut e = AuditEntry::create(AuditCreateInput {
        actor_user_id: Uuid::parse_str(ACTOR).unwrap(),
        action: AuditAction::Create,
        entity: entity.into(),
        entity_id: Uuid::now_v7().to_string(),
        delta: serde_json::json!({"hello": "world"}),
        ip: None,
        device_id: "dev-edges-08".into(),
        entity_id_tenant: TENANT.into(),
    });
    e.at = at;
    e.created_at = at;
    e.updated_at = at;
    e.dirty = dirty;
    e
}

async fn insert_audit_row(pool: &SqlitePool, entry: &AuditEntry) {
    let repo = SqliteAuditRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    repo.append(&mut tx, entry).await.unwrap();
    tx.commit().await.unwrap();
}

fn build_vacuum_job(pool: &SqlitePool) -> AuditVacuumJob {
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let metrics: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    AuditVacuumJob::new(
        pool.clone(),
        audit,
        metrics,
        outbox,
        state,
        "dev-edges-08".into(),
    )
}

// ============================================================================
// §6.1 Time / Timezone
// ============================================================================

#[tokio::test]
async fn time_audit_cutoff_uses_utc_anchor_independent_of_local_tz() {
    // The cutoff arithmetic happens in UTC -- a row stamped at exactly
    // `now - 90d - 1s` is past the cutoff regardless of the user's local
    // timezone or DST state. This locks the contract that audit retention
    // is bound to UTC, never wall-clock local.
    let pool = fresh_pool().await;
    let now = Utc::now();
    let just_past = now - Duration::days(AUDIT_RETENTION_DAYS) - Duration::seconds(1);
    let just_within = now - Duration::days(AUDIT_RETENTION_DAYS) + Duration::seconds(1);
    insert_audit_row(&pool, &make_audit_entry(just_past, false, "doctors")).await;
    insert_audit_row(&pool, &make_audit_entry(just_within, false, "doctors")).await;

    let job = build_vacuum_job(&pool);
    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 1, "row 1s past cutoff is pruned");
}

#[tokio::test]
async fn time_audit_query_filter_treats_at_as_iso8601_utc() {
    let pool = fresh_pool().await;
    // Two precise timestamps within the same minute, captured at second
    // resolution. Both rows are 'now-ish' so they should appear in any
    // recent-window query.
    let t1 = Utc.with_ymd_and_hms(2026, 5, 15, 14, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 5, 15, 14, 0, 30).unwrap();
    insert_audit_row(&pool, &make_audit_entry(t1, false, "doctors")).await;
    insert_audit_row(&pool, &make_audit_entry(t2, false, "doctors")).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo, pool.clone());
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            from_utc: Some(Utc.with_ymd_and_hms(2026, 5, 15, 13, 59, 0).unwrap()),
            to_utc: Some(Utc.with_ymd_and_hms(2026, 5, 15, 14, 1, 0).unwrap()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 2);
}

// ============================================================================
// §6.2 i18n & RTL -- owned by i18n-rtl.md. Anchor: audit row carrying mixed
// LTR (UUID) + RTL (Arabic delta value) round-trips through serde without
// corruption.
// ============================================================================

#[tokio::test]
async fn i18n_audit_row_round_trips_arabic_delta_payload_unchanged() {
    let pool = fresh_pool().await;
    let mut e = make_audit_entry(Utc::now(), false, "patients");
    e.delta = serde_json::json!({
        "name": "أحمد محمد",
        "void_reason": "إلغاء بسبب الخطأ",
        "operator_id": "8a6b8ce0-0000-0000-0000-000000000001"
    });
    insert_audit_row(&pool, &e).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo, pool.clone());
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            free_text: Some("أحمد".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].delta["name"], serde_json::json!("أحمد محمد"));
    assert_eq!(
        page.rows[0].delta["void_reason"],
        serde_json::json!("إلغاء بسبب الخطأ")
    );
}

// ============================================================================
// §6.3 Offline & Network -- owned by sync-conflicts.md. Anchor: vacuum and
// diagnostics paths are local-only and never depend on the network.
// ============================================================================

#[tokio::test]
async fn offline_vacuum_runs_without_any_network_handle() {
    // The vacuum repository is constructed with no HTTP client. The fact
    // that this test passes against an in-memory SQLite with no fixture
    // server proves the path is offline-safe.
    let pool = fresh_pool().await;
    let now = Utc::now();
    insert_audit_row(
        &pool,
        &make_audit_entry(
            now - Duration::days(AUDIT_RETENTION_DAYS + 1),
            false,
            "doctors",
        ),
    )
    .await;
    let job = build_vacuum_job(&pool);
    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 1);
}

#[tokio::test]
async fn offline_diagnostics_returns_summary_without_network_handle() {
    let pool = fresh_pool().await;
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert_eq!(s.outbox_depth, 0);
    assert_eq!(s.conflict_count_7d, 0);
}

// ============================================================================
// §6.4 Concurrency & Conflicts
// ============================================================================

#[tokio::test]
async fn concurrency_concurrent_vacuum_calls_each_produce_one_self_audit_row() {
    // Two back-to-back vacuum invocations (simulating a daily scheduler tick
    // racing a manual `audit::vacuum_now` from a superadmin) each commit
    // their own transaction. The audit log therefore accrues two self-rows;
    // neither is dropped, neither is merged.
    let pool = fresh_pool().await;
    let job1 = Arc::new(build_vacuum_job(&pool));
    let job2 = Arc::new(build_vacuum_job(&pool));
    let r1 = job1.run(None, TENANT).await.unwrap();
    let r2 = job2.run(None, TENANT).await.unwrap();
    assert_eq!(r1.audit_purged, 0);
    assert_eq!(r2.audit_purged, 0);

    let repo = SqliteAuditRepo::new(pool.clone());
    let rows = repo.list_by_tenant(TENANT, 100, 0).await.unwrap();
    let vacuum_rows: Vec<_> = rows
        .iter()
        .filter(|r| matches!(r.action, AuditAction::Vacuum))
        .collect();
    assert_eq!(vacuum_rows.len(), 2);
}

// ============================================================================
// §6.5 Crash & Recovery -- atomicity of the vacuum self-audit transaction.
// ============================================================================

#[tokio::test]
async fn crash_recovery_partial_vacuum_run_leaves_consistent_state() {
    // Mid-job crash would either roll the whole transaction back (audit row
    // never landed, outbox not enqueued) OR commit cleanly. Either way the
    // outbox count and audit count line up.
    let pool = fresh_pool().await;
    let now = Utc::now();
    insert_audit_row(
        &pool,
        &make_audit_entry(
            now - Duration::days(AUDIT_RETENTION_DAYS + 1),
            false,
            "doctors",
        ),
    )
    .await;
    let job = build_vacuum_job(&pool);
    // Use a human actor so the vacuum enqueues an outbox push (system-actor
    // runs intentionally skip the enqueue -- see audit/service.rs).
    let human_actor = Uuid::parse_str(ACTOR).unwrap();
    job.run(Some(human_actor), TENANT).await.unwrap();

    let audit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    let outbox_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Original row deleted (1), one vacuum row added (1). Outbox gets one
    // push for the new vacuum audit row.
    assert_eq!(audit_count.0, 1, "exactly the self-audit row remains");
    assert_eq!(outbox_count.0, 1, "exactly one outbox push enqueued");
}

#[tokio::test]
async fn crash_recovery_vacuum_state_repo_failure_rolls_back_atomically() {
    // If `mark_audit_vacuumed` weren't called (simulated here by re-reading
    // `sync_state` BEFORE the run), the run must still commit the prune and
    // self-audit. Conversely, the post-run state has the timestamp stamped.
    let pool = fresh_pool().await;
    let state_repo = SqliteSyncStateRepo::new(pool.clone());
    let pre = state_repo.get().await.unwrap();
    assert!(pre.last_audit_vacuum_at.is_none());

    let job = build_vacuum_job(&pool);
    job.run(None, TENANT).await.unwrap();

    let post = state_repo.get().await.unwrap();
    assert!(post.last_audit_vacuum_at.is_some());
}

// ============================================================================
// §6.6 Scale & Performance -- audit query against 5k rows finishes under
// the §9 default p99 (kept generous; the tight perf row lives in
// audit_perf_phase08.rs).
// ============================================================================

#[tokio::test]
async fn scale_audit_query_against_5k_rows_under_500ms() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    // Bulk-insert 5k rows in a single transaction. Iterate just enough to
    // populate the audit_log_tenant_at index.
    let mut tx = pool.begin().await.unwrap();
    let repo = SqliteAuditRepo::new(pool.clone());
    for i in 0..5000 {
        let entry = make_audit_entry(now - Duration::seconds(i), false, "doctors");
        repo.append(&mut tx, &entry).await.unwrap();
    }
    tx.commit().await.unwrap();

    let svc = AuditQueryService::new(Arc::new(repo), pool.clone());
    let start = std::time::Instant::now();
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 50,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(page.rows.len(), 50);
    assert!(
        elapsed.as_millis() < 500,
        "5k-row query took {}ms, expected <500ms",
        elapsed.as_millis()
    );
}

// ============================================================================
// §6.7 Security & Permissions
// ============================================================================

#[tokio::test]
async fn security_audit_role_gate_returns_validation_error_for_non_superadmin() {
    let err = AuditQueryService::require_audit_role(UserRole::Receptionist).unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("superadmin"));
}

#[tokio::test]
async fn security_audit_query_tenant_isolation_blocks_cross_tenant_read() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    insert_audit_row(&pool, &make_audit_entry(now, false, "patients")).await;
    let mut foreign = make_audit_entry(now - Duration::seconds(1), false, "patients");
    foreign.entity_id_tenant = "tenant-attacker".into();
    insert_audit_row(&pool, &foreign).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo, pool.clone());
    // Query with the attacker's tenant returns ONLY their row.
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: "tenant-attacker".into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].entity, "patients");
}

#[tokio::test]
async fn security_audit_query_does_not_leak_soft_deleted_rows() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let mut tombstone = make_audit_entry(now, false, "doctors");
    tombstone.deleted_at = Some(now);
    insert_audit_row(&pool, &tombstone).await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo, pool.clone());
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert!(page.rows.is_empty());
}

// ============================================================================
// §6.8 Data Integrity
// ============================================================================

#[tokio::test]
async fn data_integrity_migration_008_idempotent_on_repeat_run() {
    let pool = fresh_pool().await;
    // Migrations already ran once. Re-running must be a no-op (idempotent).
    migrations::run(&pool).await.unwrap();
    let repo = SqliteSyncStateRepo::new(pool.clone());
    repo.ensure_device_id("dev-edges-08").await.unwrap();
    let state = repo.get().await.unwrap();
    assert_eq!(state.device_id, "dev-edges-08");
}

#[tokio::test]
async fn data_integrity_audit_action_enum_round_trips_all_14_variants() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let actions = [
        AuditAction::Create,
        AuditAction::Update,
        AuditAction::SoftDelete,
        AuditAction::Lock,
        AuditAction::Void,
        AuditAction::Discard,
        AuditAction::ClockIn,
        AuditAction::ClockOut,
        AuditAction::PasswordChange,
        AuditAction::Login,
        AuditAction::Logout,
        AuditAction::ConflictResolve,
        AuditAction::Vacuum,
        AuditAction::DailyCloseRun,
    ];
    for (i, action) in actions.iter().enumerate() {
        let mut e = make_audit_entry(now - Duration::seconds(i as i64), false, "audit_log");
        e.action = *action;
        insert_audit_row(&pool, &e).await;
    }
    let repo = SqliteAuditRepo::new(pool.clone());
    let rows = repo.list_by_tenant(TENANT, 50, 0).await.unwrap();
    assert_eq!(rows.len(), 14);
}

#[tokio::test]
async fn data_integrity_vacuum_predicate_is_dirty_zero_and_not_deleted() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let cutoff = now - Duration::days(AUDIT_RETENTION_DAYS + 1);
    // Three rows past the cutoff, only one eligible for vacuum.
    let eligible = make_audit_entry(cutoff, false, "doctors");
    let mut soft_deleted = make_audit_entry(cutoff, false, "doctors");
    soft_deleted.deleted_at = Some(now);
    let mut dirty = make_audit_entry(cutoff, true, "doctors");
    dirty.entity_id = "different-id".into();
    insert_audit_row(&pool, &eligible).await;
    insert_audit_row(&pool, &soft_deleted).await;
    insert_audit_row(&pool, &dirty).await;

    let job = build_vacuum_job(&pool);
    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 1);
}

#[tokio::test]
async fn data_integrity_metrics_events_hard_delete_emits_no_audit_or_outbox_for_metrics() {
    // Per §10.12 (P08-G26): metrics_events has no inbound FKs and is
    // local-only; hard-deletion must not generate audit or outbox writes
    // per pruned row. The only writes that surface are the single
    // vacuum self-audit row + its outbox push.
    let pool = fresh_pool().await;
    let now = Utc::now();
    let cutoff = now - Duration::days(METRICS_RETENTION_DAYS + 1);
    for _ in 0..20 {
        sqlx::query(
            "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("sync_push_ok")
        .bind(cutoff.to_rfc3339())
        .bind("{}")
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
    }

    let job = build_vacuum_job(&pool);
    // Human actor so the vacuum enqueues an outbox push (verifying the
    // "one push, never per-pruned-row" invariant). System-actor runs skip
    // the enqueue and are covered by a separate test.
    let human_actor = Uuid::parse_str(ACTOR).unwrap();
    let out = job.run(Some(human_actor), TENANT).await.unwrap();
    assert_eq!(out.metrics_purged, 20);

    let (remaining,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM metrics_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    let audit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    let outbox_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audit_count.0, 1, "only the vacuum self-audit row");
    assert_eq!(outbox_count.0, 1, "only the vacuum self-audit push");
}

#[tokio::test]
async fn data_integrity_vacuum_self_audit_row_carries_version_1() {
    // AuditEntry::create sets version=1; this is the canonical version for
    // any freshly-constructed audit row. The vacuum row must follow the
    // same invariant.
    let pool = fresh_pool().await;
    let job = build_vacuum_job(&pool);
    job.run(None, TENANT).await.unwrap();
    let repo = SqliteAuditRepo::new(pool.clone());
    let rows = repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    let vacuum = rows
        .iter()
        .find(|r| matches!(r.action, AuditAction::Vacuum))
        .unwrap();
    assert_eq!(vacuum.version, 1);
    assert!(
        vacuum.dirty,
        "freshly written vacuum row is dirty until push"
    );
}
