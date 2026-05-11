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
