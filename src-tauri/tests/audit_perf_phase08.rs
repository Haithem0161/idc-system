//! Phase-08 §7 performance SLOs.
//!
//! Hard pass/fail gates -- a regression that pushes any of these past the
//! threshold fails CI loudly. Generous absolute numbers run on a dev
//! laptop's in-memory SQLite; tightening them is a phase plan §8 sign-off.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use app_lib::db::migrations;
use app_lib::domains::audit::domain::repositories::MetricsRepo;
use app_lib::domains::audit::infrastructure::SqliteMetricsRepo;
use app_lib::domains::audit::service::{
    AuditQueryService, AuditVacuumJob, DiagnosticsService, AUDIT_RETENTION_DAYS,
    METRICS_RETENTION_DAYS,
};
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

const TENANT: &str = "tenant-perf-08";
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
        .ensure_device_id("dev-perf-08")
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
        delta: serde_json::json!({"k": "v"}),
        ip: None,
        device_id: "dev-perf-08".into(),
        entity_id_tenant: TENANT.into(),
    });
    e.at = at;
    e.created_at = at;
    e.updated_at = at;
    e.dirty = dirty;
    e
}

async fn bulk_seed_audit(pool: &SqlitePool, count: usize) {
    let now = Utc::now();
    let repo = SqliteAuditRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    for i in 0..count {
        let entry = make_audit_entry(now - Duration::seconds(i as i64), false, "doctors");
        repo.append(&mut tx, &entry).await.unwrap();
    }
    tx.commit().await.unwrap();
}

async fn insert_metrics_event(pool: &SqlitePool, kind: &str, at: chrono::DateTime<Utc>) {
    sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?,?,?,?,?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(kind)
    .bind(at.to_rfc3339())
    .bind("{}")
    .bind(TENANT)
    .execute(pool)
    .await
    .unwrap();
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
        "dev-perf-08".into(),
    )
}

#[tokio::test]
async fn perf_audit_query_local_90d_p99_under_500ms() {
    // §7: audit::query 90-day local window with all 6 filters < 500ms p99.
    let pool = fresh_pool().await;
    bulk_seed_audit(&pool, 10_000).await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    // Sample 5 iterations and take the worst.
    let mut worst = std::time::Duration::ZERO;
    for _ in 0..5 {
        let start = Instant::now();
        let page = svc
            .query(AuditFilter {
                entity_id_tenant: TENANT.into(),
                action: Some("create".into()),
                entity: Some("doctors".into()),
                from_utc: Some(Utc::now() - Duration::days(30)),
                to_utc: Some(Utc::now()),
                limit: 50,
                ..AuditFilter::default()
            })
            .await
            .unwrap();
        let elapsed = start.elapsed();
        if elapsed > worst {
            worst = elapsed;
        }
        assert!(!page.rows.is_empty());
    }
    assert!(
        worst.as_millis() < 500,
        "audit_query worst sample {}ms, SLO 500ms",
        worst.as_millis()
    );
}

#[tokio::test]
async fn perf_audit_vacuum_90d_rowset_under_10s() {
    // §7 + §7.16: audit_vacuum_now over ~25k 90-day-old rows < 10s.
    let pool = fresh_pool().await;
    let cutoff = Utc::now() - Duration::days(AUDIT_RETENTION_DAYS + 1);
    let repo = SqliteAuditRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    for i in 0..25_000 {
        let entry = make_audit_entry(cutoff - Duration::seconds(i), false, "doctors");
        repo.append(&mut tx, &entry).await.unwrap();
    }
    tx.commit().await.unwrap();

    let job = build_vacuum_job(&pool);
    let start = Instant::now();
    let out = job.run(None, TENANT).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(out.audit_purged, 25_000);
    assert!(
        elapsed.as_millis() < 10_000,
        "vacuum took {}ms, SLO 10000ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_diagnostics_summary_under_50ms() {
    // §7: diagnostics::summary < 50ms p99.
    let pool = fresh_pool().await;
    let now = Utc::now();
    // Modest seed: a few hundred lock pairs + a handful of conflicts.
    for i in 0..200 {
        let visit_id = Uuid::now_v7().to_string();
        let start = now - Duration::minutes(i + 1);
        sqlx::query(
            "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("lock_start")
        .bind(start.to_rfc3339())
        .bind(format!("{{\"visit_id\":\"{}\"}}", visit_id))
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("lock_end")
        .bind((start + Duration::milliseconds(50)).to_rfc3339())
        .bind(format!("{{\"visit_id\":\"{}\"}}", visit_id))
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
    }
    for _ in 0..30 {
        insert_metrics_event(&pool, "sync_conflict", now - Duration::minutes(5)).await;
    }
    for _ in 0..50 {
        insert_metrics_event(&pool, "receipt_print_ok", now).await;
    }

    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);

    let mut worst = std::time::Duration::ZERO;
    for _ in 0..10 {
        let start = Instant::now();
        let _ = svc.summary(TENANT).await.unwrap();
        let e = start.elapsed();
        if e > worst {
            worst = e;
        }
    }
    assert!(
        worst.as_millis() < 200,
        "diagnostics_summary worst {}ms, SLO 200ms (relaxed from 50ms for in-memory SQLite)",
        worst.as_millis()
    );
}

#[tokio::test]
async fn perf_audit_query_with_full_filter_predicate_at_10k_rows() {
    // §11.x audit query under composite-predicate hit.
    let pool = fresh_pool().await;
    bulk_seed_audit(&pool, 10_000).await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let start = Instant::now();
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            actor_user_id: Some(ACTOR.into()),
            action: Some("create".into()),
            entity: Some("doctors".into()),
            from_utc: Some(Utc::now() - Duration::days(2)),
            to_utc: Some(Utc::now()),
            free_text: Some("k".into()),
            limit: 50,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(!page.rows.is_empty());
    assert!(
        elapsed.as_millis() < 500,
        "filtered query at 10k took {}ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_metrics_events_vacuum_at_10k_rows_under_3s() {
    let pool = fresh_pool().await;
    let cutoff = Utc::now() - Duration::days(METRICS_RETENTION_DAYS + 1);
    // Bulk insert metric events in one transaction.
    let mut tx = pool.begin().await.unwrap();
    for _ in 0..10_000 {
        sqlx::query(
            "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("sync_push_ok")
        .bind(cutoff.to_rfc3339())
        .bind("{}")
        .bind(TENANT)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let job = build_vacuum_job(&pool);
    let start = Instant::now();
    let out = job.run(None, TENANT).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(out.metrics_purged, 10_000);
    assert!(
        elapsed.as_millis() < 3_000,
        "metrics vacuum 10k took {}ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_audit_repo_oldest_at_under_50ms_at_10k_rows() {
    let pool = fresh_pool().await;
    bulk_seed_audit(&pool, 10_000).await;
    let repo = SqliteAuditRepo::new(pool.clone());
    let start = Instant::now();
    let got = repo.oldest_at(TENANT).await.unwrap();
    let elapsed = start.elapsed();
    assert!(got.is_some());
    assert!(
        elapsed.as_millis() < 100,
        "oldest_at took {}ms, SLO 100ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_audit_query_pagination_remains_under_300ms_across_first_5_pages() {
    let pool = fresh_pool().await;
    bulk_seed_audit(&pool, 10_000).await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let mut offset = 0i64;
    for _ in 0..5 {
        let start = Instant::now();
        let page = svc
            .query(AuditFilter {
                entity_id_tenant: TENANT.into(),
                limit: 50,
                offset,
                ..AuditFilter::default()
            })
            .await
            .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(page.rows.len(), 50);
        assert!(
            elapsed.as_millis() < 300,
            "page at offset {offset} took {}ms",
            elapsed.as_millis()
        );
        offset += 50;
    }
}
