//! Tauri commands exposed by the sync bounded context.

use serde::Serialize;
use tauri::State;
use tracing::instrument;

use crate::domains::sync::domain::value_objects::SyncStatus;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatusSnapshot {
    pub status: SyncStatus,
    pub pending_ops: u32,
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_status(state: State<'_, AppState>) -> AppResult<SyncStatusSnapshot> {
    let engine = state.sync_engine();
    let status = engine.status().await;
    let pending_ops = engine.outbox_repo().pending_count().await?;
    Ok(SyncStatusSnapshot {
        status,
        pending_ops,
    })
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_outbox_count(state: State<'_, AppState>) -> AppResult<u32> {
    state.sync_engine().outbox_repo().pending_count().await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_trigger_push(state: State<'_, AppState>) -> AppResult<()> {
    state.sync_engine().trigger_push().await;
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_trigger_pull(state: State<'_, AppState>) -> AppResult<()> {
    state.sync_engine().trigger_pull().await;
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_list_conflicts(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<serde_json::Value>> {
    // Phase 1 ships a placeholder list. Real conflict storage lives on the
    // server and is fetched on demand by the Phase-8 resolver UI.
    let _ = (state, limit, offset);
    Ok(vec![])
}

#[derive(Debug, serde::Deserialize)]
pub struct ResolveConflictArgs {
    pub op_id: String,
    pub choice: String,
    #[serde(default)]
    pub merged: Option<serde_json::Value>,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn sync_resolve_conflict(
    state: State<'_, AppState>,
    args: ResolveConflictArgs,
) -> AppResult<()> {
    if args.choice != "local" && args.choice != "server" && args.choice != "merged" {
        return Err(crate::error::AppError::Validation(format!(
            "invalid choice: {}",
            args.choice
        )));
    }
    state
        .sync_engine()
        .resolve_conflict(args.op_id, args.choice, args.merged)
        .await
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub app_version: String,
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn device_info(state: State<'_, AppState>) -> AppResult<DeviceInfo> {
    Ok(DeviceInfo {
        device_id: state.device_id().to_string(),
        app_version: state.app_version().to_string(),
    })
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_set_sync_server_url(state: State<'_, AppState>, url: String) -> AppResult<()> {
    if url.trim().is_empty() {
        return Err(crate::error::AppError::Validation(
            "sync server url required".into(),
        ));
    }
    state.set_sync_server_url(url.clone()).await;
    state.sync_engine().set_server_url(url).await;
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_get_sync_server_url(state: State<'_, AppState>) -> AppResult<Option<String>> {
    Ok(state.sync_server_url().await)
}
