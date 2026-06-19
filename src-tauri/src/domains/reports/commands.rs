//! Tauri commands for the reports bounded context (phase-07 §3 Tauri table).
//!
//! Role gating: every reports IPC opens with `ReportsService::require_reports_role`
//! (phase-07 §7.17). The void path on `/accounting/visits/:id` is delegated
//! to `visits_void` in the visits domain, which already requires superadmin.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::reports::domain::entities::{
    DailyClose, DashboardKpis, DashboardTops, DateRange, DoctorDrilldown, DoctorEarningsRow,
    FrozenClose, OperatorDrilldown, OperatorEarningsRow, VisitsReport, VisitsReportFilters,
    VisitsReportGroupBy,
};
use crate::domains::reports::service::ReportsService;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

async fn actor(state: &AppState) -> AppResult<(Uuid, UserRole, String)> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation(format!("unknown role: {}", ctx.role)))?;
    Ok((id, role, ctx.entity_id))
}

fn service(state: &AppState) -> AppResult<Arc<ReportsService>> {
    state
        .reports_service()
        .ok_or_else(|| AppError::Configuration("reports service unavailable".into()))
}

#[derive(Debug, Deserialize)]
pub struct RangeArgs {
    pub from_utc: chrono::DateTime<chrono::Utc>,
    pub to_utc: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub include_voided: bool,
}

impl RangeArgs {
    fn range(&self) -> DateRange {
        DateRange {
            from_utc: self.from_utc,
            to_utc: self.to_utc,
        }
    }
}

// ---- dashboard ------------------------------------------------------------

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_dashboard_kpis(
    args: RangeArgs,
    state: State<'_, AppState>,
) -> AppResult<DashboardKpis> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    svc.dashboard_kpis(&entity_id, args.range(), args.include_voided)
        .await
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_dashboard_tops(
    args: RangeArgs,
    state: State<'_, AppState>,
) -> AppResult<DashboardTops> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    svc.dashboard_tops(&entity_id, args.range(), args.include_voided)
        .await
}

// ---- visits report --------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VisitsReportArgs {
    pub from_utc: chrono::DateTime<chrono::Utc>,
    pub to_utc: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub include_voided: bool,
    #[serde(default)]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub check_type_ids: Vec<String>,
    #[serde(default)]
    pub subtype_ids: Vec<String>,
    #[serde(default)]
    pub doctor_ids: Vec<String>,
    #[serde(default)]
    pub operator_ids: Vec<String>,
    #[serde(default)]
    pub include_house: bool,
    #[serde(default)]
    pub dye: Option<bool>,
    #[serde(default)]
    pub report: Option<bool>,
    #[serde(default)]
    pub group_by: Option<VisitsReportGroupBy>,
    #[serde(default)]
    pub limit: Option<i64>,
}

fn parse_uuid_list(strs: Vec<String>) -> AppResult<Vec<Uuid>> {
    strs.into_iter()
        .map(|s| Uuid::parse_str(&s).map_err(|e| AppError::Validation(format!("uuid: {e}"))))
        .collect()
}

fn make_filters(args: VisitsReportArgs, entity_id: &str) -> AppResult<VisitsReportFilters> {
    Ok(VisitsReportFilters {
        from: args.from_utc,
        to: args.to_utc,
        include_voided: args.include_voided,
        statuses: args.statuses,
        check_type_ids: parse_uuid_list(args.check_type_ids)?,
        subtype_ids: parse_uuid_list(args.subtype_ids)?,
        doctor_ids: parse_uuid_list(args.doctor_ids)?,
        operator_ids: parse_uuid_list(args.operator_ids)?,
        include_house: args.include_house,
        dye: args.dye,
        report: args.report,
        group_by: args.group_by.unwrap_or_default(),
        limit: args.limit,
        entity_id: entity_id.to_string(),
    })
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_visits(
    args: VisitsReportArgs,
    state: State<'_, AppState>,
) -> AppResult<VisitsReport> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let filters = make_filters(args, &entity_id)?;
    svc.visits_report(filters).await
}

// ---- doctor earnings ------------------------------------------------------

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_doctor_earnings(
    args: RangeArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<DoctorEarningsRow>> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    svc.doctor_earnings(&entity_id, args.range(), args.include_voided)
        .await
}

#[derive(Debug, Deserialize)]
pub struct DoctorDrilldownArgs {
    /// `None` => the house pseudo-row.
    #[serde(default)]
    pub doctor_id: Option<String>,
    pub from_utc: chrono::DateTime<chrono::Utc>,
    pub to_utc: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub include_voided: bool,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_doctor_drilldown(
    args: DoctorDrilldownArgs,
    state: State<'_, AppState>,
) -> AppResult<DoctorDrilldown> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let did = match args.doctor_id {
        Some(s) => Some(Uuid::parse_str(&s)?),
        None => None,
    };
    let range = DateRange {
        from_utc: args.from_utc,
        to_utc: args.to_utc,
    };
    svc.doctor_drilldown(&entity_id, did, range, args.include_voided)
        .await
}

// ---- operator earnings ----------------------------------------------------

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_operator_earnings(
    args: RangeArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<OperatorEarningsRow>> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    svc.operator_earnings(&entity_id, args.range(), args.include_voided)
        .await
}

#[derive(Debug, Deserialize)]
pub struct OperatorDrilldownArgs {
    pub operator_id: String,
    pub from_utc: chrono::DateTime<chrono::Utc>,
    pub to_utc: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub include_voided: bool,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_operator_drilldown(
    args: OperatorDrilldownArgs,
    state: State<'_, AppState>,
) -> AppResult<OperatorDrilldown> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let op = Uuid::parse_str(&args.operator_id)?;
    let range = DateRange {
        from_utc: args.from_utc,
        to_utc: args.to_utc,
    };
    svc.operator_drilldown(&entity_id, op, range, args.include_voided)
        .await
}

// ---- daily close ----------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DailyCloseArgs {
    /// Local-day calendar date (YYYY-MM-DD).
    pub date: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_daily_close(
    args: DailyCloseArgs,
    state: State<'_, AppState>,
) -> AppResult<DailyClose> {
    let (user_id, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let date = NaiveDate::parse_from_str(&args.date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("date: {e}")))?;
    let settings = state.settings_snapshot().await;
    let mut snapshot: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in (*settings).iter() {
        snapshot.insert(k.clone(), v.to_string());
    }
    svc.daily_close(user_id, &entity_id, date, snapshot).await
}

// ---- sign / freeze --------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SignDailyCloseArgs {
    pub date: String,
}

/// Sign & freeze a reconciled day. Accountant or superadmin. Refuses if the day
/// still has pending-sync ops or is already frozen.
#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_sign_daily_close(
    args: SignDailyCloseArgs,
    state: State<'_, AppState>,
) -> AppResult<FrozenClose> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let user_id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation(format!("unknown role: {}", ctx.role)))?;
    ReportsService::require_reports_role(role)?;
    let signer_name = ctx.name.clone().unwrap_or_else(|| ctx.email.clone());
    let svc = service(state.inner())?;
    let date = NaiveDate::parse_from_str(&args.date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("date: {e}")))?;
    let settings = state.settings_snapshot().await;
    let mut snapshot: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in (*settings).iter() {
        snapshot.insert(k.clone(), v.to_string());
    }
    svc.sign_daily_close(user_id, signer_name, &ctx.entity_id, date, snapshot)
        .await
}

#[derive(Debug, Deserialize)]
pub struct ReopenDailyCloseArgs {
    pub date: String,
    pub reason: String,
}

/// Reopen (unfreeze) a frozen day. Superadmin only.
#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_reopen_daily_close(
    args: ReopenDailyCloseArgs,
    state: State<'_, AppState>,
) -> AppResult<FrozenClose> {
    let (user_id, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_role(role, &[UserRole::Superadmin])?;
    let svc = service(state.inner())?;
    let date = NaiveDate::parse_from_str(&args.date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("date: {e}")))?;
    svc.reopen_daily_close(user_id, &entity_id, date, args.reason)
        .await
}

#[derive(Debug, Deserialize)]
pub struct FrozenCloseForDateArgs {
    pub date: String,
}

/// The in-force frozen close for a day, if any. Accountant or superadmin.
#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_frozen_close_for_date(
    args: FrozenCloseForDateArgs,
    state: State<'_, AppState>,
) -> AppResult<Option<FrozenClose>> {
    let (_user_id, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let date = NaiveDate::parse_from_str(&args.date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("date: {e}")))?;
    svc.frozen_close_for_date(&entity_id, date).await
}

#[derive(Debug, Deserialize)]
pub struct ListFrozenClosesArgs {
    pub from_date: String,
    pub to_date: String,
}

/// All closes (in-force + reopened) in a date range, newest first. Backs the
/// month overview. Accountant or superadmin.
#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_list_daily_closes(
    args: ListFrozenClosesArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<FrozenClose>> {
    let (_user_id, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let from = NaiveDate::parse_from_str(&args.from_date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("from_date: {e}")))?;
    let to = NaiveDate::parse_from_str(&args.to_date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("to_date: {e}")))?;
    svc.list_frozen_closes(&entity_id, from, to).await
}

// ---- exports --------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ExportResultDto {
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct ExportVisitsArgs {
    pub filters: VisitsReportArgs,
    pub path: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_export_visits_csv(
    args: ExportVisitsArgs,
    state: State<'_, AppState>,
) -> AppResult<ExportResultDto> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let filters = make_filters(args.filters, &entity_id)?;
    let path = PathBuf::from(args.path);
    svc.export_visits_csv(filters, &path).await?;
    Ok(ExportResultDto { path })
}

#[derive(Debug, Deserialize)]
pub struct ExportEarningsArgs {
    pub from_utc: chrono::DateTime<chrono::Utc>,
    pub to_utc: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub include_voided: bool,
    pub path: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_export_doctors_csv(
    args: ExportEarningsArgs,
    state: State<'_, AppState>,
) -> AppResult<ExportResultDto> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let path = PathBuf::from(args.path);
    let range = DateRange {
        from_utc: args.from_utc,
        to_utc: args.to_utc,
    };
    svc.export_doctor_earnings_csv(&entity_id, range, args.include_voided, &path)
        .await?;
    Ok(ExportResultDto { path })
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_export_operators_csv(
    args: ExportEarningsArgs,
    state: State<'_, AppState>,
) -> AppResult<ExportResultDto> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let path = PathBuf::from(args.path);
    let range = DateRange {
        from_utc: args.from_utc,
        to_utc: args.to_utc,
    };
    svc.export_operator_earnings_csv(&entity_id, range, args.include_voided, &path)
        .await?;
    Ok(ExportResultDto { path })
}

#[derive(Debug, Deserialize)]
pub struct ExportDailyCloseArgs {
    pub date: String,
    pub path: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn reports_export_daily_close_pdf(
    args: ExportDailyCloseArgs,
    state: State<'_, AppState>,
) -> AppResult<ExportResultDto> {
    let (user_id, role, entity_id) = actor(state.inner()).await?;
    ReportsService::require_reports_role(role)?;
    let svc = service(state.inner())?;
    let date = NaiveDate::parse_from_str(&args.date, "%Y-%m-%d")
        .map_err(|e| AppError::Validation(format!("date: {e}")))?;
    let settings = state.settings_snapshot().await;
    let mut snapshot: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in (*settings).iter() {
        snapshot.insert(k.clone(), v.to_string());
    }
    let close = svc.daily_close(user_id, &entity_id, date, snapshot).await?;
    let path = PathBuf::from(args.path);
    svc.render_daily_close_pdf(&close, &path)?;
    Ok(ExportResultDto { path })
}
