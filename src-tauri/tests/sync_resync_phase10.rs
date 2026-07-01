//! Resync sweep (`sync_resync_local`) integration tests.
//!
//! The resync command re-enqueues EVERY syncable local row into the outbox for
//! a full re-push, so a server that lost already-synced rows can be brought
//! back into convergence. The critical, non-obvious guarantees under test:
//!
//! 1. It includes `dirty = 0` (already-synced) rows -- the whole reason it
//!    exists. The normal write path only enqueues at mutation time and never
//!    re-derives an op for a clean row.
//! 2. It enqueues one op per row.
//! 3. It enqueues in FK-dependency (APPLY_ORDER) order, so a parent entity
//!    (`users`) is drained before a child (`settings` here stands in as a later
//!    APPLY_ORDER entity; the ordering machinery is the same for every entity).
//!
//! We drive `sync_resync_local_impl` against an AppState built via
//! `AppState::for_phase02_tests`, which wires the users + settings services --
//! two entities at distinct APPLY_ORDER ranks (users = 0, settings = 1). The
//! catalog/patients/visits paths share the identical enqueue+ordering code.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::{AuthService, UserService};
use app_lib::domains::settings::infrastructure::SqliteSettingRepo;
use app_lib::domains::settings::service::SettingsService;
use app_lib::domains::sync::commands::sync_resync_local_impl;
use app_lib::domains::sync::domain::repositories::OutboxRepo;
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use app_lib::state::AppState;
use app_lib::sync::{SyncEngine, SyncEngineConfig, SyncEngineHandle};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::mock_app;
use tokio_util::sync::CancellationToken;

const DEVICE_ID: &str = "dev-A";
const ENTITY_ID: &str = "tenant-1";

struct Rig {
    state: AppState,
    pool: SqlitePool,
    outbox_repo: Arc<dyn OutboxRepo>,
    _app: tauri::App<tauri::test::MockRuntime>,
    _cancel: CancellationToken,
}

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

async fn rig() -> Rig {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let setting_repo = Arc::new(SqliteSettingRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state_repo = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        DEVICE_ID.into(),
    ));
    let user_service = Arc::new(UserService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        DEVICE_ID.into(),
    ));
    let settings_service = Arc::new(SettingsService::new(
        pool.clone(),
        setting_repo,
        audit_repo.clone(),
        outbox_repo.clone(),
        DEVICE_ID.into(),
    ));

    let mock = mock_app();
    let handle = mock.handle().clone();
    let cancel = CancellationToken::new();
    // No server URL: `trigger_push` at the end of the resync is a safe no-op,
    // so the test never touches the network.
    let engine: SyncEngineHandle = SyncEngine::spawn(
        SyncEngineConfig {
            pool: pool.clone(),
            outbox_repo: outbox_repo.clone(),
            audit_repo: audit_repo.clone(),
            state_repo,
            device_id: DEVICE_ID.into(),
            app_version: "0.1.0".into(),
            initial_server_url: None,
            initial_token: None,
            entity_id_tenant: ENTITY_ID.into(),
            refresh_hook: None,
        },
        handle,
        cancel.clone(),
    );

    // Seed a first admin so there is at least one `users` row to resync.
    auth_service
        .create_first_admin("admin@idc.io", "Mariam", "admin-strong-789", ENTITY_ID)
        .await
        .unwrap();

    let state = AppState::for_phase02_tests(
        pool.clone(),
        engine,
        auth_service,
        user_service,
        settings_service,
        user_repo.clone(),
        DEVICE_ID.into(),
        "0.1.0".into(),
        None,
    );

    Rig {
        state,
        pool,
        outbox_repo,
        _app: mock,
        _cancel: cancel,
    }
}

/// Simulate the post-sync steady state the resync is designed to recover from:
/// every row already pushed (`dirty = 0`) and the outbox empty. Without this,
/// the seeded admin would leave a stray write-path op in the outbox and mask
/// the "resync picks up clean rows" guarantee.
async fn mark_all_synced_and_drain_outbox(pool: &SqlitePool) {
    sqlx::query("UPDATE users SET dirty = 0")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE settings SET dirty = 0")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM outbox")
        .execute(pool)
        .await
        .unwrap();
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap();
    n
}

#[tokio::test]
async fn resync_reenqueues_all_clean_rows_one_op_per_row() {
    let r = rig().await;
    mark_all_synced_and_drain_outbox(&r.pool).await;

    // Precondition: nothing pending -- the normal write path has no way to
    // re-enqueue these already-synced rows.
    assert_eq!(r.outbox_repo.pending_count().await.unwrap(), 0);

    let n_users = count(&r.pool, "users").await as u64;
    let n_settings = count(&r.pool, "settings").await as u64;
    assert!(n_users >= 1, "expected at least the seeded admin");
    assert!(n_settings >= 1, "migrations seed default settings rows");

    let summary = sync_resync_local_impl(&r.state).await.expect("resync ok");

    // One op per row, total across both entities.
    assert_eq!(summary.total, n_users + n_settings);

    let users_count = summary
        .per_entity
        .iter()
        .find(|(e, _)| e == "users")
        .map(|(_, c)| *c)
        .expect("users in summary");
    let settings_count = summary
        .per_entity
        .iter()
        .find(|(e, _)| e == "settings")
        .map(|(_, c)| *c)
        .expect("settings in summary");
    assert_eq!(users_count, n_users);
    assert_eq!(settings_count, n_settings);

    // The outbox now holds exactly one op per row.
    assert_eq!(
        r.outbox_repo.pending_count().await.unwrap() as u64,
        n_users + n_settings
    );
}

#[tokio::test]
async fn resync_enqueues_parents_before_children_in_apply_order() {
    let r = rig().await;
    mark_all_synced_and_drain_outbox(&r.pool).await;

    sync_resync_local_impl(&r.state).await.expect("resync ok");

    // `next_batch` drains in creation (`created_at`, `op_id`) order -- the same
    // order the push loop uses. Every `users` op must precede every `settings`
    // op so FK parents land on the server first.
    let batch = r.outbox_repo.next_batch(10_000).await.unwrap();
    let entities: Vec<&str> = batch.iter().map(|op| op.entity.as_str()).collect();

    let last_user = entities.iter().rposition(|e| *e == "users");
    let first_setting = entities.iter().position(|e| *e == "settings");
    if let (Some(lu), Some(fs)) = (last_user, first_setting) {
        assert!(
            lu < fs,
            "all `users` ops must be enqueued before any `settings` op (APPLY_ORDER); \
             got entity sequence: {entities:?}"
        );
    }

    // Every enqueued op is an upsert carrying a non-empty payload keyed by the
    // row id (the resync never emits empty ops).
    for op in &batch {
        assert!(!op.entity_id.is_empty(), "op has an entity_id");
        assert!(!op.payload.is_empty(), "op carries a payload");
    }
}

#[tokio::test]
async fn resync_is_idempotent_across_repeated_runs() {
    let r = rig().await;
    mark_all_synced_and_drain_outbox(&r.pool).await;

    let first = sync_resync_local_impl(&r.state).await.expect("resync 1");
    // A second run mints a whole fresh set of ops (fresh op_ids) -- re-running
    // is safe because the server dedupes by op_id and upserts by row id.
    let second = sync_resync_local_impl(&r.state).await.expect("resync 2");

    assert_eq!(first.total, second.total);
    // Both runs' ops accumulate in the outbox (no dedupe against existing ops).
    assert_eq!(
        r.outbox_repo.pending_count().await.unwrap() as u64,
        first.total + second.total
    );

    // All op_ids are distinct across the two runs.
    let batch = r.outbox_repo.next_batch(1_000_000).await.unwrap();
    let mut ids: Vec<String> = batch.iter().map(|op| op.op_id.to_string()).collect();
    let total_ops = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total_ops, "every outbox op_id is unique");
}
