//! Tauri commands for the patients bounded context.

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::patients::domain::entities::{Patient, PatientDemographicsInput};
use crate::domains::patients::domain::repositories::{
    DuplicateGroup, PatientListFilter, PatientSort, PatientStats, VisitSummary,
};
use crate::domains::patients::service::{PatientCreateInput, PatientService, PatientUpdateInput};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct PatientDto {
    pub id: Uuid,
    pub name: String,
    pub phone: Option<String>,
    pub sex: Option<String>,
    pub birth_date: Option<String>,
    pub file_no: Option<String>,
    pub notes: Option<String>,
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
            phone: p.phone.clone(),
            sex: p.sex.clone(),
            birth_date: p.birth_date.clone(),
            file_no: p.file_no.clone(),
            notes: p.notes.clone(),
            created_at: p.created_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
            deleted_at: p.deleted_at.map(|d| d.to_rfc3339()),
            version: p.version,
            dirty: p.dirty,
            entity_id: p.entity_id.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VisitSummaryDto {
    pub id: Uuid,
    pub status: String,
    pub locked_at: Option<String>,
    pub created_at: String,
    pub total_amount_iqd: Option<i64>,
    pub check_type_name_ar: Option<String>,
    pub check_type_name_en: Option<String>,
    pub doctor_name: Option<String>,
    pub void_reason: Option<String>,
}

impl From<&VisitSummary> for VisitSummaryDto {
    fn from(v: &VisitSummary) -> Self {
        Self {
            id: v.id,
            status: v.status.clone(),
            locked_at: v.locked_at.map(|d| d.to_rfc3339()),
            created_at: v.created_at.to_rfc3339(),
            total_amount_iqd: v.total_amount_iqd,
            check_type_name_ar: v.check_type_name_ar.clone(),
            check_type_name_en: v.check_type_name_en.clone(),
            doctor_name: v.doctor_name.clone(),
            void_reason: v.void_reason.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PatientStatsDto {
    pub total_visits: i64,
    pub total_spent_iqd: i64,
    pub last_visit_at: Option<String>,
    pub draft_count: i64,
    pub voided_count: i64,
}

impl From<&PatientStats> for PatientStatsDto {
    fn from(s: &PatientStats) -> Self {
        Self {
            total_visits: s.total_visits,
            total_spent_iqd: s.total_spent_iqd,
            last_visit_at: s.last_visit_at.map(|d| d.to_rfc3339()),
            draft_count: s.draft_count,
            voided_count: s.voided_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DuplicateGroupDto {
    pub kind: String,
    pub key: String,
    pub patient_ids: Vec<String>,
}

impl From<&DuplicateGroup> for DuplicateGroupDto {
    fn from(g: &DuplicateGroup) -> Self {
        Self {
            kind: g.kind.clone(),
            key: g.key.clone(),
            patient_ids: g.patient_ids.iter().map(|id| id.to_string()).collect(),
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

// ---- archive: list / detail reads ----------------------------------------

#[derive(Debug, Deserialize)]
pub struct PatientListArgs {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub include_deleted: bool,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default = "default_page_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_page_limit() -> i64 {
    50
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_list(
    args: PatientListArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<PatientDto>> {
    let (_, entity_id) = actor_user_id(state.inner()).await?;
    let svc = service(state.inner())?;
    let filter = PatientListFilter {
        entity_id,
        query: args.query,
        include_deleted: args.include_deleted,
        sort: PatientSort::parse(args.sort.as_deref()),
        limit: args.limit.clamp(1, 500),
        offset: args.offset.max(0),
    };
    let rows = svc.list(&filter).await?;
    Ok(rows.iter().map(PatientDto::from).collect())
}

#[derive(Debug, Deserialize)]
pub struct PatientVisitsArgs {
    pub id: String,
    #[serde(default = "default_page_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_list_visits(
    args: PatientVisitsArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<VisitSummaryDto>> {
    let _ = actor_user_id(state.inner()).await?;
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    let rows = svc
        .list_visits(id, args.limit.clamp(1, 500), args.offset.max(0))
        .await?;
    Ok(rows.iter().map(VisitSummaryDto::from).collect())
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_stats(
    args: PatientIdArgs,
    state: State<'_, AppState>,
) -> AppResult<PatientStatsDto> {
    let _ = actor_user_id(state.inner()).await?;
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    let stats = svc.stats(id).await?;
    Ok(PatientStatsDto::from(&stats))
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_find_duplicates(
    state: State<'_, AppState>,
) -> AppResult<Vec<DuplicateGroupDto>> {
    let (_, entity_id) = actor_user_id(state.inner()).await?;
    let svc = service(state.inner())?;
    let groups = svc.find_duplicates(&entity_id).await?;
    Ok(groups.iter().map(DuplicateGroupDto::from).collect())
}

// ---- archive: writes ------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PatientDemographicsArgs {
    pub id: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub sex: Option<String>,
    #[serde(default)]
    pub birth_date: Option<String>,
    #[serde(default)]
    pub file_no: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_update_demographics(
    args: PatientDemographicsArgs,
    state: State<'_, AppState>,
) -> AppResult<PatientDto> {
    let (user_id, _) = actor_user_id(state.inner()).await?;
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    let p = svc
        .update_demographics(
            user_id,
            id,
            PatientDemographicsInput {
                phone: args.phone,
                sex: args.sex,
                birth_date: args.birth_date,
                file_no: args.file_no,
                notes: args.notes,
            },
        )
        .await?;
    Ok(PatientDto::from(&p))
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_soft_delete(
    args: PatientIdArgs,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let (user_id, _) = actor_user_id(state.inner()).await?;
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    svc.soft_delete(user_id, id).await
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_restore(
    args: PatientIdArgs,
    state: State<'_, AppState>,
) -> AppResult<PatientDto> {
    let (user_id, _) = actor_user_id(state.inner()).await?;
    let id = Uuid::parse_str(&args.id)?;
    let svc = service(state.inner())?;
    let p = svc.restore(user_id, id).await?;
    Ok(PatientDto::from(&p))
}

#[derive(Debug, Deserialize)]
pub struct PatientMergeArgs {
    pub survivor_id: String,
    pub merged_id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn patients_merge(args: PatientMergeArgs, state: State<'_, AppState>) -> AppResult<()> {
    let (user_id, _) = actor_user_id(state.inner()).await?;
    let survivor = Uuid::parse_str(&args.survivor_id)?;
    let merged = Uuid::parse_str(&args.merged_id)?;
    let svc = service(state.inner())?;
    svc.merge(user_id, survivor, merged).await
}
