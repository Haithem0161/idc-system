//! Tauri commands for auth + users.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::entities::User;
use crate::domains::auth::domain::value_objects::{LoginMode, UserRole};
use crate::domains::auth::user_service::{UserCreateInput, UserUpdateInput};
use crate::error::{AppError, AppResult};
use crate::state::{AppState, UserContext};

#[derive(Debug, Clone, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: UserRole,
    pub is_active: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub entity_id: String,
    pub version: i64,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id.to_string(),
            email: u.email,
            name: u.name,
            role: u.role,
            is_active: u.is_active,
            last_login_at: u.last_login_at,
            created_at: u.created_at,
            updated_at: u.updated_at,
            entity_id: u.entity_id,
            version: u.version,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginResult {
    pub mode: LoginMode,
    pub user: UserResponse,
}

#[derive(Debug, Deserialize)]
pub struct LoginArgs {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub entity_id_hint: Option<String>,
}

// --- testable `_impl` helpers -------------------------------------------

pub async fn auth_login_impl(state: &AppState, args: LoginArgs) -> AppResult<LoginResult> {
    let svc = state
        .auth_service()
        .ok_or_else(|| AppError::Configuration("auth service unavailable".into()))?;
    let server_url = state.sync_server_url().await;
    let entity_hint = args
        .entity_id_hint
        .unwrap_or_else(|| "unscoped".to_string());

    let result = svc
        .login(
            server_url.as_deref(),
            &args.email,
            &args.password,
            &entity_hint,
        )
        .await?;

    let ctx = UserContext {
        user_id: result.user_id.to_string(),
        entity_id: result.entity_id.clone(),
        email: result.email.clone(),
        name: Some(result.name.clone()),
        role: result.role.to_string(),
    };
    state.set_current_user(ctx).await;
    if let (Some(token), Some(exp)) = (result.access_token, result.access_token_expires_at) {
        state.set_current_token(token, exp.timestamp()).await;
    }
    // DEF-007 G01: cache the refresh token in AppState so a later
    // `auth_refresh` IPC can rotate it without the frontend touching
    // the raw value.
    state.set_refresh_token(result.refresh_token).await;

    Ok(LoginResult {
        mode: result.mode,
        user: UserResponse {
            id: result.user_id.to_string(),
            email: result.email,
            name: result.name,
            role: result.role,
            is_active: true,
            last_login_at: Some(Utc::now()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            entity_id: result.entity_id,
            version: 1,
        },
    })
}

pub async fn auth_logout_impl(state: &AppState) -> AppResult<()> {
    // DEF-007 G18: write a `logout` audit row BEFORE clearing the session
    // so the actor + entity_id are still resolvable. Failures emitting
    // the audit row are non-fatal -- a corrupt audit pipeline must not
    // strand a session. Tests with a fully-wired AuthService assert the
    // row IS written.
    if let Some(ctx) = state.get_current_user().await {
        if let Some(svc) = state.auth_service() {
            if let Ok(user_id) = Uuid::parse_str(&ctx.user_id) {
                if let Err(err) = svc.write_logout_audit(user_id, &ctx.entity_id).await {
                    tracing::warn!(error = %err, "logout audit write failed; continuing");
                }
            }
        }
    }
    state.clear_auth().await;
    Ok(())
}

pub async fn auth_current_user_impl(state: &AppState) -> AppResult<Option<UserContext>> {
    Ok(state.get_current_user().await)
}

pub async fn auth_lock_impl(state: &AppState) -> AppResult<()> {
    state.set_locked(true).await;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct UnlockArgs {
    pub password: String,
}

pub async fn auth_unlock_impl(state: &AppState, args: UnlockArgs) -> AppResult<()> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let user_id = Uuid::parse_str(&ctx.user_id)?;
    let svc = state
        .auth_service()
        .ok_or_else(|| AppError::Configuration("auth service unavailable".into()))?;
    svc.verify_user_password(user_id, &args.password).await?;
    state.set_locked(false).await;
    Ok(())
}

pub async fn auth_is_locked_impl(state: &AppState) -> AppResult<bool> {
    Ok(state.is_locked().await)
}

// --- Tauri command wrappers --------------------------------------------

#[tauri::command]
#[instrument(skip(state, app, args))]
pub async fn auth_login(
    app: AppHandle,
    state: State<'_, AppState>,
    args: LoginArgs,
) -> AppResult<LoginResult> {
    let result = auth_login_impl(&state, args).await?;
    let _ = app.emit("auth:changed", &result.mode);
    Ok(result)
}

#[tauri::command]
#[instrument(skip(state, app))]
pub async fn auth_logout(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    auth_logout_impl(&state).await?;
    let _ = app.emit("auth:changed", "logout");
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn auth_current_user(state: State<'_, AppState>) -> AppResult<Option<UserContext>> {
    auth_current_user_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state, app))]
pub async fn auth_lock(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    auth_lock_impl(&state).await?;
    let _ = app.emit("auth:lock", ());
    Ok(())
}

#[tauri::command]
#[instrument(skip(state, app, args))]
pub async fn auth_unlock(
    app: AppHandle,
    state: State<'_, AppState>,
    args: UnlockArgs,
) -> AppResult<()> {
    auth_unlock_impl(&state, args).await?;
    let _ = app.emit("auth:unlock", ());
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn auth_is_locked(state: State<'_, AppState>) -> AppResult<bool> {
    auth_is_locked_impl(&state).await
}

// ---- Users CRUD ---------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UsersListArgs {
    #[serde(default)]
    pub include_inactive: bool,
}

pub async fn users_list_impl(
    state: &AppState,
    args: UsersListArgs,
) -> AppResult<Vec<UserResponse>> {
    let repo = state
        .user_repo()
        .ok_or_else(|| AppError::Configuration("user repo unavailable".into()))?;
    // DEF-006 fix (P02-G29): the build spec §7.28 says `users::list` only
    // exposes inactive rows to a Superadmin. The repo trusts the flag; the
    // IPC layer is the role-gate. Force the flag off for any non-superadmin
    // caller so a tampered frontend cannot widen the view.
    let (_, actor_role, _) = current_actor(state).await?;
    let include_inactive = matches!(actor_role, UserRole::Superadmin) && args.include_inactive;
    let users = repo
        .list(crate::domains::auth::domain::repositories::UserListFilter {
            include_inactive,
            entity_id: None,
        })
        .await?;
    Ok(users.into_iter().map(UserResponse::from).collect())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn users_list(
    state: State<'_, AppState>,
    args: UsersListArgs,
) -> AppResult<Vec<UserResponse>> {
    users_list_impl(&state, args).await
}

#[derive(Debug, Deserialize)]
pub struct UserIdArgs {
    pub id: String,
}

pub async fn users_get_impl(state: &AppState, args: UserIdArgs) -> AppResult<UserResponse> {
    let id = Uuid::parse_str(&args.id)?;
    let repo = state
        .user_repo()
        .ok_or_else(|| AppError::Configuration("user repo unavailable".into()))?;
    let user = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user {id}")))?;
    Ok(user.into())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn users_get(state: State<'_, AppState>, args: UserIdArgs) -> AppResult<UserResponse> {
    users_get_impl(&state, args).await
}

#[derive(Debug, Deserialize)]
pub struct UserCreateArgs {
    pub email: String,
    pub name: String,
    pub role: UserRole,
    pub password: String,
}

pub async fn current_actor(state: &AppState) -> AppResult<(Uuid, UserRole, String)> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation(format!("invalid role: {}", ctx.role)))?;
    Ok((id, role, ctx.entity_id))
}

pub async fn users_create_impl(state: &AppState, args: UserCreateArgs) -> AppResult<UserResponse> {
    let (actor_id, role, entity_id) = current_actor(state).await?;
    let svc = state
        .user_service()
        .ok_or_else(|| AppError::Configuration("user service unavailable".into()))?;
    let user = svc
        .create(
            actor_id,
            role,
            UserCreateInput {
                email: args.email,
                name: args.name,
                role: args.role,
                password: args.password,
                entity_id,
            },
        )
        .await?;
    Ok(user.into())
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn users_create(
    state: State<'_, AppState>,
    args: UserCreateArgs,
) -> AppResult<UserResponse> {
    users_create_impl(&state, args).await
}

#[derive(Debug, Deserialize)]
pub struct UserUpdateArgs {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<UserRole>,
}

pub async fn users_update_impl(state: &AppState, args: UserUpdateArgs) -> AppResult<UserResponse> {
    let (actor_id, role, _) = current_actor(state).await?;
    let target_id = Uuid::parse_str(&args.id)?;
    let svc = state
        .user_service()
        .ok_or_else(|| AppError::Configuration("user service unavailable".into()))?;
    let user = svc
        .update(
            actor_id,
            role,
            target_id,
            UserUpdateInput {
                email: args.email,
                name: args.name,
                role: args.role,
            },
        )
        .await?;
    Ok(user.into())
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn users_update(
    state: State<'_, AppState>,
    args: UserUpdateArgs,
) -> AppResult<UserResponse> {
    users_update_impl(&state, args).await
}

pub async fn users_soft_delete_impl(state: &AppState, args: UserIdArgs) -> AppResult<()> {
    let (actor_id, role, _) = current_actor(state).await?;
    let target_id = Uuid::parse_str(&args.id)?;
    let svc = state
        .user_service()
        .ok_or_else(|| AppError::Configuration("user service unavailable".into()))?;
    svc.soft_delete(actor_id, role, target_id).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn users_soft_delete(state: State<'_, AppState>, args: UserIdArgs) -> AppResult<()> {
    users_soft_delete_impl(&state, args).await
}

#[derive(Debug, Deserialize)]
pub struct UserResetPasswordArgs {
    pub id: String,
    pub new_password: String,
}

pub async fn users_reset_password_impl(
    state: &AppState,
    args: UserResetPasswordArgs,
) -> AppResult<()> {
    let (actor_id, role, _) = current_actor(state).await?;
    let target_id = Uuid::parse_str(&args.id)?;
    let svc = state
        .user_service()
        .ok_or_else(|| AppError::Configuration("user service unavailable".into()))?;
    svc.reset_password(actor_id, role, target_id, &args.new_password)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn users_reset_password(
    state: State<'_, AppState>,
    args: UserResetPasswordArgs,
) -> AppResult<()> {
    users_reset_password_impl(&state, args).await
}

#[derive(Debug, Deserialize)]
pub struct FirstAdminArgs {
    pub email: String,
    pub name: String,
    pub password: String,
    #[serde(default)]
    pub entity_id: Option<String>,
}

pub async fn auth_has_any_user_impl(state: &AppState) -> AppResult<bool> {
    let repo = state
        .user_repo()
        .ok_or_else(|| AppError::Configuration("user repo unavailable".into()))?;
    Ok(repo.count().await? > 0)
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn auth_has_any_user(state: State<'_, AppState>) -> AppResult<bool> {
    auth_has_any_user_impl(&state).await
}

pub async fn users_create_first_admin_impl(
    state: &AppState,
    args: FirstAdminArgs,
) -> AppResult<UserResponse> {
    let svc = state
        .auth_service()
        .ok_or_else(|| AppError::Configuration("auth service unavailable".into()))?;
    let entity_id = args.entity_id.unwrap_or_else(|| "unscoped".to_string());
    let user = svc
        .create_first_admin(&args.email, &args.name, &args.password, &entity_id)
        .await?;

    let ctx = UserContext {
        user_id: user.id.to_string(),
        entity_id: user.entity_id.clone(),
        email: user.email.clone(),
        name: Some(user.name.clone()),
        role: user.role.to_string(),
    };
    state.set_current_user(ctx).await;

    // Mirror the superadmin to the sync server (best-effort) and cache the
    // JWT so the sync engine can authenticate. Failures here are non-fatal --
    // the local bootstrap already succeeded.
    if let Some(server_url) = state.sync_server_url().await {
        if !server_url.is_empty() {
            svc.bootstrap_remote_superadmin(
                &server_url,
                user.id,
                &args.email,
                &args.name,
                &args.password,
                &entity_id,
            )
            .await?;
            match svc
                .login(Some(&server_url), &args.email, &args.password, &entity_id)
                .await
            {
                Ok(result) => {
                    if let (Some(token), Some(exp)) =
                        (result.access_token, result.access_token_expires_at)
                    {
                        state.set_current_token(token, exp.timestamp()).await;
                    }
                    state.set_refresh_token(result.refresh_token).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "online login after bootstrap failed; sync will retry");
                }
            }
        }
    }

    Ok(user.into())
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn users_create_first_admin(
    state: State<'_, AppState>,
    args: FirstAdminArgs,
) -> AppResult<UserResponse> {
    users_create_first_admin_impl(&state, args).await
}

// ---- DEF-007 G01: auth::refresh -------------------------------------------

/// Payload emitted on the Tauri event `auth:refreshed` after a successful
/// rotation. Subscribers (`useCurrentUser`, `<UserMenu>`, the sync engine
/// token-bus) use the timestamp to invalidate caches.
#[derive(Debug, Clone, Serialize)]
pub struct AuthRefreshedEvent {
    pub refreshed_at: DateTime<Utc>,
}

pub async fn auth_refresh_impl(state: &AppState) -> AppResult<AuthRefreshedEvent> {
    let svc = state
        .auth_service()
        .ok_or_else(|| AppError::Configuration("auth service unavailable".into()))?;
    let refresh_token = state
        .get_refresh_token()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let server_url = state.sync_server_url().await;
    let result = svc.refresh(server_url.as_deref(), &refresh_token).await?;
    state
        .set_current_token(
            result.access_token,
            result.access_token_expires_at.timestamp(),
        )
        .await;
    state.set_refresh_token(Some(result.refresh_token)).await;
    Ok(AuthRefreshedEvent {
        refreshed_at: result.refreshed_at,
    })
}

#[tauri::command]
#[instrument(skip(state, app))]
pub async fn auth_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<AuthRefreshedEvent> {
    let event = auth_refresh_impl(&state).await?;
    let _ = app.emit("auth:refreshed", &event);
    Ok(event)
}

// ---- DEF-007 G31: auth::change_password (online-required) -----------------

#[derive(Debug, Deserialize)]
pub struct ChangePasswordArgs {
    pub current_password: String,
    pub new_password: String,
}

pub async fn auth_change_password_impl(
    state: &AppState,
    args: ChangePasswordArgs,
) -> AppResult<()> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let user_id = Uuid::parse_str(&ctx.user_id)?;
    let svc = state
        .auth_service()
        .ok_or_else(|| AppError::Configuration("auth service unavailable".into()))?;
    let access_token = state
        .get_current_token()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let server_url = state.sync_server_url().await;
    svc.change_password(
        server_url.as_deref(),
        &access_token,
        user_id,
        &args.current_password,
        &args.new_password,
    )
    .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn auth_change_password(
    state: State<'_, AppState>,
    args: ChangePasswordArgs,
) -> AppResult<()> {
    auth_change_password_impl(&state, args).await
}

// ---- DEF-007 G08 / G21: bootstrap + verify pinned JWT public key --------

use crate::domains::auth::infrastructure::{
    pin_public_key, read_pinned_pem, BootstrapOutcome, JwtVerifier,
};

#[derive(Debug, Deserialize)]
pub struct BootstrapJwtKeyArgs {
    /// Server URL to fetch the PEM from. Falls back to
    /// `state.sync_server_url()` when omitted.
    #[serde(default)]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapJwtKeyResult {
    pub outcome: BootstrapOutcome,
    /// SHA-256 of the pinned bytes (lowercase hex). Surfaced for
    /// telemetry + audit -- the actual PEM never crosses the IPC
    /// boundary.
    pub pinned_sha256: String,
}

/// Test-friendly impl that accepts an already-fetched PEM. Production
/// `auth_bootstrap_jwt_key` IPC pulls the PEM from the server.
pub async fn auth_bootstrap_jwt_key_with_pem(
    app_data_dir: &std::path::Path,
    pem_bytes: &[u8],
) -> AppResult<BootstrapJwtKeyResult> {
    let outcome = pin_public_key(app_data_dir, pem_bytes)?;
    let verifier = JwtVerifier::from_pinned_file(app_data_dir)?;
    Ok(BootstrapJwtKeyResult {
        outcome,
        pinned_sha256: verifier.pinned_bytes_sha256_hex(),
    })
}

/// Read the SHA-256 of the currently-pinned key WITHOUT exposing the
/// bytes. Returns `None` when no pin exists -- the caller is expected
/// to invoke `auth_bootstrap_jwt_key` to remediate.
pub fn auth_pinned_jwt_key_sha256(app_data_dir: &std::path::Path) -> AppResult<Option<String>> {
    let bytes = match read_pinned_pem(app_data_dir)? {
        Some(b) => b,
        None => return Ok(None),
    };
    let verifier = JwtVerifier::from_pem_bytes(&bytes)?;
    Ok(Some(verifier.pinned_bytes_sha256_hex()))
}

#[tauri::command]
#[instrument(skip(app, state, args))]
pub async fn auth_bootstrap_jwt_key(
    app: AppHandle,
    state: State<'_, AppState>,
    args: BootstrapJwtKeyArgs,
) -> AppResult<BootstrapJwtKeyResult> {
    let server_url = match args.server_url {
        Some(u) => u,
        None => state.sync_server_url().await.ok_or_else(|| {
            AppError::Configuration(
                "sync server URL not configured -- run first-launch setup".into(),
            )
        })?,
    };
    let url = format!("{}/auth/public-key", server_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(AppError::from)?;
    if !resp.status().is_success() {
        return Err(AppError::SyncUnavailable(format!(
            "bootstrap_jwt_key {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )));
    }
    let pem_bytes = resp.bytes().await.map_err(AppError::from)?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?;
    auth_bootstrap_jwt_key_with_pem(&app_data_dir, &pem_bytes).await
}

#[tauri::command]
#[instrument(skip(app))]
pub async fn auth_jwt_pinned_sha256(app: AppHandle) -> AppResult<Option<String>> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?;
    auth_pinned_jwt_key_sha256(&app_data_dir)
}
