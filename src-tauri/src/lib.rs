//! IDC System -- Tauri application library.
//!
//! Modes:
//! - **Standalone**: normal Tauri window app with full sync engine.
//! - **Embedded**: headless mode for Business OS integration (auth only).

pub mod config;
pub mod db;
pub mod domains;
pub mod embedded;
pub mod error;
pub mod state;
pub mod sync;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::db::migrations;
use crate::domains::sync::commands::{
    config_get_sync_server_url, config_set_sync_server_url, device_info, sync_list_conflicts,
    sync_outbox_count, sync_resolve_conflict, sync_status, sync_trigger_pull, sync_trigger_push,
};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use crate::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use crate::state::AppState;
use crate::sync::{SyncEngine, SyncEngineHandle};

/// Standalone-mode embedded flag (PRD §5.3 / phase-01 §7.35).
///
/// IDC's Business OS embedded code path is gated by `IDC_EMBEDDED_MODE=1`.
/// Shipped builds explicitly set this to `0` so the embedded subsystem stays
/// inert.
fn embedded_mode_enabled() -> bool {
    std::env::var("IDC_EMBEDDED_MODE").unwrap_or_else(|_| "0".into()) == "1"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if embedded::is_embedded_mode() {
        if !embedded_mode_enabled() {
            tracing::info!("embedded_mode=disabled (IDC_EMBEDDED_MODE != 1)");
        }
        eprintln!("[STARTUP] Embedded mode detected (TORCH_EMBEDDED_MODE=true)");
        match embedded::EmbeddedConfig::from_env() {
            Ok(embedded_config) => {
                let rt = tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime for embedded mode");
                if let Err(e) = rt.block_on(embedded::run_embedded(embedded_config)) {
                    eprintln!("[ERROR] Embedded mode failed: {e}");
                    std::process::exit(1);
                }
                return;
            }
            Err(e) => {
                eprintln!("[ERROR] Invalid embedded mode configuration: {e}");
                eprintln!("[ERROR] Required env vars: TORCH_IPC_PORT, TORCH_RUN_ID");
                std::process::exit(1);
            }
        }
    }

    tracing::info!("embedded_mode=disabled");
    eprintln!("[STARTUP] Running in standalone mode");

    let cancel = CancellationToken::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup({
            let cancel = cancel.clone();
            move |app| {
                if cfg!(debug_assertions) {
                    app.handle().plugin(
                        tauri_plugin_log::Builder::default()
                            .level(log::LevelFilter::Info)
                            .build(),
                    )?;
                }

                let handle = app.handle().clone();
                let cancel = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = bootstrap(&handle, cancel).await {
                        error!(error = %e, "bootstrap failed");
                    }
                });
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            sync_status,
            sync_outbox_count,
            sync_trigger_push,
            sync_trigger_pull,
            sync_list_conflicts,
            sync_resolve_conflict,
            device_info,
            config_set_sync_server_url,
            config_get_sync_server_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn bootstrap(
    app: &tauri::AppHandle,
    cancel: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db_path = resolve_db_path(app)?;
    info!(path = ?db_path, "opening local database");
    let pool = crate::db::init_pool(&db_path).await?;
    migrations::run(&pool).await?;

    let device_id = resolve_device_id(&pool).await?;
    let app_version = app.package_info().version.to_string();
    let entity_id_tenant = "unscoped".to_string();
    let initial_server_url = std::env::var("IDC_SYNC_SERVER_URL").ok();

    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let engine_handle: SyncEngineHandle = SyncEngine::spawn(
        crate::sync::engine::SyncEngineConfig {
            pool: pool.clone(),
            outbox_repo,
            audit_repo,
            state_repo,
            device_id: device_id.clone(),
            app_version: app_version.clone(),
            initial_server_url: initial_server_url.clone(),
            initial_token: None,
            entity_id_tenant: entity_id_tenant.clone(),
        },
        app.clone(),
        cancel.clone(),
    );

    let state = AppState::new(
        pool,
        engine_handle,
        device_id,
        app_version,
        initial_server_url,
    );
    app.manage(state);

    info!("bootstrap complete");
    Ok(())
}

fn resolve_db_path(
    app: &tauri::AppHandle,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("idc-local.db"))
}

async fn resolve_device_id(
    pool: &sqlx::SqlitePool,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let repo = SqliteSyncStateRepo::new(pool.clone());
    let candidate = Uuid::now_v7().to_string();
    let device_id = repo.ensure_device_id(&candidate).await?;
    Ok(device_id)
}
