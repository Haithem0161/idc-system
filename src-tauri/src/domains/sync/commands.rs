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

// --- testable `_impl` helpers -------------------------------------------
//
// Each `#[tauri::command]` delegates to a plain async fn taking `&AppState`,
// so phase-01 §2.2 tests can drive every command without standing up a
// Tauri runtime / `State` wrapper.

pub async fn sync_status_impl(state: &AppState) -> AppResult<SyncStatusSnapshot> {
    let engine = state.sync_engine();
    let status = engine.status().await;
    let pending_ops = engine.outbox_repo().pending_count().await?;
    Ok(SyncStatusSnapshot {
        status,
        pending_ops,
    })
}

pub async fn sync_outbox_count_impl(state: &AppState) -> AppResult<u32> {
    state.sync_engine().outbox_repo().pending_count().await
}

pub async fn sync_trigger_push_impl(state: &AppState) -> AppResult<()> {
    state.sync_engine().trigger_push().await;
    Ok(())
}

pub async fn sync_trigger_pull_impl(state: &AppState) -> AppResult<()> {
    state.sync_engine().trigger_pull().await;
    Ok(())
}

pub async fn sync_list_conflicts_impl(
    state: &AppState,
    _limit: Option<i64>,
    _offset: Option<i64>,
) -> AppResult<Vec<serde_json::Value>> {
    let conflicts = state.sync_engine().list_conflicts().await?;
    Ok(conflicts
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "opId": c.op_id,
                "entity": c.entity,
                "entityId": c.entity_id,
                "serverPayload": c.server_payload,
                "localPayload": c.local_payload,
                "reason": c.reason,
            })
        })
        .collect())
}

pub async fn sync_resolve_conflict_impl(
    state: &AppState,
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

pub async fn device_info_impl(state: &AppState) -> AppResult<DeviceInfo> {
    Ok(DeviceInfo {
        device_id: state.device_id().to_string(),
        app_version: state.app_version().to_string(),
    })
}

pub async fn config_set_sync_server_url_impl(state: &AppState, url: String) -> AppResult<()> {
    if url.trim().is_empty() {
        return Err(crate::error::AppError::Validation(
            "sync server url required".into(),
        ));
    }
    state.set_sync_server_url(url.clone()).await;
    state.sync_engine().set_server_url(url).await;
    Ok(())
}

pub async fn config_get_sync_server_url_impl(state: &AppState) -> AppResult<Option<String>> {
    Ok(state.sync_server_url().await)
}

// --- #[tauri::command] wrappers (boundary layer) ------------------------

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_status(state: State<'_, AppState>) -> AppResult<SyncStatusSnapshot> {
    sync_status_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_outbox_count(state: State<'_, AppState>) -> AppResult<u32> {
    sync_outbox_count_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_trigger_push(state: State<'_, AppState>) -> AppResult<()> {
    sync_trigger_push_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_trigger_pull(state: State<'_, AppState>) -> AppResult<()> {
    sync_trigger_pull_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_list_conflicts(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<serde_json::Value>> {
    sync_list_conflicts_impl(&state, limit, offset).await
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
    sync_resolve_conflict_impl(&state, args).await
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub app_version: String,
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn device_info(state: State<'_, AppState>) -> AppResult<DeviceInfo> {
    device_info_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_set_sync_server_url(state: State<'_, AppState>, url: String) -> AppResult<()> {
    config_set_sync_server_url_impl(&state, url).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_get_sync_server_url(state: State<'_, AppState>) -> AppResult<Option<String>> {
    config_get_sync_server_url_impl(&state).await
}
