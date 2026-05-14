//! Phase-01 §5 persona script: P3 Mariam (superadmin) -- foundational
//! sync plumbing day.
//!
//! Mariam's phase-01 surface is the sync engine's control plane. This
//! script walks every IPC the phase ships, end-to-end, asserting the
//! invariants the plan §5 calls out for the canonical persona:
//!
//! 1. App boots clean -- sync_status reports `idle` with zero pending ops.
//! 2. Mariam configures the sync server URL.
//! 3. Mariam triggers a manual push -- no error, even with an empty queue.
//! 4. A new audit row is enqueued (simulating any phase-2+ business write
//!    going through AuditWriter::with_audit).
//! 5. sync_outbox_count + sync_status reflect the new outbox depth.
//! 6. Mariam opens the conflict list -- empty (no server configured to
//!    reach in test, returns Ok(vec![]) per engine offline behaviour).
//! 7. Mariam attempts an invalid conflict resolution -- typed error.
//! 8. Mariam attempts a legal resolution -- engine reports
//!    SERVER_UNAVAILABLE (no real server), proving the validation gate
//!    passed and only the network path failed.
//! 9. Mariam checks device_info -- gets the persisted device_id + version.
//! 10. Mariam updates the server URL -- get_sync_server_url reflects it.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use app_lib::db::migrations;
use app_lib::domains::sync::commands::{
    config_get_sync_server_url_impl, config_set_sync_server_url_impl, device_info_impl,
    sync_list_conflicts_impl, sync_outbox_count_impl, sync_resolve_conflict_impl, sync_status_impl,
    sync_trigger_pull_impl, sync_trigger_push_impl, ResolveConflictArgs,
};
use app_lib::domains::sync::domain::entities::OutboxOp;
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use app_lib::domains::sync::domain::value_objects::SyncStatus;
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use app_lib::state::AppState;
use app_lib::sync::{SyncEngine, SyncEngineConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::mock_app;
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

#[tokio::test]
async fn mariam_superadmin_phase01_day_script() {
    // --- Boot sequence -------------------------------------------------
    let pool = fresh_pool().await;
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mock = mock_app();
    let handle = mock.handle().clone();
    let cancel = CancellationToken::new();

    let engine = SyncEngine::spawn(
        SyncEngineConfig {
            pool: pool.clone(),
            outbox_repo: outbox_repo.clone(),
            audit_repo,
            state_repo,
            device_id: "mariam-laptop".into(),
            app_version: "0.1.0".into(),
            initial_server_url: None,
            initial_token: None,
            entity_id_tenant: "tenant-idc".into(),
        },
        handle,
        cancel.clone(),
    );

    let state = AppState::for_sync_tests(
        pool.clone(),
        engine,
        "mariam-laptop".into(),
        "0.1.0".into(),
        None,
    );

    // --- Step 1: app boots clean --------------------------------------
    let snap = sync_status_impl(&state).await.expect("status ok at boot");
    assert_eq!(snap.status, SyncStatus::Idle);
    assert_eq!(snap.pending_ops, 0);

    // --- Step 2: device_info available even pre-server-config ---------
    let info = device_info_impl(&state).await.unwrap();
    assert_eq!(info.device_id, "mariam-laptop");
    assert_eq!(info.app_version, "0.1.0");

    // --- Step 3: pre-config, list_conflicts is empty (no HTTP client) -
    // The engine returns Ok(vec![]) when no server URL has been
    // configured yet -- the resolver page renders the empty state.
    let conflicts = sync_list_conflicts_impl(&state, Some(100), Some(0))
        .await
        .expect("offline list conflicts is empty, not an error");
    assert!(conflicts.is_empty());

    // --- Step 4: pre-config, get returns None -------------------------
    let stored = config_get_sync_server_url_impl(&state).await.unwrap();
    assert!(stored.is_none(), "fresh install has no server URL");

    // --- Step 5: invalid conflict resolution returns typed error
    // (this fires BEFORE the engine call, so it's deterministic
    // regardless of whether a server is configured.)
    let err = sync_resolve_conflict_impl(
        &state,
        ResolveConflictArgs {
            op_id: "op-x".into(),
            choice: "ignore-it".into(),
            merged: None,
        },
    )
    .await
    .expect_err("bad choice must error");
    assert_eq!(err.code(), "VALIDATION_ERROR");

    // --- Step 6: legal choice without server -> SERVER_UNAVAILABLE ----
    // Validation gate passed; engine reports no server configured.
    let err = sync_resolve_conflict_impl(
        &state,
        ResolveConflictArgs {
            op_id: "op-x".into(),
            choice: "local".into(),
            merged: None,
        },
    )
    .await
    .expect_err("no server configured -> SyncUnavailable");
    assert_eq!(err.code(), "SERVER_UNAVAILABLE");

    // --- Step 7: configure the sync server URL ------------------------
    config_set_sync_server_url_impl(&state, "https://sync.idc.iq".into())
        .await
        .expect("set URL ok");
    let stored = config_get_sync_server_url_impl(&state).await.unwrap();
    assert_eq!(stored.as_deref(), Some("https://sync.idc.iq"));

    // --- Step 8: manual push / pull triggers -- both infallible -------
    // Even with the (unreachable) server now configured, the trigger
    // commands return Ok immediately; the engine surfaces network state
    // through sync:status events, not via the trigger return value.
    sync_trigger_push_impl(&state)
        .await
        .expect("trigger push ok");
    sync_trigger_pull_impl(&state)
        .await
        .expect("trigger pull ok");

    // --- Step 9: enqueue a phase-2-style audit row directly -----------
    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "audit-row-1", b"snapshot".to_vec());
    outbox_repo.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    // --- Step 10: sync_outbox_count + sync_status reflect the depth ---
    let count = sync_outbox_count_impl(&state).await.unwrap();
    assert_eq!(count, 1);
    let snap = sync_status_impl(&state).await.unwrap();
    assert_eq!(snap.pending_ops, 1);

    // Drain engine's mpsc so spawned task processes pending cmds before
    // the test exits (avoids the "channel dropped" log line).
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();
}
