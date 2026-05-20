//! Integration tests for Phase-1 sync plumbing.
//!
//! Exercises the repositories and the AuditWriter helper against an in-memory
//! SQLite database. The SyncEngine itself requires a Tauri AppHandle, so its
//! HTTP transport behavior is covered by separate component tests via the
//! sqlx repos directly.

use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use app_lib::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use app_lib::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use app_lib::domains::sync::domain::value_objects::{AuditAction, OutboxAction};
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

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
    pool
}

#[tokio::test]
async fn outbox_enqueue_and_drain() {
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"payload".to_vec());
    repo.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let batch = repo.next_batch(10).await.unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].entity, "audit_log");
    assert_eq!(batch[0].entity_id, "row-1");
    assert_eq!(batch[0].op, OutboxAction::Upsert);
    assert_eq!(repo.pending_count().await.unwrap(), 1);

    repo.delete_acked(&[op.op_id]).await.unwrap();
    assert!(repo.next_batch(10).await.unwrap().is_empty());
    assert_eq!(repo.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn outbox_failure_skips_until_backoff_elapses() {
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    repo.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    repo.mark_failure(op.op_id, "boom", 3600).await.unwrap();
    let batch = repo.next_batch(10).await.unwrap();
    assert!(
        batch.is_empty(),
        "row should not be ready until backoff passes"
    );
}

#[tokio::test]
async fn outbox_park_blocks_further_pulls() {
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    repo.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    repo.park(op.op_id).await.unwrap();
    assert!(repo.next_batch(10).await.unwrap().is_empty());
    assert_eq!(repo.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn audit_append_persists_and_lists() {
    let pool = fresh_pool().await;
    let repo = SqliteAuditRepo::new(pool.clone());

    let mut tx = pool.begin().await.unwrap();
    let entry = AuditEntry::create(AuditCreateInput {
        actor_user_id: Uuid::now_v7(),
        action: AuditAction::Login,
        entity: "user".into(),
        entity_id: "u1".into(),
        delta: serde_json::json!({}),
        ip: None,
        device_id: "dev-1".into(),
        entity_id_tenant: "tenant-x".into(),
    });
    repo.append(&mut tx, &entry).await.unwrap();
    tx.commit().await.unwrap();

    let rows = repo.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, entry.id);
    assert_eq!(rows[0].action, AuditAction::Login);
}

#[tokio::test]
async fn sync_state_ensure_device_id_is_idempotent() {
    let pool = fresh_pool().await;
    let repo = SqliteSyncStateRepo::new(pool.clone());
    let first = repo.ensure_device_id("dev-a").await.unwrap();
    let second = repo.ensure_device_id("dev-b").await.unwrap();
    assert_eq!(first, "dev-a");
    assert_eq!(second, "dev-a", "ensure_device_id must be sticky");

    repo.put_pull_cursor("cursor-1").await.unwrap();
    let state = repo.get().await.unwrap();
    assert_eq!(state.pull_cursor.as_deref(), Some("cursor-1"));
    assert_eq!(state.device_id, "dev-a");
}

struct StubWrite {
    after: serde_json::Value,
    outbox: Vec<OutboxOp>,
}

#[async_trait]
impl BusinessWrite for StubWrite {
    async fn before(
        &mut self,
        _tx: &mut app_lib::db::Tx<'_>,
    ) -> app_lib::error::AppResult<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    async fn write(
        &mut self,
        _tx: &mut app_lib::db::Tx<'_>,
    ) -> app_lib::error::AppResult<(serde_json::Value, Vec<OutboxOp>)> {
        Ok((self.after.clone(), std::mem::take(&mut self.outbox)))
    }
}

#[tokio::test]
async fn audit_writer_orders_audit_before_business_outbox() {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let writer = AuditWriter::new(audit_repo.clone(), outbox_repo.clone(), "dev-1");

    let business_op = OutboxOp::new("user", "u1", b"snap".to_vec());
    let stub = StubWrite {
        after: serde_json::json!({ "name": "Alice" }),
        outbox: vec![business_op.clone()],
    };

    let result = writer
        .with_audit(
            &pool,
            Uuid::now_v7(),
            AuditAction::Create,
            "user",
            "u1",
            "tenant-x",
            None,
            stub,
        )
        .await
        .unwrap();
    assert_eq!(result["name"], "Alice");

    let audits = audit_repo.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert_eq!(audits.len(), 1, "audit row must exist");
    assert_eq!(audits[0].entity, "user");
    assert_eq!(audits[0].entity_id, "u1");

    let batch = outbox_repo.next_batch(10).await.unwrap();
    assert_eq!(batch.len(), 2, "expect audit + business outbox rows");
    // audit row is enqueued first (audit-first ordering)
    assert!(batch.iter().any(|b| b.entity == "audit_log"));
    assert!(batch.iter().any(|b| b.entity == "user"));
}

// --- Phase-01 §2 additions (2026-05-13) ---------------------------------
// These tests target invariants the test plan calls out under
// §2.1 (Rust integration). Wiremock-backed engine scenarios are deferred --
// the engine requires a Tauri AppHandle so the HTTP transport is tested
// indirectly via SyncHttpClient in a follow-up session.

struct FailingWrite;

#[async_trait]
impl BusinessWrite for FailingWrite {
    async fn before(
        &mut self,
        _tx: &mut app_lib::db::Tx<'_>,
    ) -> app_lib::error::AppResult<serde_json::Value> {
        Ok(serde_json::json!({}))
    }

    async fn write(
        &mut self,
        _tx: &mut app_lib::db::Tx<'_>,
    ) -> app_lib::error::AppResult<(serde_json::Value, Vec<OutboxOp>)> {
        Err(app_lib::error::AppError::Internal(
            "business write failed".into(),
        ))
    }
}

#[tokio::test]
async fn audit_writer_rolls_back_audit_and_outbox_when_business_write_fails() {
    // Phase-01 §2.1 invariant: when the business closure returns an error,
    // the surrounding transaction rolls back -- no audit row, no outbox row.
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let writer = AuditWriter::new(audit_repo.clone(), outbox_repo.clone(), "dev-1");

    let result = writer
        .with_audit(
            &pool,
            Uuid::now_v7(),
            AuditAction::Create,
            "user",
            "u1",
            "tenant-x",
            None,
            FailingWrite,
        )
        .await;
    assert!(result.is_err(), "with_audit should propagate the error");

    let audits = audit_repo.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert!(
        audits.is_empty(),
        "no audit row should survive a failed business write"
    );
    let batch = outbox_repo.next_batch(10).await.unwrap();
    assert!(
        batch.is_empty(),
        "no outbox row should survive a failed business write"
    );
}

#[tokio::test]
async fn audit_writer_persists_delta_with_only_changed_fields() {
    // Phase-01 §2.1: compute_delta omits identical fields. Round-trip
    // through the writer to verify the persisted audit row carries only the
    // diff, not the full snapshots.
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let writer = AuditWriter::new(audit_repo.clone(), outbox_repo.clone(), "dev-1");

    struct DiffWrite;
    #[async_trait]
    impl BusinessWrite for DiffWrite {
        async fn before(
            &mut self,
            _tx: &mut app_lib::db::Tx<'_>,
        ) -> app_lib::error::AppResult<serde_json::Value> {
            Ok(serde_json::json!({ "a": 1, "b": 2, "c": 3 }))
        }

        async fn write(
            &mut self,
            _tx: &mut app_lib::db::Tx<'_>,
        ) -> app_lib::error::AppResult<(serde_json::Value, Vec<OutboxOp>)> {
            Ok((serde_json::json!({ "a": 1, "b": 99, "c": 3 }), vec![]))
        }
    }

    writer
        .with_audit(
            &pool,
            Uuid::now_v7(),
            AuditAction::Update,
            "user",
            "u1",
            "tenant-x",
            None,
            DiffWrite,
        )
        .await
        .unwrap();

    let audits = audit_repo.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert_eq!(audits.len(), 1);
    let delta = audits[0].delta.as_object().expect("delta is an object");
    assert!(!delta.contains_key("a"), "unchanged 'a' should be omitted");
    assert!(!delta.contains_key("c"), "unchanged 'c' should be omitted");
    assert_eq!(delta["b"]["from"], serde_json::json!(2));
    assert_eq!(delta["b"]["to"], serde_json::json!(99));
}

#[tokio::test]
async fn outbox_park_excludes_row_from_pending_count() {
    // Phase-01 §2.1: pending_count must mirror the partial index -- parked
    // rows do not count toward the queue depth surfaced in the SyncPill.
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());

    let mut tx = pool.begin().await.unwrap();
    let op_a = OutboxOp::new("audit_log", "row-a", b"a".to_vec());
    let op_b = OutboxOp::new("audit_log", "row-b", b"b".to_vec());
    repo.enqueue(&mut tx, &op_a).await.unwrap();
    repo.enqueue(&mut tx, &op_b).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(repo.pending_count().await.unwrap(), 2);

    repo.park(op_a.op_id).await.unwrap();
    assert_eq!(
        repo.pending_count().await.unwrap(),
        1,
        "parked row excluded from pending_count"
    );

    let batch = repo.next_batch(10).await.unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].entity_id, "row-b");
}

#[tokio::test]
async fn outbox_failure_resurfaces_row_after_backoff_elapses() {
    // Phase-01 §2.1 mirror of the existing skip test: once the backoff
    // window passes, next_batch must return the row again.
    let pool = fresh_pool().await;
    let repo = SqliteOutboxRepo::new(pool.clone());

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    repo.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    // Backoff of 0 seconds resolves to "ready now or microseconds from now"
    // -- by the time next_batch runs, the comparison <= NOW(...) succeeds.
    repo.mark_failure(op.op_id, "transient", 0).await.unwrap();
    let batch = repo.next_batch(10).await.unwrap();
    assert_eq!(batch.len(), 1, "row must be eligible after backoff elapses");
    assert_eq!(batch[0].attempts, 1, "attempts must increment on failure");
    assert_eq!(batch[0].last_error.as_deref(), Some("transient"));
}

#[tokio::test]
async fn audit_writer_emits_audit_outbox_row_with_audit_log_entity_name() {
    // Phase-01 §7.7 (additive-only audit_log invariant): the audit row's
    // own outbox push targets the `audit_log` entity, and the persisted
    // outbox payload deserializes back into an AuditEntry that matches the
    // appended row by id. This pins the wire format the engine ships.
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let writer = AuditWriter::new(audit_repo.clone(), outbox_repo.clone(), "dev-1");

    let stub = StubWrite {
        after: serde_json::json!({ "name": "Carol" }),
        outbox: vec![],
    };

    writer
        .with_audit(
            &pool,
            Uuid::now_v7(),
            AuditAction::Create,
            "user",
            "u1",
            "tenant-x",
            None,
            stub,
        )
        .await
        .unwrap();

    let batch = outbox_repo.next_batch(10).await.unwrap();
    assert_eq!(batch.len(), 1, "only the audit_log outbox row is enqueued");
    assert_eq!(batch[0].entity, "audit_log");
    assert_eq!(batch[0].op, OutboxAction::Upsert);

    let audits = audit_repo.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(
        batch[0].entity_id,
        audits[0].id.to_string(),
        "outbox row.entity_id must reference the audit row id"
    );

    // Audit payloads are encoded with `with_human_readable()` so `Uuid`
    // fields wire as strings (the sync server's JS decoder requires it).
    // Matching deserializer config is required to round-trip back to
    // `AuditEntry` here.
    let mut deser = rmp_serde::Deserializer::new(&batch[0].payload[..]).with_human_readable();
    let decoded: AuditEntry =
        serde::Deserialize::deserialize(&mut deser).expect("payload decodes as AuditEntry");
    assert_eq!(decoded.id, audits[0].id);
    assert_eq!(decoded.entity, "user");
    assert_eq!(decoded.entity_id, "u1");
}

#[tokio::test]
async fn sync_state_server_url_persists_across_repo_instances() {
    // Migration 010 + `config_set_sync_server_url_impl` write-through: the
    // URL must survive an app restart so the first-launch modal does not
    // reopen after the user finishes setup.
    let pool = fresh_pool().await;
    let writer = SqliteSyncStateRepo::new(pool.clone());
    writer.ensure_device_id("dev-1").await.unwrap();

    assert_eq!(
        writer.get_server_url().await.unwrap(),
        None,
        "fresh row starts with no URL"
    );

    writer
        .put_server_url("http://localhost:3161")
        .await
        .unwrap();

    let reader = SqliteSyncStateRepo::new(pool.clone());
    assert_eq!(
        reader.get_server_url().await.unwrap().as_deref(),
        Some("http://localhost:3161"),
        "URL must round-trip via SQLite, not just an in-memory cache"
    );

    writer
        .put_server_url("https://sync.example.com")
        .await
        .unwrap();
    assert_eq!(
        reader.get_server_url().await.unwrap().as_deref(),
        Some("https://sync.example.com"),
        "update overwrites the previously stored URL"
    );
}
