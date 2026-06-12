//! Tauri commands for the visits bounded context.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::Operator;
use crate::domains::receipts::{ReceiptArtifacts, ReceiptRenderOptions};
use crate::domains::visits::domain::entities::{Visit, VisitStatus};
use crate::domains::visits::domain::repositories::WorkspaceFilters;
use crate::domains::visits::domain::services::MoneySettings;
use crate::domains::visits::service::{
    ChecksGridCard, CreateDraftInput, LockResult, UpdateDraftInput, VisitService,
};
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

fn service(state: &AppState) -> AppResult<Arc<VisitService>> {
    state
        .visit_service()
        .ok_or_else(|| AppError::Configuration("visits service unavailable".into()))
}

async fn money_settings(state: &AppState) -> AppResult<MoneySettings> {
    // These three keys are seeded by migration 002 and warmed into the cache at
    // bootstrap. A missing key here means a broken DB or an unwarmed cache --
    // fail loudly instead of locking a visit with a silently-zeroed money
    // snapshot (which would permanently corrupt the immutable receipt).
    async fn required_i64(state: &AppState, key: &str) -> AppResult<i64> {
        state
            .get_setting(key)
            .await
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                AppError::Configuration(format!("required money setting `{key}` is not configured"))
            })
    }
    Ok(MoneySettings {
        dye_cost_iqd: required_i64(state, "dye_cost_iqd").await?,
        report_cost_iqd: required_i64(state, "report_cost_iqd").await?,
        internal_doctor_pct: required_i64(state, "internal_doctor_pct").await?,
    })
}

async fn receipt_options(state: &AppState) -> ReceiptRenderOptions {
    let clinic_en = state
        .get_setting("clinic_display_name_en")
        .await
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let clinic_ar = state
        .get_setting("clinic_display_name_ar")
        .await
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let clinic_name = clinic_en.or(clinic_ar).unwrap_or_else(|| "IDC".into());
    let thermal_width = state
        .get_setting("thermal_width")
        .await
        .and_then(|v| v.as_i64())
        .unwrap_or(32);
    let arabic_numerals = state
        .get_setting("arabic_numerals")
        .await
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let currency_symbol = state
        .get_setting("currency_symbol")
        .await
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "IQD".into());
    ReceiptRenderOptions {
        clinic_name,
        thermal_width: thermal_width.max(20) as u32,
        arabic_numerals,
        currency_symbol,
    }
}

#[derive(Debug, Serialize)]
pub struct VisitDto {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub status: &'static str,
    pub receptionist_user_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub operator_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    pub locked_at: Option<String>,
    pub voided_at: Option<String>,
    pub voided_by_user_id: Option<Uuid>,
    pub void_reason: Option<String>,
    pub snapshots: Option<crate::domains::visits::domain::entities::VisitSnapshots>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub version: i64,
    pub dirty: bool,
    pub entity_id: String,
}

impl From<&Visit> for VisitDto {
    fn from(v: &Visit) -> Self {
        Self {
            id: v.id,
            patient_id: v.patient_id,
            status: v.status.as_str(),
            receptionist_user_id: v.receptionist_user_id,
            check_type_id: v.check_type_id,
            check_subtype_id: v.check_subtype_id,
            doctor_id: v.doctor_id,
            operator_id: v.operator_id,
            dye: v.dye,
            report: v.report,
            locked_at: v.locked_at.map(|d| d.to_rfc3339()),
            voided_at: v.voided_at.map(|d| d.to_rfc3339()),
            voided_by_user_id: v.voided_by_user_id,
            void_reason: v.void_reason.clone(),
            snapshots: v.snapshots.clone(),
            created_at: v.created_at.to_rfc3339(),
            updated_at: v.updated_at.to_rfc3339(),
            deleted_at: v.deleted_at.map(|d| d.to_rfc3339()),
            version: v.version,
            dirty: v.dirty,
            entity_id: v.entity_id.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OperatorDto {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
}

impl From<&Operator> for OperatorDto {
    fn from(o: &Operator) -> Self {
        Self {
            id: o.id,
            name: o.name.clone(),
            is_active: o.is_active,
        }
    }
}

// ---- create draft ---------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VisitCreateDraftArgs {
    pub patient_id: String,
    pub check_type_id: String,
    #[serde(default)]
    pub check_subtype_id: Option<String>,
    #[serde(default)]
    pub doctor_id: Option<String>,
    #[serde(default)]
    pub dye: bool,
    #[serde(default)]
    pub report: bool,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_create_draft(
    args: VisitCreateDraftArgs,
    state: State<'_, AppState>,
) -> AppResult<VisitDto> {
    let (user_id, role, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let v = svc
        .create_draft(
            user_id,
            role,
            &entity_id,
            CreateDraftInput {
                patient_id: Uuid::parse_str(&args.patient_id)?,
                check_type_id: Uuid::parse_str(&args.check_type_id)?,
                check_subtype_id: match args.check_subtype_id {
                    Some(s) => Some(Uuid::parse_str(&s)?),
                    None => None,
                },
                doctor_id: match args.doctor_id {
                    Some(d) => Some(Uuid::parse_str(&d)?),
                    None => None,
                },
                dye: args.dye,
                report: args.report,
            },
        )
        .await?;
    Ok(VisitDto::from(&v))
}

#[derive(Debug, Deserialize)]
pub struct VisitUpdateDraftArgs {
    pub visit_id: String,
    #[serde(default)]
    pub patient_id: Option<String>,
    #[serde(default)]
    pub check_subtype_id: Option<Option<String>>,
    #[serde(default)]
    pub doctor_id: Option<Option<String>>,
    #[serde(default)]
    pub dye: Option<bool>,
    #[serde(default)]
    pub report: Option<bool>,
}

fn parse_uuid_set_opt(v: Option<Option<String>>) -> AppResult<Option<Option<Uuid>>> {
    match v {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(s)) => Ok(Some(Some(Uuid::parse_str(&s)?))),
    }
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_update_draft(
    args: VisitUpdateDraftArgs,
    state: State<'_, AppState>,
) -> AppResult<VisitDto> {
    let (user_id, role, _) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let v = svc
        .update_draft(
            user_id,
            role,
            UpdateDraftInput {
                visit_id: Uuid::parse_str(&args.visit_id)?,
                patient_id: args.patient_id.map(|s| Uuid::parse_str(&s)).transpose()?,
                check_subtype_id: parse_uuid_set_opt(args.check_subtype_id)?,
                doctor_id: parse_uuid_set_opt(args.doctor_id)?,
                dye: args.dye,
                report: args.report,
            },
        )
        .await?;
    Ok(VisitDto::from(&v))
}

#[derive(Debug, Deserialize)]
pub struct VisitIdArgs {
    pub visit_id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_get(args: VisitIdArgs, state: State<'_, AppState>) -> AppResult<VisitDto> {
    let svc = service(state.inner())?;
    let v = svc.get(Uuid::parse_str(&args.visit_id)?).await?;
    Ok(VisitDto::from(&v))
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_discard(args: VisitIdArgs, state: State<'_, AppState>) -> AppResult<()> {
    let (user_id, role, _) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    svc.discard(user_id, role, Uuid::parse_str(&args.visit_id)?)
        .await
}

#[derive(Debug, Deserialize)]
pub struct ChecksGridArgs {}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_checks_grid(state: State<'_, AppState>) -> AppResult<Vec<ChecksGridCard>> {
    let (_, _, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    svc.checks_grid(&entity_id).await
}

#[derive(Debug, Deserialize)]
pub struct CheckTypeIdArgs {
    pub check_type_id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_list_today_by_check(
    args: CheckTypeIdArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<VisitDto>> {
    let (_, _, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let rows = svc
        .list_today_by_check(&entity_id, Uuid::parse_str(&args.check_type_id)?)
        .await?;
    Ok(rows.iter().map(VisitDto::from).collect())
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_list_drafts_by_check(
    args: CheckTypeIdArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<VisitDto>> {
    let (_, _, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let rows = svc
        .list_drafts_by_check(&entity_id, Uuid::parse_str(&args.check_type_id)?)
        .await?;
    Ok(rows.iter().map(VisitDto::from).collect())
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceListArgs {
    pub check_type_id: String,
    #[serde(default)]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub doctor_ids: Vec<String>,
    #[serde(default)]
    pub subtype_ids: Vec<String>,
    #[serde(default = "default_workspace_limit")]
    pub limit: i64,
}

fn default_workspace_limit() -> i64 {
    50
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_list_workspace(
    args: WorkspaceListArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<VisitDto>> {
    let (_, _, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let statuses = args
        .statuses
        .iter()
        .filter_map(|s| VisitStatus::parse(s))
        .collect::<Vec<_>>();
    let doctor_ids = args
        .doctor_ids
        .iter()
        .map(|s| Uuid::parse_str(s))
        .collect::<Result<Vec<_>, _>>()?;
    let subtype_ids = args
        .subtype_ids
        .iter()
        .map(|s| Uuid::parse_str(s))
        .collect::<Result<Vec<_>, _>>()?;
    let filters = WorkspaceFilters {
        statuses,
        doctor_ids,
        subtype_ids,
        from: None,
        to: None,
    };
    let rows = svc
        .list_workspace(
            &entity_id,
            Uuid::parse_str(&args.check_type_id)?,
            filters,
            args.limit,
        )
        .await?;
    Ok(rows.iter().map(VisitDto::from).collect())
}

// ---- qualified operators --------------------------------------------------

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_qualified_operators(
    args: CheckTypeIdArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<OperatorDto>> {
    let (_, _, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let ops = svc
        .qualified_operators(&entity_id, Uuid::parse_str(&args.check_type_id)?)
        .await?;
    Ok(ops.iter().map(OperatorDto::from).collect())
}

// ---- lock & void ----------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VisitLockArgs {
    pub visit_id: String,
    pub operator_id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_lock(args: VisitLockArgs, state: State<'_, AppState>) -> AppResult<LockResult> {
    let (user_id, role, _) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let settings = money_settings(state.inner()).await?;
    let receipt = receipt_options(state.inner()).await;
    svc.lock(
        user_id,
        role,
        Uuid::parse_str(&args.visit_id)?,
        Uuid::parse_str(&args.operator_id)?,
        settings,
        receipt,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct VisitVoidArgs {
    pub visit_id: String,
    pub reason: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_void(args: VisitVoidArgs, state: State<'_, AppState>) -> AppResult<VisitDto> {
    let (user_id, role, _) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let v = svc
        .void(user_id, role, Uuid::parse_str(&args.visit_id)?, args.reason)
        .await?;
    Ok(VisitDto::from(&v))
}

// ---- pricing resolve ------------------------------------------------------

#[instrument(skip(state))]
#[tauri::command]
pub async fn visits_pricing_resolve(
    args: VisitIdArgs,
    state: State<'_, AppState>,
) -> AppResult<crate::domains::visits::service::ResolvedSnapshots> {
    let svc = service(state.inner())?;
    let settings = money_settings(state.inner()).await?;
    svc.resolve_snapshots(Uuid::parse_str(&args.visit_id)?, settings)
        .await
}

// ---- receipts -------------------------------------------------------------

#[instrument(skip(state))]
#[tauri::command]
pub async fn receipts_reprint(
    args: VisitIdArgs,
    state: State<'_, AppState>,
) -> AppResult<ReceiptArtifacts> {
    let svc = service(state.inner())?;
    let receipt = receipt_options(state.inner()).await;
    svc.render_receipt(Uuid::parse_str(&args.visit_id)?, receipt)
        .await
}

// ---- lines run today (phase-04 §7.25) -------------------------------------

#[derive(Debug, Deserialize)]
pub struct OperatorIdArgs {
    pub operator_id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn shifts_lines_run_today(
    args: OperatorIdArgs,
    state: State<'_, AppState>,
) -> AppResult<i64> {
    let (_, _, entity_id) = current_actor(state.inner()).await?;
    let svc = service(state.inner())?;
    svc.lines_run_today(&entity_id, Uuid::parse_str(&args.operator_id)?)
        .await
}
