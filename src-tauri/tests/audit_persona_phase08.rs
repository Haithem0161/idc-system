//! Phase-08 canonical persona script: **P3 Mariam the Superadmin**.
//!
//! Walks the audit + diagnostics + vacuum surface end-to-end in 10 sequenced
//! steps. See `docs/idc-system/testing/personas.md` for the narrative.
//!
//! Steps:
//! 1. Bootstrap an in-memory database with all migrations applied.
//! 2. Role gate -- Mariam is superadmin; receptionist + accountant blocked.
//! 3. Seed 5 days of operator audit history (15 rows across actors + actions).
//! 4. Audit query: filter by `action = lock` and `entity = visits` -- finds
//!    today's locks.
//! 5. Audit query: entity_id prefix lookup -- jumps to a specific visit.
//! 6. Audit query: free-text search -- finds the void reason from yesterday.
//! 7. Diagnostics summary -- reads outbox depth + last-sync + conflict count
//!    + receipt-print rate.
//! 8. Trigger `audit_vacuum_now` -- prunes 90-day-old rows + writes one
//!    self-audit row + enqueues one outbox push.
//! 9. Audit query after vacuum -- self-audit row appears in the most-recent
//!    page; cursor `last_audit_vacuum_at` stamped.
//! 10. Subsequent vacuum invocation produces a second self-audit row but no
//!     new prunes (idempotent over already-pruned data).

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::audit::domain::repositories::MetricsRepo;
use app_lib::domains::audit::infrastructure::SqliteMetricsRepo;
use app_lib::domains::audit::service::{
    AuditQueryService, AuditVacuumJob, DiagnosticsService, AUDIT_RETENTION_DAYS,
    SYSTEM_VACUUM_ENTITY_ID,
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

const TENANT: &str = "tenant-mariam";
const MARIAM_ID: &str = "00000000-0000-0000-0000-000000000301";
const RIYAD_ID: &str = "00000000-0000-0000-0000-000000000302";
const ASMA_ID: &str = "00000000-0000-0000-0000-000000000303";

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
        .ensure_device_id("device-mariam")
        .await
        .unwrap();
    pool
}

fn audit_entry(
    actor: &str,
    action: AuditAction,
    entity: &str,
    entity_id: &str,
    delta: serde_json::Value,
    at: chrono::DateTime<Utc>,
    dirty: bool,
) -> AuditEntry {
    let mut e = AuditEntry::create(AuditCreateInput {
        actor_user_id: Uuid::parse_str(actor).unwrap(),
        action,
        entity: entity.into(),
        entity_id: entity_id.into(),
        delta,
        ip: None,
        device_id: "device-mariam".into(),
        entity_id_tenant: TENANT.into(),
    });
    e.at = at;
    e.created_at = at;
    e.updated_at = at;
    e.dirty = dirty;
    e
}

async fn append(pool: &SqlitePool, entry: &AuditEntry) {
    let repo = SqliteAuditRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    repo.append(&mut tx, entry).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn p3_mariam_superadmin_day_walks_the_phase_08_surface_end_to_end() {
    // ----- Step 1: Bootstrap --------------------------------------------
    let pool = fresh_pool().await;
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let query_svc = AuditQueryService::new(audit_repo.clone(), pool.clone());
    let diagnostics_svc = DiagnosticsService::new(
        metrics_repo.clone(),
        outbox_repo.clone(),
        state_repo.clone(),
    );
    let vacuum_job = AuditVacuumJob::new(
        pool.clone(),
        audit_repo.clone(),
        metrics_repo.clone(),
        outbox_repo.clone(),
        state_repo.clone(),
        "device-mariam".into(),
    );

    // ----- Step 2: Role gate (positive + negative) ----------------------
    assert!(AuditQueryService::require_audit_role(UserRole::Superadmin).is_ok());
    assert!(AuditQueryService::require_audit_role(UserRole::Receptionist).is_err());
    assert!(AuditQueryService::require_audit_role(UserRole::Accountant).is_err());

    // ----- Step 3: Seed 5 days of operator history ----------------------
    let now = Utc::now();
    // Today: 3 locks on visits by Riyad.
    let visit_ids = [
        "00000001-0000-0000-0000-000000000001",
        "00000002-0000-0000-0000-000000000002",
        "00000003-0000-0000-0000-000000000003",
    ];
    for (i, vid) in visit_ids.iter().enumerate() {
        let lock = audit_entry(
            RIYAD_ID,
            AuditAction::Lock,
            "visits",
            vid,
            serde_json::json!({
                "items": 3,
                "total_iqd": 25000 + i as i64 * 5000
            }),
            now - Duration::hours(i as i64),
            false,
        );
        append(&pool, &lock).await;
    }
    // Yesterday: a void by Mariam with reason.
    let void = audit_entry(
        MARIAM_ID,
        AuditAction::Void,
        "visits",
        "00000004-0000-0000-0000-000000000004",
        serde_json::json!({"void_reason": "duplicate billing"}),
        now - Duration::days(1),
        false,
    );
    append(&pool, &void).await;
    // 2 days ago: 2 patient creates by Riyad.
    for i in 0..2 {
        let pid = format!("a0000000-0000-0000-0000-00000000000{i}");
        let create = audit_entry(
            RIYAD_ID,
            AuditAction::Create,
            "patients",
            &pid,
            serde_json::json!({"name": "أحمد"}),
            now - Duration::days(2) - Duration::hours(i as i64),
            false,
        );
        append(&pool, &create).await;
    }
    // Asma's logins (1 per day for 5 days).
    for i in 0..5 {
        let login = audit_entry(
            ASMA_ID,
            AuditAction::Login,
            "audit_log",
            "00000000-0000-0000-0000-000000000000",
            serde_json::json!({"ip": "127.0.0.1"}),
            now - Duration::days(i),
            false,
        );
        append(&pool, &login).await;
    }
    // 91 days ago: stale rows eligible for vacuum.
    let stale = audit_entry(
        RIYAD_ID,
        AuditAction::Update,
        "doctors",
        "fc000000-0000-0000-0000-000000000fff",
        serde_json::json!({"name": {"from": "Dr A", "to": "Dr B"}}),
        now - Duration::days(AUDIT_RETENTION_DAYS + 1),
        false,
    );
    append(&pool, &stale).await;
    let stale2 = audit_entry(
        RIYAD_ID,
        AuditAction::Update,
        "doctors",
        "fc000000-0000-0000-0000-000000000ffe",
        serde_json::json!({"phone": {"from": "x", "to": "y"}}),
        now - Duration::days(AUDIT_RETENTION_DAYS + 2),
        false,
    );
    append(&pool, &stale2).await;

    // Seed metrics for diagnostics: 5 lock pairs + 9 receipt prints + 2 conflicts.
    for i in 0..5 {
        let visit_id = visit_ids[0];
        let start = now - Duration::minutes(i + 1);
        sqlx::query(
            "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("lock_start")
        .bind(start.to_rfc3339())
        .bind(format!("{{\"visit_id\":\"{visit_id}-{i}\"}}"))
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("lock_end")
        .bind((start + Duration::milliseconds(80 + i * 15)).to_rfc3339())
        .bind(format!("{{\"visit_id\":\"{visit_id}-{i}\"}}"))
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
    }
    for _ in 0..9 {
        sqlx::query(
            "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("receipt_print_ok")
        .bind(now.to_rfc3339())
        .bind("{}")
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
    }
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("sync_conflict")
        .bind((now - Duration::hours(2)).to_rfc3339())
        .bind("{}")
        .bind(TENANT)
        .execute(&pool)
        .await
        .unwrap();
    }

    // ----- Step 4: filter by action + entity (today's locks) ------------
    let locks = query_svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            action: Some("lock".into()),
            entity: Some("visits".into()),
            from_utc: Some(now - Duration::hours(6)),
            to_utc: Some(now + Duration::seconds(1)),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(locks.rows.len(), 3, "today's locks must appear");

    // ----- Step 5: entity_id prefix lookup ------------------------------
    let one_visit = query_svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            entity_id_prefix: Some("00000001".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(one_visit.rows.len(), 1);
    assert!(one_visit.rows[0].entity_id.starts_with("00000001"));

    // ----- Step 6: free-text search for yesterday's void reason ---------
    let voids = query_svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            free_text: Some("duplicate billing".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(voids.rows.len(), 1);
    assert_eq!(voids.rows[0].action, "void");

    // ----- Step 7: diagnostics summary ----------------------------------
    let summary = diagnostics_svc.summary(TENANT).await.unwrap();
    assert!(summary.lock_latency_p95_ms.is_some());
    assert!(
        (summary.receipt_print_success_rate_30d.unwrap() - 1.0).abs() < 1e-6,
        "all prints succeeded"
    );
    assert_eq!(summary.conflict_count_7d, 2);
    // No outbox pushes yet (only audit_log direct inserts during seed).
    assert_eq!(summary.outbox_depth, 0);

    // ----- Step 8: trigger audit vacuum ---------------------------------
    let outcome = vacuum_job
        .run(Some(Uuid::parse_str(MARIAM_ID).unwrap()), TENANT)
        .await
        .unwrap();
    assert_eq!(outcome.audit_purged, 2, "two stale rows pruned");
    assert_eq!(outcome.metrics_purged, 0);
    // last_audit_vacuum_at is now populated.
    let state = state_repo.get().await.unwrap();
    assert!(state.last_audit_vacuum_at.is_some());

    // ----- Step 9: post-vacuum query surfaces the self-audit row --------
    let after = query_svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            action: Some("vacuum".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(after.rows.len(), 1);
    assert_eq!(after.rows[0].entity_id, SYSTEM_VACUUM_ENTITY_ID);
    assert_eq!(after.rows[0].actor_user_id, MARIAM_ID);
    assert_eq!(after.rows[0].delta["audit_purged"], serde_json::json!(2));

    // Outbox now carries the self-audit push.
    let summary2 = diagnostics_svc.summary(TENANT).await.unwrap();
    assert_eq!(summary2.outbox_depth, 1);

    // ----- Step 10: re-running vacuum is idempotent over data -----------
    let outcome2 = vacuum_job
        .run(Some(Uuid::parse_str(MARIAM_ID).unwrap()), TENANT)
        .await
        .unwrap();
    assert_eq!(outcome2.audit_purged, 0);
    assert_eq!(outcome2.metrics_purged, 0);
    let after2 = query_svc
        .query(AuditFilter {
            entity_id_tenant: TENANT.into(),
            action: Some("vacuum".into()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(
        after2.rows.len(),
        2,
        "second run records its own self-audit"
    );
}
