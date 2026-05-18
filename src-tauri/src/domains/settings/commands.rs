//! Tauri commands for the settings bounded context.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::settings::domain::entities::Setting;
use crate::domains::settings::domain::value_objects::SettingValue;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct SettingResponse {
    pub id: String,
    pub key: String,
    pub value: SettingValue,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
    pub entity_id: String,
}

impl From<Setting> for SettingResponse {
    fn from(s: Setting) -> Self {
        Self {
            id: s.id.to_string(),
            key: s.key,
            value: s.value,
            updated_at: s.updated_at,
            version: s.version,
            entity_id: s.entity_id,
        }
    }
}

async fn resolve_entity_id(state: &AppState) -> String {
    state
        .get_current_user()
        .await
        .map(|c| c.entity_id)
        .unwrap_or_else(|| "unscoped".to_string())
}

pub async fn settings_list_impl(state: &AppState) -> AppResult<Vec<SettingResponse>> {
    let svc = state
        .settings_service()
        .ok_or_else(|| AppError::Configuration("settings service unavailable".into()))?;
    let entity_id = resolve_entity_id(state).await;
    let rows = svc.list(&entity_id).await?;
    Ok(rows.into_iter().map(SettingResponse::from).collect())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn settings_list(state: State<'_, AppState>) -> AppResult<Vec<SettingResponse>> {
    settings_list_impl(&state).await
}

#[derive(Debug, Deserialize)]
pub struct SettingKeyArgs {
    pub key: String,
}

pub async fn settings_get_impl(
    state: &AppState,
    args: SettingKeyArgs,
) -> AppResult<Option<SettingResponse>> {
    let svc = state
        .settings_service()
        .ok_or_else(|| AppError::Configuration("settings service unavailable".into()))?;
    let entity_id = resolve_entity_id(state).await;
    let row = svc.get(&args.key, &entity_id).await?;
    Ok(row.map(SettingResponse::from))
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn settings_get(
    state: State<'_, AppState>,
    args: SettingKeyArgs,
) -> AppResult<Option<SettingResponse>> {
    settings_get_impl(&state, args).await
}

#[derive(Debug, Deserialize)]
pub struct SettingUpdateArgs {
    pub key: String,
    pub value: SettingValue,
}

pub async fn settings_update_impl(
    state: &AppState,
    args: SettingUpdateArgs,
) -> AppResult<SettingResponse> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let actor_id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation("invalid actor role".into()))?;

    let svc = state
        .settings_service()
        .ok_or_else(|| AppError::Configuration("settings service unavailable".into()))?;
    let updated = svc
        .update(actor_id, role, &ctx.entity_id, &args.key, args.value)
        .await?;

    // Cache the new value for in-memory consumers.
    state
        .set_setting(updated.key.clone(), serde_json::to_value(&updated.value)?)
        .await;

    Ok(updated.into())
}

#[tauri::command]
#[instrument(skip(state, app, args))]
pub async fn settings_update(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SettingUpdateArgs,
) -> AppResult<SettingResponse> {
    let updated = settings_update_impl(&state, args).await?;
    // Notify frontend so active drafts can prompt recompute (phase-05).
    let _ = app.emit(
        "settings:changed",
        serde_json::json!({
            "key": updated.key,
            "version": updated.version,
        }),
    );
    Ok(updated)
}

#[derive(Debug, Deserialize)]
pub struct SetLocaleArgs {
    pub locale: String,
}

const ALLOWED_LOCALES: &[&str] = &["en", "ar"];

pub async fn settings_set_locale_impl(
    state: &AppState,
    args: SetLocaleArgs,
) -> AppResult<SettingResponse> {
    if !ALLOWED_LOCALES.contains(&args.locale.as_str()) {
        return Err(AppError::Validation(format!(
            "locale must be one of: en, ar (got {})",
            args.locale
        )));
    }
    settings_update_impl(
        state,
        SettingUpdateArgs {
            key: "locale".into(),
            value: SettingValue::Text(args.locale),
        },
    )
    .await
}

#[tauri::command]
#[instrument(skip(state, app, args))]
pub async fn settings_set_locale(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SetLocaleArgs,
) -> AppResult<SettingResponse> {
    let updated = settings_set_locale_impl(&state, args).await?;
    let _ = app.emit(
        "settings:changed",
        serde_json::json!({
            "key": updated.key,
            "version": updated.version,
        }),
    );
    let _ = app.emit(
        "locale:changed",
        serde_json::json!({
            "locale": updated.value.as_storage(),
            "version": updated.version,
        }),
    );
    Ok(updated)
}
