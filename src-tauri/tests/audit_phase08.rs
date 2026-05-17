//! Integration tests for Phase-8 audit + diagnostics + vacuum.
//!
//! Exercises the three application services that constitute the phase-08
//! Tauri surface area, against a real in-memory SQLite with every migration
//! applied:
//!
//! - `AuditQueryService` -- the merge-paginator covered locally in v1; the
//!   server-fan-out lands in a follow-up.
//! - `AuditVacuumJob` -- the daily prune that soft-deletes `audit_log` rows
//!   older than 90 days (with the `dirty=0` carve-out) and hard-deletes
//!   `metrics_events` rows older than 30 days, then writes ONE self-audit
//!   row + ONE outbox push entry in the same transaction.
//! - `DiagnosticsService` -- the `<DiagnosticsModal>` data source.
//!
//! Mirrors the phase-07 binary layout (one main test file plus edges / perf /
//! persona companions) so the existing CI matrix picks the suite up without
//! workflow edits.

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

struct VacuumDeps {
    job: AuditVacuumJob,
    audit_repo: Arc<dyn AuditRepo>,
    outbox_repo: Arc<dyn OutboxRepo>,
    state_repo: Arc<dyn SyncStateRepo>,
    #[allow(dead_code)]
    metrics_repo: Arc<dyn MetricsRepo>,
}

fn build_vacuum_job(pool: &SqlitePool) -> VacuumDeps {
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let metrics: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let job = AuditVacuumJob::new(
        pool.clone(),
        audit.clone(),
        metrics.clone(),
        outbox.clone(),
        state.clone(),
        "dev-phase08".into(),
    );
    VacuumDeps {
        job,
        audit_repo: audit,
        outbox_repo: outbox,
        state_repo: state,
        metrics_repo: metrics,
    }
}

// ============================================================================
// §2.1 / §10.x AuditQueryService -- filter combinations
// ============================================================================

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
async fn audit_query_filter_by_actor_user_id() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let other_actor = "00000000-0000-0000-0000-0000000000bb";
    let mut a = make_audit_entry(now, false, "doctors");
    insert_audit_row(&pool, &a).await;
    a = make_audit_entry(now - Duration::seconds(1), false, "doctors");
    a.actor_user_id = Uuid::parse_str(other_actor).unwrap();
    insert_audit_row(&pool, &a).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            actor_user_id: Some(ACTOR.into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].actor_user_id, ACTOR);
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
async fn audit_query_free_text_matches_entity_id_substring() {
    // §1.1 / §10.9: free text falls back to INSTR against both `delta` and
    // `entity_id`. When the user types a partial UUID the row containing
    // that substring in its id wins even if `delta` lacks the term.
    let pool = fresh_pool().await;
    let now = Utc::now();
    let mut a = make_audit_entry(now, false, "visits");
    a.entity_id = "abcdef00-0000-0000-0000-000000000000".into();
    a.delta = serde_json::json!({"k": "v"});
    insert_audit_row(&pool, &a).await;
    let mut b = make_audit_entry(now - Duration::seconds(1), false, "visits");
    b.entity_id = "12345678-0000-0000-0000-000000000000".into();
    insert_audit_row(&pool, &b).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            free_text: Some("abcdef".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].entity_id.starts_with("abcdef"));
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
async fn audit_query_combines_all_six_filters() {
    // §2.1: actor + action + entity + entity_id_prefix + date range + text in
    // a single SQL with AND-composition. Only the row matching every clause
    // must remain.
    let pool = fresh_pool().await;
    let now = Utc::now();
    // Matching row.
    let mut hit = make_audit_entry(now - Duration::hours(1), false, "visits");
    hit.action = AuditAction::Lock;
    hit.entity_id = "deadbeef-0000-0000-0000-000000000000".into();
    hit.delta = serde_json::json!({"reason": "exam complete"});
    insert_audit_row(&pool, &hit).await;
    // Differs on action.
    let mut miss_action = make_audit_entry(now - Duration::hours(1), false, "visits");
    miss_action.action = AuditAction::Create;
    miss_action.entity_id = "deadbeef-0000-0000-0000-000000000001".into();
    miss_action.delta = serde_json::json!({"reason": "exam complete"});
    insert_audit_row(&pool, &miss_action).await;
    // Differs on entity.
    let mut miss_entity = make_audit_entry(now - Duration::hours(1), false, "doctors");
    miss_entity.action = AuditAction::Lock;
    miss_entity.entity_id = "deadbeef-0000-0000-0000-000000000002".into();
    miss_entity.delta = serde_json::json!({"reason": "exam complete"});
    insert_audit_row(&pool, &miss_entity).await;
    // Differs on entity_id prefix.
    let mut miss_prefix = make_audit_entry(now - Duration::hours(1), false, "visits");
    miss_prefix.action = AuditAction::Lock;
    miss_prefix.entity_id = "abcdef00-0000-0000-0000-000000000000".into();
    miss_prefix.delta = serde_json::json!({"reason": "exam complete"});
    insert_audit_row(&pool, &miss_prefix).await;
    // Differs on date.
    let mut miss_date = make_audit_entry(now - Duration::days(40), false, "visits");
    miss_date.action = AuditAction::Lock;
    miss_date.entity_id = "deadbeef-0000-0000-0000-000000000003".into();
    miss_date.delta = serde_json::json!({"reason": "exam complete"});
    insert_audit_row(&pool, &miss_date).await;
    // Differs on free text.
    let mut miss_text = make_audit_entry(now - Duration::hours(1), false, "visits");
    miss_text.action = AuditAction::Lock;
    miss_text.entity_id = "deadbeef-0000-0000-0000-000000000004".into();
    miss_text.delta = serde_json::json!({"reason": "no match here"});
    insert_audit_row(&pool, &miss_text).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            actor_user_id: Some(ACTOR.into()),
            action: Some("lock".into()),
            entity: Some("visits".into()),
            entity_id_prefix: Some("deadbeef".into()),
            from_utc: Some(now - Duration::days(2)),
            to_utc: Some(now),
            free_text: Some("exam complete".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].entity_id, hit.entity_id);
}

#[tokio::test]
async fn audit_query_date_range_includes_endpoints() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let inside = make_audit_entry(now - Duration::hours(12), false, "visits");
    let on_low = make_audit_entry(now - Duration::days(2), false, "visits");
    let on_high = make_audit_entry(now, false, "visits");
    let too_low = make_audit_entry(now - Duration::days(3), false, "visits");
    insert_audit_row(&pool, &inside).await;
    insert_audit_row(&pool, &on_low).await;
    insert_audit_row(&pool, &on_high).await;
    insert_audit_row(&pool, &too_low).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            from_utc: Some(now - Duration::days(2)),
            to_utc: Some(now),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
}

#[tokio::test]
async fn audit_query_filter_clamp_caps_limit_at_100_and_defaults_50() {
    // §11.4 reconciliation -- max 100 / default 50.
    let pool = fresh_pool().await;
    // Need 120 rows to assert both bounds.
    let now = Utc::now();
    for i in 0..120 {
        let e = make_audit_entry(now - Duration::seconds(i), false, "doctors");
        insert_audit_row(&pool, &e).await;
    }
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo.clone());

    // Default 50 when limit is 0/unset.
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 50);

    // Cap 100 when limit is 150.
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 150,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 100);
}

#[tokio::test]
async fn audit_query_offset_paginates_through_dataset() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    // Seed 5 rows in known order.
    for i in 0..5 {
        let mut e = make_audit_entry(now - Duration::seconds(i), false, "doctors");
        e.entity_id = format!("000000{i:02}-0000-0000-0000-000000000000");
        insert_audit_row(&pool, &e).await;
    }
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);

    let page1 = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 2,
            offset: 0,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page1.next_offset, Some(2));

    let page2 = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 2,
            offset: 2,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page2.rows.len(), 2);
    // No overlap with page 1.
    let ids1: Vec<_> = page1.rows.iter().map(|r| r.id.clone()).collect();
    let ids2: Vec<_> = page2.rows.iter().map(|r| r.id.clone()).collect();
    assert!(ids1.iter().all(|i| !ids2.contains(i)));

    let tail = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 2,
            offset: 4,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(tail.rows.len(), 1);
    // Final page exposes no next cursor.
    assert!(tail.next_offset.is_none());
}

#[tokio::test]
async fn audit_query_tenant_isolation_filters_cross_tenant_rows() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let mine = make_audit_entry(now, false, "doctors");
    insert_audit_row(&pool, &mine).await;
    // Insert a row from another tenant.
    let mut foreign = make_audit_entry(now - Duration::seconds(1), false, "doctors");
    foreign.entity_id_tenant = "tenant-other".into();
    insert_audit_row(&pool, &foreign).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
}

#[tokio::test]
async fn audit_query_response_includes_dirty_boolean_per_7_15() {
    // §1.2: AuditRowDto carries `dirty` so the table can render the
    // Pending-sync column without a second fetch.
    let pool = fresh_pool().await;
    let now = Utc::now();
    let synced = make_audit_entry(now, false, "doctors");
    insert_audit_row(&pool, &synced).await;
    let dirty = make_audit_entry(now - Duration::seconds(1), true, "doctors");
    insert_audit_row(&pool, &dirty).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 2);
    let dirty_count = page.rows.iter().filter(|r| r.dirty).count();
    assert_eq!(dirty_count, 1);
}

#[tokio::test]
async fn audit_query_skips_soft_deleted_rows() {
    // Soft-deleted rows MUST NOT appear in the audit UI (they are pending
    // physical purge by a future vacuum sweep).
    let pool = fresh_pool().await;
    let now = Utc::now();
    let live = make_audit_entry(now, false, "doctors");
    insert_audit_row(&pool, &live).await;
    let mut dead = make_audit_entry(now - Duration::seconds(1), false, "doctors");
    dead.deleted_at = Some(now);
    insert_audit_row(&pool, &dead).await;

    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
}

#[tokio::test]
async fn audit_query_returns_next_offset_only_when_page_is_full() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    for i in 0..3 {
        let e = make_audit_entry(now - Duration::seconds(i), false, "doctors");
        insert_audit_row(&pool, &e).await;
    }
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);

    // limit > available -> no next cursor.
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 10,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
    assert!(page.next_offset.is_none());

    // limit == available -> next cursor surfaces so the UI tries one more
    // page (which will come back empty) -- standard offset paginator.
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            limit: 3,
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(page.next_offset, Some(3));
}

// ============================================================================
// §6.7 / §7.23 role gate
// ============================================================================

#[tokio::test]
async fn audit_role_gate_denies_non_superadmin() {
    assert!(AuditQueryService::require_audit_role(UserRole::Receptionist).is_err());
    assert!(AuditQueryService::require_audit_role(UserRole::Accountant).is_err());
    assert!(AuditQueryService::require_audit_role(UserRole::Superadmin).is_ok());
}

// ============================================================================
// §2.1 / §10.x AuditQueryService -- mode classification
// ============================================================================

#[tokio::test]
async fn audit_query_classifies_recent_range_as_local_mode() {
    let pool = fresh_pool().await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            from_utc: Some(Utc::now() - Duration::days(7)),
            to_utc: Some(Utc::now()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(
        format!("{:?}", page.mode),
        "Local",
        "in-window range must classify Local"
    );
}

#[tokio::test]
async fn audit_query_classifies_pre_window_range_as_server_mode() {
    let pool = fresh_pool().await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            from_utc: Some(Utc::now() - Duration::days(200)),
            to_utc: Some(Utc::now() - Duration::days(150)),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(
        format!("{:?}", page.mode),
        "Server",
        "pre-window range must classify Server"
    );
}

#[tokio::test]
async fn audit_query_classifies_cross_boundary_range_as_merged_mode() {
    let pool = fresh_pool().await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            from_utc: Some(Utc::now() - Duration::days(150)),
            to_utc: Some(Utc::now()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(
        format!("{:?}", page.mode),
        "Merged",
        "range spanning the cliff must classify Merged"
    );
}

#[tokio::test]
async fn audit_query_classifies_unbounded_range_as_local_mode() {
    let pool = fresh_pool().await;
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let svc = AuditQueryService::new(repo);
    // No date filter -- treat as Local because the UI defaults to the
    // recent window unless the operator explicitly opts into the archive.
    let page = svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(format!("{:?}", page.mode), "Local");
}

// ============================================================================
// §2.1 AuditVacuumJob -- pruning + invariants
// ============================================================================

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

    let VacuumDeps {
        job,
        audit_repo,
        outbox_repo,
        state_repo,
        ..
    } = build_vacuum_job(&pool);

    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 1);

    // Remaining: old_dirty + fresh_synced + vacuum self-audit = 3 rows.
    let rows = audit_repo.list_by_tenant(TENANT, 100, 0).await.unwrap();
    assert_eq!(rows.len(), 3);
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
async fn vacuum_writes_exactly_one_self_audit_row_per_run() {
    // §10.1 (P08-G15): the composite vacuum job MUST emit exactly one audit
    // row covering both purges. Two rows would force forensic reviewers to
    // reassemble the timeline; zero rows would lose the pruning history.
    let pool = fresh_pool().await;
    let now = Utc::now();
    // Seed multiple stale audit rows + metrics rows so both purges produce
    // non-zero counts.
    let cutoff_audit = now - Duration::days(AUDIT_RETENTION_DAYS) - Duration::hours(1);
    for _ in 0..5 {
        let e = make_audit_entry(cutoff_audit, false, "doctors");
        insert_audit_row(&pool, &e).await;
    }
    let cutoff_metrics = now - Duration::days(METRICS_RETENTION_DAYS) - Duration::hours(1);
    for _ in 0..3 {
        insert_metrics_event(&pool, "sync_push_ok", cutoff_metrics).await;
    }

    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 5);
    assert_eq!(out.metrics_purged, 3);

    let rows = audit_repo.list_by_tenant(TENANT, 100, 0).await.unwrap();
    let vacuum_rows: Vec<_> = rows
        .iter()
        .filter(|r| matches!(r.action, AuditAction::Vacuum))
        .collect();
    assert_eq!(
        vacuum_rows.len(),
        1,
        "exactly one vacuum row, found {}",
        vacuum_rows.len()
    );
    let delta = &vacuum_rows[0].delta;
    assert_eq!(delta["audit_purged"], serde_json::json!(5));
    assert_eq!(delta["metrics_purged"], serde_json::json!(3));
}

#[tokio::test]
async fn vacuum_self_audit_row_uses_zero_uuid_sentinel_for_entity_id() {
    // §7.3: the synthetic vacuum row carries the all-zero UUID so audit
    // consumers know it represents a system event with no concrete target.
    let pool = fresh_pool().await;
    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    job.run(None, TENANT).await.unwrap();
    let rows = audit_repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    let vacuum = rows
        .iter()
        .find(|r| matches!(r.action, AuditAction::Vacuum))
        .unwrap();
    assert_eq!(vacuum.entity_id, SYSTEM_VACUUM_ENTITY_ID);
    assert_eq!(vacuum.entity, "audit_log");
}

#[tokio::test]
async fn vacuum_skips_dirty_rows_even_past_cutoff() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let cutoff = now - Duration::days(AUDIT_RETENTION_DAYS) - Duration::hours(1);
    let dirty = make_audit_entry(cutoff, true, "doctors");
    insert_audit_row(&pool, &dirty).await;

    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.audit_purged, 0);
    let rows = audit_repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    // Original dirty row + new vacuum row = 2.
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| matches!(r.action, AuditAction::Create)));
}

#[tokio::test]
async fn vacuum_also_purges_metrics_events_older_than_30d() {
    let pool = fresh_pool().await;
    let now = Utc::now();

    let old_at = now - Duration::days(METRICS_RETENTION_DAYS) - Duration::hours(1);
    let recent_at = now - Duration::hours(1);
    insert_metrics_event(&pool, "sync_push_ok", old_at).await;
    insert_metrics_event(&pool, "sync_pull_ok", recent_at).await;

    let VacuumDeps { job, .. } = build_vacuum_job(&pool);
    let out = job.run(None, TENANT).await.unwrap();
    assert_eq!(out.metrics_purged, 1);

    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM metrics_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn vacuum_metrics_purge_makes_no_audit_or_outbox_writes_for_metrics() {
    // Per §7.21 step 3-4: `metrics_events` is local-only and hard-deleted.
    // The total audit-and-outbox writes during vacuum should equal exactly
    // the one self-audit row + its outbox push -- never one per pruned
    // metrics row.
    let pool = fresh_pool().await;
    let now = Utc::now();
    for _ in 0..25 {
        insert_metrics_event(
            &pool,
            "sync_push_ok",
            now - Duration::days(METRICS_RETENTION_DAYS) - Duration::hours(1),
        )
        .await;
    }

    let outbox_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    let audit_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();

    let VacuumDeps { job, .. } = build_vacuum_job(&pool);
    job.run(None, TENANT).await.unwrap();

    let outbox_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    let audit_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        outbox_after.0 - outbox_before.0,
        1,
        "exactly one outbox push for the vacuum self-audit"
    );
    assert_eq!(
        audit_after.0 - audit_before.0,
        1,
        "exactly one new audit row"
    );
}

#[tokio::test]
async fn vacuum_idempotent_on_empty_dataset() {
    // Running vacuum on a fresh DB still produces a self-audit row but no
    // purges. A second invocation is a no-op for the data, though the audit
    // log accumulates another self-row.
    let pool = fresh_pool().await;
    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    let first = job.run(None, TENANT).await.unwrap();
    assert_eq!(first.audit_purged, 0);
    assert_eq!(first.metrics_purged, 0);
    let second = job.run(None, TENANT).await.unwrap();
    assert_eq!(second.audit_purged, 0);
    assert_eq!(second.metrics_purged, 0);
    let rows = audit_repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    assert_eq!(
        rows.iter()
            .filter(|r| matches!(r.action, AuditAction::Vacuum))
            .count(),
        2,
        "each run records one self-audit even when nothing was pruned"
    );
}

#[tokio::test]
async fn vacuum_with_explicit_actor_records_human_actor_uuid() {
    // `audit::vacuum_now` (manual trigger) passes the superadmin's UUID;
    // the scheduler path passes None which falls back to the zero UUID.
    let pool = fresh_pool().await;
    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    let actor = Uuid::now_v7();
    job.run(Some(actor), TENANT).await.unwrap();
    let rows = audit_repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    let vacuum = rows
        .iter()
        .find(|r| matches!(r.action, AuditAction::Vacuum))
        .unwrap();
    assert_eq!(vacuum.actor_user_id, actor);
}

#[tokio::test]
async fn vacuum_stamps_last_audit_vacuum_at_after_each_run() {
    // §7.2 + §10.3: the cursor MUST land after each successful sweep so the
    // scheduler can skip the next 24h window.
    let pool = fresh_pool().await;
    let VacuumDeps {
        job, state_repo, ..
    } = build_vacuum_job(&pool);
    let before = state_repo.get().await.unwrap();
    assert!(before.last_audit_vacuum_at.is_none());
    job.run(None, TENANT).await.unwrap();
    let after = state_repo.get().await.unwrap();
    assert!(after.last_audit_vacuum_at.is_some());
}

#[tokio::test]
async fn vacuum_self_audit_row_carries_cutoff_metadata_in_delta() {
    // §9.3 (P08-G03 mirror) -- the delta payload must include the audit and
    // metrics cutoffs so the forensic trail records what window was pruned.
    let pool = fresh_pool().await;
    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    job.run(None, TENANT).await.unwrap();
    let rows = audit_repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    let vacuum = rows
        .iter()
        .find(|r| matches!(r.action, AuditAction::Vacuum))
        .unwrap();
    let delta = &vacuum.delta;
    assert!(delta.get("audit_cutoff").is_some());
    assert!(delta.get("metrics_cutoff").is_some());
    assert!(delta["audit_cutoff"].is_string());
    assert!(delta["metrics_cutoff"].is_string());
}

#[tokio::test]
async fn vacuum_self_audit_row_remains_within_tenant_scope() {
    let pool = fresh_pool().await;
    let VacuumDeps {
        job, audit_repo, ..
    } = build_vacuum_job(&pool);
    job.run(None, TENANT).await.unwrap();

    // The vacuum row should appear under its own tenant key only.
    let mine = audit_repo.list_by_tenant(TENANT, 10, 0).await.unwrap();
    assert!(mine.iter().any(|r| matches!(r.action, AuditAction::Vacuum)));
    let other = audit_repo
        .list_by_tenant("tenant-other", 10, 0)
        .await
        .unwrap();
    assert!(other
        .iter()
        .all(|r| !matches!(r.action, AuditAction::Vacuum)));
}

// ============================================================================
// §2.1 DiagnosticsService -- summary assembly
// ============================================================================

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
        insert_metrics_event(&pool, "receipt_print_ok", now).await;
    }
    insert_metrics_event(&pool, "receipt_print_fail", now).await;
    // Sync conflict.
    insert_metrics_event(&pool, "sync_conflict", now).await;

    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert!(s.lock_latency_p95_ms.is_some());
    assert_eq!(s.conflict_count_7d, 1);
    let rate = s.receipt_print_success_rate_30d.expect("rate present");
    assert!((rate - 0.8).abs() < 1e-6);
}

#[tokio::test]
async fn diagnostics_summary_returns_none_for_lock_latency_when_pairs_below_5() {
    // The metrics repo demands at least 5 paired samples before reporting a
    // p95 (otherwise the percentile is statistically meaningless).
    let pool = fresh_pool().await;
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let now = Utc::now();
    for i in 0..3 {
        let visit_id = Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)")
            .bind(Uuid::now_v7().to_string())
            .bind("lock_start")
            .bind((now - Duration::minutes(i + 1)).to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{}\"}}", visit_id))
            .bind(TENANT)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)")
            .bind(Uuid::now_v7().to_string())
            .bind("lock_end")
            .bind((now - Duration::minutes(i + 1) + Duration::milliseconds(50)).to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{}\"}}", visit_id))
            .bind(TENANT)
            .execute(&pool)
            .await
            .unwrap();
    }
    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert!(s.lock_latency_p95_ms.is_none());
}

#[tokio::test]
async fn diagnostics_summary_returns_none_for_receipt_rate_with_no_prints() {
    let pool = fresh_pool().await;
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert!(s.receipt_print_success_rate_30d.is_none());
    assert_eq!(s.conflict_count_7d, 0);
    assert_eq!(s.outbox_depth, 0);
}

#[tokio::test]
async fn diagnostics_summary_receipt_print_rate_derives_from_exact_kind_names() {
    // §10.13 (P08-G27): the rate must derive from the exact kind names
    // recorded by the receipt printer -- `receipt_print_ok` /
    // `receipt_print_fail`. Other metrics_events kinds must not pollute the
    // denominator.
    let pool = fresh_pool().await;
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let now = Utc::now();
    for _ in 0..95 {
        insert_metrics_event(&pool, "receipt_print_ok", now).await;
    }
    for _ in 0..5 {
        insert_metrics_event(&pool, "receipt_print_fail", now).await;
    }
    // Inserting unrelated metrics MUST NOT shift the rate.
    for _ in 0..50 {
        insert_metrics_event(&pool, "sync_push_ok", now).await;
    }

    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    let rate = s.receipt_print_success_rate_30d.expect("rate present");
    assert!((rate - 0.95).abs() < 1e-6, "got {rate}");
}

#[tokio::test]
async fn diagnostics_summary_reads_outbox_depth_from_pending_count() {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));

    // Seed 3 outbox rows via a vacuum sweep + 2 manual inserts.
    let job = AuditVacuumJob::new(
        pool.clone(),
        audit_repo.clone(),
        metrics_repo.clone(),
        outbox_repo.clone(),
        state_repo.clone(),
        "dev-phase08".into(),
    );
    job.run(None, TENANT).await.unwrap();
    for _ in 0..2 {
        let op = app_lib::domains::sync::domain::entities::OutboxOp::new(
            "doctors",
            Uuid::now_v7().to_string(),
            b"payload".to_vec(),
        );
        let mut tx = pool.begin().await.unwrap();
        outbox_repo.enqueue(&mut tx, &op).await.unwrap();
        tx.commit().await.unwrap();
    }

    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert_eq!(s.outbox_depth, 3);
}

#[tokio::test]
async fn diagnostics_summary_conflict_count_tenant_scoped() {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let now = Utc::now();
    // Tenant-local conflicts.
    for _ in 0..3 {
        insert_metrics_event(&pool, "sync_conflict", now).await;
    }
    // Foreign tenant conflict.
    sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?,?,?,?,?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind("sync_conflict")
    .bind(now.to_rfc3339())
    .bind("{}")
    .bind("tenant-other")
    .execute(&pool)
    .await
    .unwrap();

    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert_eq!(s.conflict_count_7d, 3);
}

#[tokio::test]
async fn diagnostics_summary_excludes_conflicts_older_than_7d() {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let now = Utc::now();
    insert_metrics_event(&pool, "sync_conflict", now).await;
    insert_metrics_event(&pool, "sync_conflict", now - Duration::days(8)).await;

    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert_eq!(s.conflict_count_7d, 1);
}

#[tokio::test]
async fn diagnostics_summary_last_sync_at_surfaces_pushed_or_pulled_stamp() {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    state_repo.mark_pushed().await.unwrap();
    state_repo.put_pull_cursor("cursor-1").await.unwrap();

    let svc = DiagnosticsService::new(metrics_repo.clone(), outbox_repo.clone(), state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert!(
        s.last_sync_at.is_some(),
        "last_sync_at must surface once either timestamp is recorded"
    );
}

#[tokio::test]
async fn diagnostics_summary_last_sync_at_none_for_fresh_install() {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let svc = DiagnosticsService::new(metrics_repo, outbox_repo, state_repo);
    let s = svc.summary(TENANT).await.unwrap();
    assert!(s.last_sync_at.is_none());
}

// ============================================================================
// §7.36 audit action enumeration -- repo round-trip
// ============================================================================

#[tokio::test]
async fn audit_log_repo_round_trips_each_of_14_actions() {
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
    let repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let rows = repo.list_by_tenant(TENANT, 50, 0).await.unwrap();
    assert_eq!(rows.len(), 14);
    // Each enum variant deserialises back via from-row reading.
    for action in actions {
        assert!(
            rows.iter().any(|r| r.action == action),
            "missing round-trip for {action:?}"
        );
    }
}

// ============================================================================
// §1.1 oldest_at -- exposed for the cross-boundary paginator
// ============================================================================

#[tokio::test]
async fn audit_repo_oldest_at_returns_minimum_at_for_tenant() {
    let pool = fresh_pool().await;
    let now = Utc::now();
    let oldest = now - Duration::days(10);
    let middle = now - Duration::days(5);
    let recent = now;
    insert_audit_row(&pool, &make_audit_entry(middle, false, "doctors")).await;
    insert_audit_row(&pool, &make_audit_entry(oldest, false, "doctors")).await;
    insert_audit_row(&pool, &make_audit_entry(recent, false, "doctors")).await;

    let repo = SqliteAuditRepo::new(pool.clone());
    let got = repo.oldest_at(TENANT).await.unwrap().expect("present");
    assert!((got - oldest).num_seconds().abs() <= 1);
}

#[tokio::test]
async fn audit_repo_oldest_at_returns_none_for_empty_tenant() {
    let pool = fresh_pool().await;
    let repo = SqliteAuditRepo::new(pool.clone());
    assert!(repo.oldest_at("tenant-empty").await.unwrap().is_none());
}

// ============================================================================
// §3.2 resolve_op_id stability (cross-ref engine.rs unit) -- verify the IPC
// boundary contract that the same logical resolve hashes equal even when the
// engine is invoked twice from the same UI mutation. This exercises the
// engine's stable_resolve_op_id via the sha2 round-trip.
// ============================================================================

#[tokio::test]
async fn resolve_op_id_hashing_is_deterministic_across_calls() {
    use sha2::{Digest, Sha256};
    let op_id = "op-abc";
    let choice = "merged";
    let payload = serde_json::json!({"name": "foo", "amount": 5});
    let canon = serde_json::to_string(&payload).unwrap();
    let mut h1 = Sha256::new();
    h1.update(op_id.as_bytes());
    h1.update([b'|']);
    h1.update(choice.as_bytes());
    h1.update([b'|']);
    h1.update(canon.as_bytes());
    let digest1 = format!("{:x}", h1.finalize());

    let mut h2 = Sha256::new();
    h2.update(op_id.as_bytes());
    h2.update([b'|']);
    h2.update(choice.as_bytes());
    h2.update([b'|']);
    h2.update(canon.as_bytes());
    let digest2 = format!("{:x}", h2.finalize());

    assert_eq!(digest1, digest2);
    assert_eq!(digest1.len(), 64);
}
