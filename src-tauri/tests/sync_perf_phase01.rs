//! Phase-01 §7 perf SLO assertions.
//!
//! Single-sample latency / throughput pins. Multi-sample p99 / soak tests
//! live in `performance-soak.md` (phase-08 owns the harness). These are
//! deterministic enough to run in CI as hard pass/fail gates: a regression
//! that doubles the cost of a primary read fails this suite immediately.

use std::str::FromStr;
use std::time::Instant;

use app_lib::db::migrations;
use app_lib::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use app_lib::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::domain::value_objects::AuditAction;
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

async fn fresh_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
        .unwrap();
    migrations::run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn perf_outbox_enqueue_single_row_under_ten_ms() {
    // §7 SLO (Tauri SQLite single-record write): the local commit that
    // gates UI confirmation must stay well under 30 ms p99. A single-row
    // outbox enqueue is the simplest write -- pin it at 10 ms.
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());

    let start = Instant::now();
    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    repo.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "single outbox enqueue took {} ms (>10 ms SLO)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_outbox_pending_count_at_10k_rows_under_one_hundred_ms() {
    // §7 SLO: SyncPill polls pending_count every 2 s. Even at the 10x of
    // typical depth (10k vs the documented 800-row steady-state cap), the
    // read must stay snappy enough not to block the UI.
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    for i in 0..10_000 {
        let op = OutboxOp::new("audit_log", format!("row-{i}"), b"x".to_vec());
        repo.enqueue(&mut tx, &op).await.unwrap();
    }
    tx.commit().await.unwrap();

    let start = Instant::now();
    let n = repo.pending_count().await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(n, 10_000);
    assert!(
        elapsed.as_millis() < 100,
        "pending_count over 10k rows took {} ms (>100 ms SLO)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_outbox_drain_throughput_meets_fifty_ops_per_second_floor() {
    // §7 SLO: outbox drain throughput >= 50 ops/sec. Approximated here by
    // measuring the wall-clock cost of `delete_acked` on a 200-row batch
    // (the real drain wraps a network call too -- this isolates the SQL
    // step). Required: the SQL drain alone clears 200 rows in well under
    // 4 s, leaving 3+ s of budget for the network round-trip per batch.
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());

    let mut ids = Vec::with_capacity(200);
    let mut tx = pool.begin().await.unwrap();
    for i in 0..200 {
        let op = OutboxOp::new("audit_log", format!("row-{i}"), b"x".to_vec());
        repo.enqueue(&mut tx, &op).await.unwrap();
        ids.push(op.op_id);
    }
    tx.commit().await.unwrap();

    let start = Instant::now();
    repo.delete_acked(&ids).await.unwrap();
    let elapsed = start.elapsed();

    let throughput = (ids.len() as f64) / elapsed.as_secs_f64();
    assert!(
        throughput >= 50.0,
        "drain throughput {:.0} ops/sec (<50 ops/sec SLO; took {} ms)",
        throughput,
        elapsed.as_millis()
    );
    assert_eq!(repo.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn perf_audit_append_single_row_under_ten_ms() {
    // §7 SLO (audit-first invariant inside with_audit): the audit insert
    // must stay cheap so the audit-first ordering doesn't dominate the
    // write budget. Single-row append at 10 ms.
    let pool = fresh_pool().await;
    let repo = SqliteAuditRepo::new(pool.clone());

    let start = Instant::now();
    let mut tx = pool.begin().await.unwrap();
    let entry = AuditEntry::create(AuditCreateInput {
        actor_user_id: Uuid::now_v7(),
        action: AuditAction::Login,
        entity: "user".into(),
        entity_id: "u-1".into(),
        delta: serde_json::json!({}),
        ip: None,
        device_id: "dev".into(),
        entity_id_tenant: "tenant-x".into(),
    });
    repo.append(&mut tx, &entry).await.unwrap();
    tx.commit().await.unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "single audit append took {} ms (>10 ms SLO)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn perf_audit_list_by_tenant_index_used_at_5k_rows_under_thirty_ms() {
    // §7 SLO (list query, 50 rows): the audit_log_tenant_at index keeps
    // `list_by_tenant LIMIT 50` fast even with 5k rows in the table.
    let pool = fresh_pool().await;
    let repo = SqliteAuditRepo::new(pool.clone());

    let mut tx = pool.begin().await.unwrap();
    for i in 0..5_000 {
        let entry = AuditEntry::create(AuditCreateInput {
            actor_user_id: Uuid::now_v7(),
            action: AuditAction::Login,
            entity: "user".into(),
            entity_id: format!("u-{i}"),
            delta: serde_json::json!({}),
            ip: None,
            device_id: "dev".into(),
            entity_id_tenant: "tenant-x".into(),
        });
        repo.append(&mut tx, &entry).await.unwrap();
    }
    tx.commit().await.unwrap();

    let start = Instant::now();
    let rows = repo.list_by_tenant("tenant-x", 50, 0).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(rows.len(), 50);
    assert!(
        elapsed.as_millis() < 30,
        "list_by_tenant LIMIT 50 over 5k rows took {} ms (>30 ms SLO)",
        elapsed.as_millis()
    );
}
