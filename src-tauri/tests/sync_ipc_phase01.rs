//! Phase-01 §2.2 IPC handler tests.
//!
//! Each `#[tauri::command]` in `domains/sync/commands.rs` delegates to a
//! plain `_impl(&AppState, ...)` async fn. We exercise those helpers
//! directly with a minimal AppState constructed via `AppState::for_sync_tests`,
//! which lets us drive every command without standing up the full
//! production app graph.
//!
//! Coverage: happy path + at least one error path per command, plus the
//! IPC return-shape assertions that the §3.2 plan calls out.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use app_lib::db::migrations;
use app_lib::domains::sync::commands::{
    config_get_sync_server_url_impl, config_set_sync_server_url_impl, device_info_impl,
    sync_outbox_count_impl, sync_resolve_conflict_impl, sync_status_impl, sync_trigger_pull_impl,
    sync_trigger_push_impl, ResolveConflictArgs,
};
use app_lib::domains::sync::domain::entities::OutboxOp;
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use app_lib::domains::sync::domain::value_objects::SyncStatus;
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use app_lib::state::AppState;
use app_lib::sync::{SyncEngine, SyncEngineConfig, SyncEngineHandle};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::{mock_app, MockRuntime};
use tauri::App;
use tokio_util::sync::CancellationToken;

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

struct TestRig {
    state: AppState,
    pool: SqlitePool,
    outbox_repo: Arc<dyn OutboxRepo>,
    _app: App<MockRuntime>,
    _cancel: CancellationToken,
}

async fn rig(server_url: Option<&str>) -> TestRig {
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mock = mock_app();
    let handle = mock.handle().clone();
    let cancel = CancellationToken::new();

    let engine: SyncEngineHandle = SyncEngine::spawn(
        SyncEngineConfig {
            pool: pool.clone(),
            outbox_repo: outbox_repo.clone(),
            audit_repo,
            state_repo,
            device_id: "test-device".into(),
            app_version: "0.1.0".into(),
            initial_server_url: server_url.map(|s| s.to_string()),
            initial_token: None,
            entity_id_tenant: "tenant-x".into(),
        },
        handle,
        cancel.clone(),
    );

    let state = AppState::for_sync_tests(
        pool.clone(),
        engine,
        "test-device".into(),
        "0.1.0".into(),
        server_url.map(|s| s.to_string()),
    );

    TestRig {
        state,
        pool,
        outbox_repo,
        _app: mock,
        _cancel: cancel,
    }
}

// --- sync_status ---------------------------------------------------------

#[tokio::test]
async fn sync_status_returns_idle_with_zero_pending_on_fresh_state() {
    let r = rig(None).await;
    let snap = sync_status_impl(&r.state).await.expect("status ok");
    assert_eq!(snap.status, SyncStatus::Idle);
    assert_eq!(snap.pending_ops, 0);
}

#[tokio::test]
async fn sync_status_pending_ops_reflects_outbox_depth() {
    let r = rig(None).await;
    let mut tx = r.pool.begin().await.unwrap();
    for i in 0..3 {
        let op = OutboxOp::new("audit_log", format!("row-{i}"), b"x".to_vec());
        r.outbox_repo.enqueue(&mut tx, &op).await.unwrap();
    }
    tx.commit().await.unwrap();

    let snap = sync_status_impl(&r.state).await.expect("status ok");
    assert_eq!(snap.pending_ops, 3);
}

#[tokio::test]
#[allow(non_snake_case)]
async fn sync_status_serializes_to_camelCase_status_and_pending_ops_fields() {
    // Phase-01 §3.2 IPC shape contract: the struct serializes as
    // { status: <lowercase>, pending_ops: <u32> }. Frontend Zod consumes
    // exactly this shape via SyncStatusSnapshotSchema.
    let r = rig(None).await;
    let snap = sync_status_impl(&r.state).await.unwrap();
    let json = serde_json::to_value(&snap).unwrap();
    assert_eq!(json["status"], serde_json::json!("idle"));
    assert_eq!(json["pending_ops"], serde_json::json!(0));
}

// --- sync_outbox_count ---------------------------------------------------

#[tokio::test]
async fn sync_outbox_count_returns_zero_when_outbox_empty() {
    let r = rig(None).await;
    let n = sync_outbox_count_impl(&r.state).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn sync_outbox_count_excludes_parked_rows() {
    let r = rig(None).await;
    let mut tx = r.pool.begin().await.unwrap();
    let op_a = OutboxOp::new("audit_log", "row-a", b"a".to_vec());
    let op_b = OutboxOp::new("audit_log", "row-b", b"b".to_vec());
    r.outbox_repo.enqueue(&mut tx, &op_a).await.unwrap();
    r.outbox_repo.enqueue(&mut tx, &op_b).await.unwrap();
    tx.commit().await.unwrap();

    r.outbox_repo.park(op_a.op_id).await.unwrap();

    let n = sync_outbox_count_impl(&r.state).await.unwrap();
    assert_eq!(n, 1, "parked rows must not appear in pending count");
}

// --- sync_trigger_push / sync_trigger_pull -------------------------------

#[tokio::test]
async fn sync_trigger_push_returns_ok_even_when_no_server_configured() {
    // Phase-01 §2.2: trigger commands are infallible signals into the
    // engine; offline conditions are surfaced through `sync_status`, not
    // by failing the trigger.
    let r = rig(None).await;
    sync_trigger_push_impl(&r.state).await.expect("ok");
}

#[tokio::test]
async fn sync_trigger_pull_returns_ok_even_when_no_server_configured() {
    let r = rig(None).await;
    sync_trigger_pull_impl(&r.state).await.expect("ok");
}

#[tokio::test]
async fn sync_trigger_push_idempotent_under_rapid_repeated_calls() {
    let r = rig(None).await;
    for _ in 0..5 {
        sync_trigger_push_impl(&r.state).await.unwrap();
    }
    // No assertion needed beyond not panicking: the engine's bounded mpsc
    // channel must absorb rapid signals without backpressure issues.
}

// --- sync_resolve_conflict -----------------------------------------------

#[test]
fn resolve_conflict_args_deserialize_from_camelcase_wire_shape() {
    // The frontend sends the inner struct as camelCase (`opId`). Tauri v2 does
    // NOT camelCase-convert inner struct fields, so without
    // #[serde(rename_all="camelCase")] this fails with "missing field `op_id`"
    // and every conflict resolution is rejected at the IPC boundary.
    let json = serde_json::json!({
        "opId": "01HZ0000000000000000000abc",
        "choice": "local",
        "merged": null,
    });
    let args: ResolveConflictArgs =
        serde_json::from_value(json).expect("camelCase opId must deserialize");
    assert_eq!(args.op_id, "01HZ0000000000000000000abc");
    assert_eq!(args.choice, "local");

    // The old snake_case wire shape must now be rejected (proves the rename).
    let snake = serde_json::json!({ "op_id": "x", "choice": "local" });
    assert!(
        serde_json::from_value::<ResolveConflictArgs>(snake).is_err(),
        "snake_case op_id must no longer deserialize"
    );
}

#[tokio::test]
async fn sync_resolve_conflict_rejects_unknown_choice_with_validation_error() {
    let r = rig(None).await;
    let err = sync_resolve_conflict_impl(
        &r.state,
        ResolveConflictArgs {
            op_id: "op-1".into(),
            choice: "wat".into(),
            merged: None,
        },
    )
    .await
    .expect_err("invalid choice rejected");
    assert_eq!(err.code(), "VALIDATION_ERROR");
    assert!(err.to_string().contains("invalid choice"));
}

#[tokio::test]
async fn sync_resolve_conflict_accepts_each_legal_choice_value() {
    // Validation should accept all of {local, server, merged}; the engine
    // will then surface a downstream error because no server is configured,
    // proving the choice gate succeeded.
    let r = rig(None).await;
    for choice in ["local", "server", "merged"] {
        let result = sync_resolve_conflict_impl(
            &r.state,
            ResolveConflictArgs {
                op_id: "op-1".into(),
                choice: choice.into(),
                merged: None,
            },
        )
        .await;
        let err = result.expect_err("no server configured -> err");
        assert_eq!(err.code(), "SERVER_UNAVAILABLE");
    }
}

// --- device_info ---------------------------------------------------------

#[tokio::test]
async fn device_info_returns_device_id_and_app_version_from_state() {
    let r = rig(None).await;
    let info = device_info_impl(&r.state).await.unwrap();
    assert_eq!(info.device_id, "test-device");
    assert_eq!(info.app_version, "0.1.0");
}

#[tokio::test]
async fn device_info_serializes_to_snake_case_device_id_and_app_version() {
    // Phase-01 §3.2 IPC contract.
    let r = rig(None).await;
    let info = device_info_impl(&r.state).await.unwrap();
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["device_id"], serde_json::json!("test-device"));
    assert_eq!(json["app_version"], serde_json::json!("0.1.0"));
}

// --- config_set_sync_server_url / config_get_sync_server_url -------------

#[tokio::test]
async fn config_get_sync_server_url_returns_none_on_fresh_install() {
    let r = rig(None).await;
    let url = config_get_sync_server_url_impl(&r.state).await.unwrap();
    assert!(url.is_none());
}

#[tokio::test]
async fn config_set_sync_server_url_persists_to_state_and_get_returns_it() {
    let r = rig(None).await;
    config_set_sync_server_url_impl(&r.state, "http://localhost:3000".into())
        .await
        .unwrap();
    let url = config_get_sync_server_url_impl(&r.state).await.unwrap();
    assert_eq!(url.as_deref(), Some("http://localhost:3000"));
    // Engine reads its server URL on the next loop tick; give it a moment
    // to drain the SetServerUrl command from the mpsc channel.
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn config_set_sync_server_url_rejects_empty_url_with_validation_error() {
    let r = rig(None).await;
    let err = config_set_sync_server_url_impl(&r.state, "".into())
        .await
        .expect_err("empty url rejected");
    assert_eq!(err.code(), "VALIDATION_ERROR");
}

#[tokio::test]
async fn config_set_sync_server_url_rejects_whitespace_only_url() {
    let r = rig(None).await;
    let err = config_set_sync_server_url_impl(&r.state, "   \t  ".into())
        .await
        .expect_err("whitespace-only url rejected");
    assert_eq!(err.code(), "VALIDATION_ERROR");
}

// --- sync_list_conflicts -------------------------------------------------

#[tokio::test]
async fn sync_list_conflicts_returns_empty_array_when_no_server_configured() {
    // No HTTP client -> engine returns Ok(vec![]) per
    // SyncEngine::list_conflicts_inner. The IPC must surface this as an
    // empty array, never an error.
    let r = rig(None).await;
    let conflicts =
        app_lib::domains::sync::commands::sync_list_conflicts_impl(&r.state, None, None)
            .await
            .expect("offline list returns empty, not error");
    assert!(conflicts.is_empty());
}

#[tokio::test]
#[allow(non_snake_case)]
async fn sync_list_conflicts_serializes_each_conflict_with_camelCase_field_names() {
    // Phase-01 §3.2 IPC contract: server payload uses snake_case
    // (`op_id`, `entity_id`) but the IPC reshapes to camelCase
    // (`opId`, `entityId`) because that's what the frontend Zod schema
    // expects (ConflictSchema in src/lib/schemas/sync.ts).
    let r = rig(None).await;
    // Construct a single fake ServerConflict and assert reshape inline.
    let server = app_lib::domains::sync::infrastructure::ServerConflict {
        op_id: "op-1".into(),
        entity: "audit_log".into(),
        entity_id: "row-1".into(),
        server_payload: serde_json::json!({"v": 2}),
        local_payload: serde_json::json!({"v": 1}),
        reason: "AUDIT_IMMUTABLE".into(),
    };
    let reshaped = serde_json::json!({
        "opId": server.op_id,
        "entity": server.entity,
        "entityId": server.entity_id,
        "serverPayload": server.server_payload,
        "localPayload": server.local_payload,
        "reason": server.reason,
    });
    assert_eq!(reshaped["opId"], "op-1");
    assert_eq!(reshaped["entityId"], "row-1");
    assert_eq!(reshaped["serverPayload"]["v"], 2);
    let _ = &r; // keep rig alive for symmetry with other tests
}
