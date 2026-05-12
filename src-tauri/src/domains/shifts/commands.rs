//! Tauri commands for the shifts bounded context.
//!
//! Reads are open to any authenticated user; mutations are role-gated
//! inside `ShiftService`. The IPC layer exists to:
//! - parse string UUIDs and ISO timestamps to typed values,
//! - resolve the current actor from `AppState`,
//! - thread the `entity_id` through to the service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::shifts::domain::entities::OperatorShift;
use crate::domains::shifts::service::{ShiftEditInput, ShiftWithMeta};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

async fn current_actor(state: &AppState) -> AppResult<(Uuid, UserRole, String)> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation(format!("invalid role: {}", ctx.role)))?;
    Ok((id, role, ctx.entity_id))
}

fn service(state: &AppState) -> AppResult<std::sync::Arc<crate::domains::shifts::ShiftService>> {
    state
        .shift_service()
        .ok_or_else(|| AppError::Configuration("shifts service unavailable".into()))
}

// ---- args --------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ShiftClockInArgs {
    pub operator_id: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShiftIdArgs {
    pub shift_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ShiftEditArgs {
    pub shift_id: String,
    pub check_in_at: DateTime<Utc>,
    #[serde(default)]
    pub check_out_at: Option<DateTime<Utc>>,
    /// Use `null` to leave the note unchanged. Use `{ "value": null }` to
    /// explicitly clear it. Use `{ "value": "..." }` to set it.
    #[serde(default)]
    pub note: Option<NoteUpdate>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum NoteUpdate {
    Replace { value: Option<String> },
}

impl NoteUpdate {
    fn into_value(self) -> Option<String> {
        match self {
            NoteUpdate::Replace { value } => value,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ShiftSoftDeleteArgs {
    pub shift_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ShiftListOverlapsArgs {
    #[serde(default)]
    pub operator_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OverlapPairResponse {
    pub left: OperatorShift,
    pub right: OperatorShift,
}

// ---- commands ----------------------------------------------------------

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn shifts_clock_in(
    state: State<'_, AppState>,
    args: ShiftClockInArgs,
) -> AppResult<OperatorShift> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = service(&state)?;
    let operator_id = Uuid::parse_str(&args.operator_id)?;
    svc.clock_in(uid, role, &entity_id, operator_id, args.note)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn shifts_clock_out(
    state: State<'_, AppState>,
    args: ShiftIdArgs,
) -> AppResult<OperatorShift> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = service(&state)?;
    let shift_id = Uuid::parse_str(&args.shift_id)?;
    svc.clock_out(uid, role, shift_id).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn shifts_list_open(state: State<'_, AppState>) -> AppResult<Vec<ShiftWithMeta>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = service(&state)?;
    svc.list_open(&entity_id).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn shifts_history_today(state: State<'_, AppState>) -> AppResult<Vec<ShiftWithMeta>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = service(&state)?;
    let now = Utc::now();
    let today_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::Internal("date floor".into()))?
        .and_utc();
    let today_end = today_start + chrono::Duration::days(1);
    svc.history_today(&entity_id, today_start, today_end).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn shifts_edit(
    state: State<'_, AppState>,
    args: ShiftEditArgs,
) -> AppResult<OperatorShift> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = service(&state)?;
    let shift_id = Uuid::parse_str(&args.shift_id)?;
    let note: Option<Option<String>> = args.note.map(NoteUpdate::into_value);
    svc.edit(
        uid,
        role,
        ShiftEditInput {
            shift_id,
            check_in_at: args.check_in_at,
            check_out_at: args.check_out_at,
            note,
        },
    )
    .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn shifts_soft_delete(
    state: State<'_, AppState>,
    args: ShiftSoftDeleteArgs,
) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = service(&state)?;
    let shift_id = Uuid::parse_str(&args.shift_id)?;
    svc.soft_delete(uid, role, shift_id, args.reason).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn shifts_list_overlaps(
    state: State<'_, AppState>,
    args: ShiftListOverlapsArgs,
) -> AppResult<Vec<OverlapPairResponse>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = service(&state)?;
    let operator_id = args
        .operator_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()?;
    let pairs = svc.list_overlaps(&entity_id, operator_id).await?;
    Ok(pairs
        .into_iter()
        .map(|p| OverlapPairResponse {
            left: p.left,
            right: p.right,
        })
        .collect())
}
