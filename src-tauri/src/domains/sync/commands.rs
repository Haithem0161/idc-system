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
    /// Ops that can no longer make progress on their own (parked after a
    /// server rejection or attempts-capped). Surfaced so stranded work is
    /// visible instead of silently lost; recover via `sync_requeue_op`.
    pub stuck_ops: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StuckOp {
    pub op_id: String,
    pub entity: String,
    pub entity_id: String,
    pub attempts: i32,
    pub parked: bool,
    pub last_error: Option<String>,
    pub created_at: String,
}

// --- testable `_impl` helpers -------------------------------------------
//
// Each `#[tauri::command]` delegates to a plain async fn taking `&AppState`,
// so phase-01 §2.2 tests can drive every command without standing up a
// Tauri runtime / `State` wrapper.

pub async fn sync_status_impl(state: &AppState) -> AppResult<SyncStatusSnapshot> {
    let engine = state.sync_engine();
    let status = engine.status().await;
    let outbox = engine.outbox_repo();
    let pending_ops = outbox.pending_count().await?;
    let stuck_ops = outbox.stuck_count().await?;
    Ok(SyncStatusSnapshot {
        status,
        pending_ops,
        stuck_ops,
    })
}

pub async fn sync_outbox_count_impl(state: &AppState) -> AppResult<u32> {
    state.sync_engine().outbox_repo().pending_count().await
}

pub async fn sync_list_stuck_impl(state: &AppState) -> AppResult<Vec<StuckOp>> {
    let ops = state.sync_engine().outbox_repo().list_stuck().await?;
    Ok(ops
        .into_iter()
        .map(|op| StuckOp {
            op_id: op.op_id.to_string(),
            entity: op.entity,
            entity_id: op.entity_id,
            attempts: op.attempts,
            parked: op.parked,
            last_error: op.last_error,
            created_at: op.created_at.to_rfc3339(),
        })
        .collect())
}

pub async fn sync_requeue_op_impl(state: &AppState, op_id: String) -> AppResult<()> {
    let parsed = uuid::Uuid::parse_str(&op_id)?;
    let affected = state
        .sync_engine()
        .outbox_repo()
        .requeue_stuck(parsed)
        .await?;
    if affected == 0 {
        return Err(crate::error::AppError::NotFound(format!(
            "no stuck op with id {op_id}"
        )));
    }
    // Kick a push so the requeued op is attempted promptly.
    state.sync_engine().trigger_push().await;
    Ok(())
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
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(crate::error::AppError::Validation(
            "sync server url required".into(),
        ));
    }
    let url = trimmed.to_string();
    // Persist FIRST so a crash between writes can never leave the engine
    // pointing at a URL the DB has forgotten. The setter is also called by
    // the first-launch modal and the superadmin first-run wizard; without
    // persistence the modal reopened on every restart.
    state
        .sync_engine()
        .state_repo()
        .put_server_url(&url)
        .await?;
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
pub async fn sync_list_stuck(state: State<'_, AppState>) -> AppResult<Vec<StuckOp>> {
    sync_list_stuck_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_requeue_op(state: State<'_, AppState>, op_id: String) -> AppResult<()> {
    sync_requeue_op_impl(&state, op_id).await
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
