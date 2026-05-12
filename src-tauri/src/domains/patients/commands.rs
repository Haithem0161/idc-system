//! Tauri commands for the patients bounded context.

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::patients::domain::entities::Patient;
use crate::domains::patients::service::{PatientCreateInput, PatientService, PatientUpdateInput};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct PatientDto {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub version: i64,
    pub dirty: bool,
    pub entity_id: String,
}

impl From<&Patient> for PatientDto {
    fn from(p: &Patient) -> Self {
        Self {
            id: p.id,
            name: p.name.clone(),
            created_at: p.created_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
            deleted_at: p.deleted_at.map(|d| d.to_rfc3339()),
            version: p.version,
            dirty: p.dirty,
            entity_id: p.entity_id.clone(),
        }
    }
}

async fn actor_user_id(state: &AppState) -> AppResult<(Uuid, String)> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let id = Uuid::parse_str(&ctx.user_id)?;
    Ok((id, ctx.entity_id))
}

fn service(state: &AppState) -> AppResult<std::sync::Arc<PatientService>> {
    state
        .patient_service()
        .ok_or_else(|| AppError::Configuration("patients service unavailable".into()))
}

#[derive(Debug, Deserialize)]
pub struct PatientSearchArgs {
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_search(
    args: PatientSearchArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<PatientDto>> {
    let (_, entity_id) = actor_user_id(state.inner()).await?;
    let svc = service(state.inner())?;
    let rows = svc.search(&entity_id, &args.query, args.limit).await?;
    Ok(rows.iter().map(PatientDto::from).collect())
}

#[derive(Debug, Deserialize)]
pub struct PatientCreateArgs {
    pub name: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_create(
    args: PatientCreateArgs,
    state: State<'_, AppState>,
) -> AppResult<PatientDto> {
    let (user_id, entity_id) = actor_user_id(state.inner()).await?;
    let svc = service(state.inner())?;
    let p = svc
        .create(user_id, &entity_id, PatientCreateInput { name: args.name })
        .await?;
    Ok(PatientDto::from(&p))
}

#[derive(Debug, Deserialize)]
pub struct PatientIdArgs {
    pub id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_get(
    args: PatientIdArgs,
    state: State<'_, AppState>,
) -> AppResult<PatientDto> {
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    let p = svc.get(id).await?;
    Ok(PatientDto::from(&p))
}

#[derive(Debug, Deserialize)]
pub struct PatientUpdateArgs {
    pub id: String,
    pub name: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_update(
    args: PatientUpdateArgs,
    state: State<'_, AppState>,
) -> AppResult<PatientDto> {
    let (user_id, _) = actor_user_id(state.inner()).await?;
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    let p = svc
        .update(user_id, id, PatientUpdateInput { name: args.name })
        .await?;
    Ok(PatientDto::from(&p))
}
