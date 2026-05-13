//! Tauri commands for the audit + diagnostics bounded context (phase-08
//! §3 Tauri table).
//!
//! Every command opens with `actor()` to extract `(user_id, role, entity_id)`
//! from the auth context, then `AuditQueryService::require_audit_role` for
//! the gated paths (phase-08 §7.23).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::audit::domain::{AuditPage, DiagnosticsSummaryDto};
use crate::domains::audit::service::{
    AuditQueryService, AuditVacuumJob, AuditVacuumOutcome, DiagnosticsService,
};
use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::sync::domain::repositories::AuditFilter;
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

fn audit_service(state: &AppState) -> AppResult<Arc<AuditQueryService>> {
    state
        .audit_query_service()
        .ok_or_else(|| AppError::Configuration("audit service unavailable".into()))
}

fn vacuum_job(state: &AppState) -> AppResult<Arc<AuditVacuumJob>> {
    state
        .audit_vacuum_job()
        .ok_or_else(|| AppError::Configuration("audit vacuum unavailable".into()))
}

fn diagnostics_service(state: &AppState) -> AppResult<Arc<DiagnosticsService>> {
    state
        .diagnostics_service()
        .ok_or_else(|| AppError::Configuration("diagnostics service unavailable".into()))
}

#[derive(Debug, Deserialize)]
pub struct AuditQueryArgs {
    #[serde(default)]
    pub actor_user_id: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub entity: Option<String>,
    #[serde(default)]
    pub entity_id_prefix: Option<String>,
    #[serde(default)]
    pub from_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub to_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn audit_query(state: State<'_, AppState>, args: AuditQueryArgs) -> AppResult<AuditPage> {
    let (_, role, entity_id) = actor(state.inner()).await?;
    AuditQueryService::require_audit_role(role)?;
    let svc = audit_service(state.inner())?;
    let filter = AuditFilter {
        entity_id_tenant: entity_id,
        actor_user_id: args.actor_user_id,
        action: args.action,
        entity: args.entity,
        entity_id_prefix: args.entity_id_prefix,
        from_utc: args.from_utc,
        to_utc: args.to_utc,
        free_text: args.text,
        limit: args.limit.unwrap_or(50),
        offset: args.offset.unwrap_or(0),
    };
    svc.query(filter).await
}

#[derive(Debug, Clone, Serialize)]
pub struct VacuumResultDto {
    pub audit_purged: u64,
    pub metrics_purged: u64,
}

impl From<AuditVacuumOutcome> for VacuumResultDto {
    fn from(o: AuditVacuumOutcome) -> Self {
        Self {
            audit_purged: o.audit_purged,
            metrics_purged: o.metrics_purged,
        }
    }
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn audit_vacuum_now(state: State<'_, AppState>) -> AppResult<VacuumResultDto> {
    let (user_id, role, entity_id) = actor(state.inner()).await?;
    AuditQueryService::require_audit_role(role)?;
    let job = vacuum_job(state.inner())?;
    let out = job.run(Some(user_id), &entity_id).await?;
    Ok(out.into())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn diagnostics_summary(state: State<'_, AppState>) -> AppResult<DiagnosticsSummaryDto> {
    let (_, _, entity_id) = actor(state.inner()).await?;
    // Diagnostics is read-only and visible to any authenticated role; the
    // payload exposes nothing sensitive (counters + latency + last-sync).
    let svc = diagnostics_service(state.inner())?;
    svc.summary(&entity_id).await
}
