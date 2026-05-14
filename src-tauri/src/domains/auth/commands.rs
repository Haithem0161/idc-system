//! Tauri commands for auth + users.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
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
    let users = repo
        .list(crate::domains::auth::domain::repositories::UserListFilter {
            include_inactive: args.include_inactive,
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
    // Auto-login the user post-bootstrap.
    let ctx = UserContext {
        user_id: user.id.to_string(),
        entity_id: user.entity_id.clone(),
        email: user.email.clone(),
        name: Some(user.name.clone()),
        role: user.role.to_string(),
    };
    state.set_current_user(ctx).await;
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
