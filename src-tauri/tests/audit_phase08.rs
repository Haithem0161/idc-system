//! Integration tests for Phase-8 audit + diagnostics + vacuum.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::audit::domain::repositories::MetricsRepo;
use app_lib::domains::audit::infrastructure::SqliteMetricsRepo;
use app_lib::domains::audit::service::{
    AuditQueryService, AuditVacuumJob, DiagnosticsService, AUDIT_RETENTION_DAYS,
    METRICS_RETENTION_DAYS, SYSTEM_ACTOR_ID, SYSTEM_VACUUM_ENTITY_ID,
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
use chrono::{Duration, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const TENANT: &str = "tenant-phase08";
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
    // Sync state initialized by the migration runner expects an explicit
    // row inserted by `ensure_device_id` in production. Mirror that here so
    // `mark_audit_vacuumed` can update it.
    let repo = SqliteSyncStateRepo::new(pool.clone());
    repo.ensure_device_id("dev-phase08").await.unwrap();
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
        device_id: "dev-phase08".into(),
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

#[tokio::test]
async fn audit_query_filter_by_action_and_entity() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let mut a = make_audit_entry(now, false, "doctors");
    a.action = AuditAction::Lock;
    insert_audit_row(&pool, &a).await;
    let mut b = make_audit_entry(now - Duration::seconds(1), false, "doctors");
    b.action = AuditAction::Create;
    insert_audit_row(&pool, &b).await;
    let mut c = make_audit_entry(now - Duration::seconds(2), false, "visits");
    c.action = AuditAction::Create;
    insert_audit_row(&pool, &c).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            action: Some("create".into()),
            entity: Some("doctors".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].action, "create");
    assert_eq!(page.rows[0].entity, "doctors");
}

#[tokio::test]
async fn audit_query_entity_id_prefix_filter() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let mut a = make_audit_entry(now, false, "patients");
    a.entity_id = "ab123456-0000-0000-0000-000000000000".into();
    insert_audit_row(&pool, &a).await;
    let mut b = make_audit_entry(now - Duration::seconds(1), false, "patients");
    b.entity_id = "fe543210-0000-0000-0000-000000000000".into();
    insert_audit_row(&pool, &b).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            entity_id_prefix: Some("ab12".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].entity_id.starts_with("ab12"));
}

#[tokio::test]
async fn audit_query_free_text_searches_delta_and_entity_id() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let mut a = make_audit_entry(now, false, "visits");
    a.delta = serde_json::json!({"void_reason": "duplicate billing"});
    insert_audit_row(&pool, &a).await;
    let b = make_audit_entry(now - Duration::seconds(1), false, "visits");
    insert_audit_row(&pool, &b).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            free_text: Some("duplicate".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
}

#[tokio::test]
async fn audit_query_orders_at_desc_id_desc() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    for i in 0..3 {
        let e = make_audit_entry(now - Duration::seconds(i), false, "doctors");
        insert_audit_row(&pool, &e).await;
    }
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
    assert!(page.rows[0].at >= page.rows[1].at);
    assert!(page.rows[1].at >= page.rows[2].at);
}

#[tokio::test]
async fn audit_role_gate_denies_non_superadmin() {
    assert!(AuditQueryService::require_audit_role(UserRole::Receptionist).is_err());
    assert!(AuditQueryService::require_audit_role(UserRole::Accountant).is_err());
    assert!(AuditQueryService::require_audit_role(UserRole::Superadmin).is_ok());
}

#[tokio::test]
async fn vacuum_deletes_synced_rows_older_than_90d_and_preserves_dirty() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let cutoff = now - Duration::days(AUDIT_RETENTION_DAYS) - Duration::hours(1);

    let old_synced = make_audit_entry(cutoff, false, "doctors");
    let old_dirty = make_audit_entry(cutoff, true, "doctors");
    let fresh_synced = make_audit_entry(now, false, "doctors");
    insert_audit_row(&pool, &old_synced).await;
    insert_audit_row(&pool, &old_dirty).await;
    insert_audit_row(&pool, &fresh_synced).await;

    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let job = AuditVacuumJob::new(
        pool.clone(),
        audit_repo.clone(),
        metrics_repo,
        outbox_repo.clone(),
        state_repo.clone(),
        "dev-phase08".into(),
    );

    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 1);

    // Remaining: old_dirty + fresh_synced + vacuum self-audit = 3 rows.
    let rows = audit_repo.list_by_tenant(TENANT, 100, 0).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .any(|r| matches!(r.action, AuditAction::Vacuum) && r.entity == "audit_log"));
    let vacuum_row = rows
        .iter()
        .find(|r| matches!(r.action, AuditAction::Vacuum))
        .unwrap();
    assert_eq!(vacuum_row.entity_id, SYSTEM_VACUUM_ENTITY_ID);
    assert_eq!(
        vacuum_row.actor_user_id.to_string(),
        SYSTEM_ACTOR_ID.to_string()
    );

    // Outbox now contains the vacuum row's push entry.
    assert!(outbox_repo.pending_count().await.unwrap() >= 1);

    // sync_state.last_audit_vacuum_at stamped.
    let state = state_repo.get().await.unwrap();
    assert!(state.last_audit_vacuum_at.is_some());
}

#[tokio::test]
async fn vacuum_also_purges_metrics_events_older_than_30d() {
    let pool = fresh_pool().await;
    let now = Utc::now();

    let old_at = now - Duration::days(METRICS_RETENTION_DAYS) - Duration::hours(1);
    let recent_at = now - Duration::hours(1);
    sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind("sync_push_ok")
    .bind(old_at.to_rfc3339())
    .bind("{}")
    .bind(TENANT)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind("sync_pull_ok")
    .bind(recent_at.to_rfc3339())
    .bind("{}")
    .bind(TENANT)
    .execute(&pool)
    .await
    .unwrap();

    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let job = AuditVacuumJob::new(
        pool.clone(),
        audit_repo,
        metrics_repo,
        outbox_repo,
        state_repo,
        "dev-phase08".into(),
    );

    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.metrics_purged, 1);

    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM metrics_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn diagnostics_summary_assembles_counters() {
    let pool = fresh_pool().await;
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let now = Utc::now();
    // 6 lock pairs spanning ~10..60ms
    for i in 0..6 {
        let visit_id = Uuid::now_v7().to_string();
        let start = now - Duration::minutes(i + 1);
        sqlx::query("INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)")
            .bind(Uuid::now_v7().to_string())
            .bind("lock_start")
            .bind(start.to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{}\"}}", visit_id))
            .bind(TENANT)
            .execute(&pool)
            .await
            .unwrap();
        let end = start + Duration::milliseconds(10 + i * 10);
        sqlx::query("INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)")
            .bind(Uuid::now_v7().to_string())
            .bind("lock_end")
            .bind(end.to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{}\"}}", visit_id))
            .bind(TENANT)
            .execute(&pool)
            .await
            .unwrap();
    }
    // Receipt prints: 4 ok, 1 fail.
    for _ in 0..4 {
        sqlx::query("INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)")
            .bind(Uuid::now_v7().to_string())
            .bind("receipt_print_ok")
            .bind(now.to_rfc3339())
            .bind("{}")
            .bind(TENANT)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind("receipt_print_fail")
    .bind(now.to_rfc3339())
    .bind("{}")
    .bind(TENANT)
    .execute(&pool)
    .await
    .unwrap();
    // Sync conflict.
    sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind("sync_conflict")
    .bind(now.to_rfc3339())
    .bind("{}")
    .bind(TENANT)
    .execute(&pool)
    .await
    .unwrap();

    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert!(s.lock_latency_p95_ms.is_some());
    assert_eq!(s.conflict_count_7d, 1);
    let rate = s.receipt_print_success_rate_30d.expect("rate present");
    assert!((rate - 0.8).abs() < 1e-6);
}
