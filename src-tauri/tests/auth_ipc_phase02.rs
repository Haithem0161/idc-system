//! Phase-02 §2.2 IPC handler tests.
//!
//! Each `#[tauri::command]` in `domains/auth/commands.rs` + `domains/settings/commands.rs`
//! delegates to a plain `_impl(&AppState, args)` async fn. We exercise those
//! helpers directly with an AppState built via `AppState::for_phase02_tests`,
//! which wires the auth + users + settings services without standing up the
//! full app graph.
//!
//! Coverage: happy path + at least one error path per command, plus the IPC
//! return-shape assertions the §3.2 plan calls out (UserResponse never carries
//! `password_hash`, LoginResult shape, role serialization, etc.).

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::commands::{
    auth_bootstrap_jwt_key_with_pem, auth_change_password_impl, auth_current_user_impl,
    auth_is_locked_impl, auth_lock_impl, auth_login_impl, auth_logout_impl, auth_refresh_impl,
    auth_unlock_impl, current_actor, users_create_first_admin_impl, users_create_impl,
    users_get_impl, users_list_impl, users_reset_password_impl, users_soft_delete_impl,
    users_update_impl, ChangePasswordArgs, FirstAdminArgs, LoginArgs, UnlockArgs, UserCreateArgs,
    UserIdArgs, UserResetPasswordArgs, UserUpdateArgs, UsersListArgs,
};
use app_lib::domains::auth::domain::value_objects::{LoginMode, UserRole};
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::infrastructure::{BootstrapOutcome, JwtVerifier};
use app_lib::domains::auth::AuthService;
use app_lib::domains::auth::UserService;
use app_lib::domains::settings::commands::{
    settings_get_impl, settings_list_impl, settings_set_locale_impl, settings_update_impl,
    SetLocaleArgs, SettingKeyArgs, SettingUpdateArgs,
};
use app_lib::domains::settings::domain::value_objects::SettingValue;
use app_lib::domains::settings::infrastructure::SqliteSettingRepo;
use app_lib::domains::settings::service::SettingsService;
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use app_lib::error::AppError;
use app_lib::state::{AppState, UserContext};
use app_lib::sync::{SyncEngine, SyncEngineConfig, SyncEngineHandle};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::mock_app;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn fresh_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    migrations::run(&pool).await.unwrap();
    pool
}

struct Rig {
    state: AppState,
    pool: SqlitePool,
    user_repo: Arc<SqliteUserRepo>,
    _app: tauri::App<tauri::test::MockRuntime>,
    _cancel: CancellationToken,
}

async fn rig(server_url: Option<&str>) -> Rig {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let setting_repo = Arc::new(SqliteSettingRepo::new(pool.clone()));
    let outbox_repo = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state_repo = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        "dev-A".into(),
    ));
    let user_service = Arc::new(UserService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        "dev-A".into(),
    ));
    let settings_service = Arc::new(SettingsService::new(
        pool.clone(),
        setting_repo,
        audit_repo.clone(),
        outbox_repo.clone(),
        "dev-A".into(),
    ));

    let mock = mock_app();
    let handle = mock.handle().clone();
    let cancel = CancellationToken::new();
    let engine: SyncEngineHandle = SyncEngine::spawn(
        SyncEngineConfig {
            pool: pool.clone(),
            outbox_repo: outbox_repo.clone(),
            audit_repo: audit_repo.clone(),
            state_repo,
            device_id: "dev-A".into(),
            app_version: "0.1.0".into(),
            initial_server_url: server_url.map(|s| s.to_string()),
            initial_token: None,
            entity_id_tenant: "tenant-1".into(),
            refresh_hook: None,
        },
        handle,
        cancel.clone(),
    );

    let state = AppState::for_phase02_tests(
        pool.clone(),
        engine,
        auth_service,
        user_service,
        settings_service,
        user_repo.clone(),
        "dev-A".into(),
        "0.1.0".into(),
        server_url.map(|s| s.to_string()),
    );

    Rig {
        state,
        pool,
        user_repo,
        _app: mock,
        _cancel: cancel,
    }
}

/// Seed a logged-in local superadmin for tests that need an authenticated
/// actor (refresh, change-password, etc.).
///
/// This deliberately seeds LOCALLY via the service's `create_first_admin` +
/// an offline login, rather than the `users_create_first_admin_impl` command.
/// The command is now SERVER-AUTHORITATIVE (it POSTs to `/auth/bootstrap-
/// superadmin` and requires success when a sync URL is set); a rig pointed at a
/// MockServer that only mocks `/auth/refresh` would 404 on bootstrap. These
/// tests only need a local logged-in admin, so we create one directly. The
/// server-first bootstrap path itself is covered by `bootstrap_creates_*` tests
/// (with the bootstrap endpoint mocked) and the live roundtrip gate.
async fn bootstrap_superadmin(rig: &Rig) -> String {
    let svc = rig.state.auth_service().expect("auth service");
    let user = svc
        .create_first_admin("admin@idc.io", "Mariam", "admin-pass", "tenant-1")
        .await
        .unwrap();
    // Mirror the command's post-create state: log in offline so a current user
    // + (offline) session exists for the refresh/change-password assertions.
    auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    user.id.to_string()
}

// --- auth_login ----------------------------------------------------------

#[tokio::test]
async fn auth_login_offline_returns_login_result_with_user_response_and_sets_state() {
    let rig = rig(None).await;
    let admin_id = bootstrap_superadmin(&rig).await;
    // Bootstrap auto-logs-in; sign out to test login fresh.
    auth_logout_impl(&rig.state).await.unwrap();

    let result = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.mode, LoginMode::Offline);
    assert_eq!(result.user.id, admin_id);
    assert_eq!(result.user.role, UserRole::Superadmin);
    assert_eq!(result.user.email, "admin@idc.io");
    let ctx = rig.state.get_current_user().await.unwrap();
    assert_eq!(ctx.user_id, admin_id);
    assert_eq!(ctx.role, "superadmin");
}

#[tokio::test]
async fn auth_login_serialized_response_never_contains_password_hash() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    auth_logout_impl(&rig.state).await.unwrap();
    let result = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    let json = serde_json::to_string(&result).unwrap();
    assert!(
        !json.contains("password_hash") && !json.contains("$argon2id$"),
        "IPC envelope must not contain password_hash: {json}"
    );
}

#[tokio::test]
async fn auth_login_wrong_password_returns_not_authenticated() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    auth_logout_impl(&rig.state).await.unwrap();
    let err = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "WRONG".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
    // The error path must NOT populate the user context.
    assert!(rig.state.get_current_user().await.is_none());
}

#[tokio::test]
async fn auth_login_defaults_entity_id_hint_to_unscoped_when_missing() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    auth_logout_impl(&rig.state).await.unwrap();

    // Same bootstrap created the user under "tenant-1". With no hint, the
    // implementation defaults to "unscoped" -- which fails offline since the
    // local row lives under "tenant-1". This pins the default behaviour.
    let err = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

// --- auth_logout ---------------------------------------------------------

#[tokio::test]
async fn auth_logout_clears_user_context_and_token() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    assert!(rig.state.get_current_user().await.is_some());

    auth_logout_impl(&rig.state).await.unwrap();
    assert!(rig.state.get_current_user().await.is_none());
    assert!(rig.state.get_current_token().await.is_none());
}

#[tokio::test]
async fn auth_logout_is_idempotent_on_a_signed_out_state() {
    let rig = rig(None).await;
    // No bootstrap; signed out by default.
    auth_logout_impl(&rig.state).await.unwrap();
    auth_logout_impl(&rig.state).await.unwrap();
    assert!(rig.state.get_current_user().await.is_none());
}

// DEF-007 G18 FIX VERIFICATION: auth_logout MUST emit one audit_log row with
// `action='logout'`, `entity='users'`, `actor_user_id=<current user>` BEFORE
// clearing the session. Without this row, the forensic trail has a hole
// between the prior `login` row and the next state-changing action -- a
// superadmin reviewing the audit log cannot tell when a session ended.
#[tokio::test]
async fn p02_g18_auth_logout_writes_audit_row_def_007_g18_fixed() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let user_ctx = rig.state.get_current_user().await.expect("user signed in");

    let (before,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action = 'logout'")
            .fetch_one(&rig.pool)
            .await
            .unwrap();
    assert_eq!(before, 0, "no prior logout rows before this test runs");

    auth_logout_impl(&rig.state).await.unwrap();

    let (after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action = 'logout'")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert_eq!(
        after, 1,
        "DEF-007 G18 fix: logout must write exactly one audit row",
    );

    // Pin the action + entity + actor + delta shape so future regressions
    // surface here. The audit row's actor_user_id must match the cleared
    // session's user (resolved BEFORE clear_auth).
    let (action, entity, actor, delta_str): (String, String, String, String) = sqlx::query_as(
        "SELECT action, entity, actor_user_id, delta FROM audit_log \
         WHERE action = 'logout' ORDER BY at DESC LIMIT 1",
    )
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert_eq!(action, "logout");
    assert_eq!(entity, "users");
    assert_eq!(
        actor, user_ctx.user_id,
        "actor must be the user who just logged out"
    );
    let parsed: serde_json::Value = serde_json::from_str(&delta_str).unwrap();
    assert_eq!(parsed["mode"], "manual");

    // The session is still cleared after the audit row lands.
    assert!(rig.state.get_current_user().await.is_none());

    // The audit row carries the entity_id_tenant matching the actor's tenant.
    let (tenant,): (String,) =
        sqlx::query_as("SELECT entity_id_tenant FROM audit_log WHERE action = 'logout' LIMIT 1")
            .fetch_one(&rig.pool)
            .await
            .unwrap();
    assert_eq!(tenant, user_ctx.entity_id);

    // Sync envelope: the logout audit row is enqueued into outbox so the
    // server-side audit query sees client logouts. (Mirrors DEF-005 login.)
    let (outbox_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'audit_log'")
            .fetch_one(&rig.pool)
            .await
            .unwrap();
    assert!(
        outbox_count >= 1,
        "outbox must carry at least one audit_log push after logout (got {outbox_count})",
    );
}

#[tokio::test]
async fn p02_g18_auth_logout_signed_out_state_emits_no_audit_row() {
    // DEF-007 G18 negative case: if no user is signed in, logout is a no-op
    // and must NOT emit a stray audit row (no actor to attribute it to).
    let rig = rig(None).await;
    // No bootstrap.
    assert!(rig.state.get_current_user().await.is_none());

    auth_logout_impl(&rig.state).await.unwrap();

    let (after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert_eq!(after, 0, "signed-out logout must not write an audit row");
}

// --- auth_current_user ---------------------------------------------------

#[tokio::test]
async fn auth_current_user_returns_none_when_signed_out() {
    let rig = rig(None).await;
    let result = auth_current_user_impl(&rig.state).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn auth_current_user_returns_context_when_signed_in() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let result = auth_current_user_impl(&rig.state).await.unwrap();
    let ctx = result.unwrap();
    assert_eq!(ctx.email, "admin@idc.io");
    assert_eq!(ctx.role, "superadmin");
    assert_eq!(ctx.entity_id, "tenant-1");
}

// --- auth_lock / auth_unlock / auth_is_locked ---------------------------

#[tokio::test]
async fn auth_lock_sets_is_locked_true() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    assert!(!auth_is_locked_impl(&rig.state).await.unwrap());
    auth_lock_impl(&rig.state).await.unwrap();
    assert!(auth_is_locked_impl(&rig.state).await.unwrap());
}

#[tokio::test]
async fn auth_unlock_with_correct_password_clears_locked_flag_and_preserves_session() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    auth_lock_impl(&rig.state).await.unwrap();

    auth_unlock_impl(
        &rig.state,
        UnlockArgs {
            password: "admin-pass".into(),
        },
    )
    .await
    .unwrap();
    assert!(!auth_is_locked_impl(&rig.state).await.unwrap());
    // Session must survive the unlock.
    assert!(rig.state.get_current_user().await.is_some());
}

#[tokio::test]
async fn auth_unlock_with_wrong_password_returns_not_authenticated_and_keeps_locked() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    auth_lock_impl(&rig.state).await.unwrap();

    let err = auth_unlock_impl(
        &rig.state,
        UnlockArgs {
            password: "WRONG-PASS".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
    // Lock state must NOT clear on failure.
    assert!(auth_is_locked_impl(&rig.state).await.unwrap());
}

#[tokio::test]
async fn auth_unlock_when_signed_out_returns_not_authenticated() {
    let rig = rig(None).await;
    // No bootstrap -> no signed-in user.
    let err = auth_unlock_impl(
        &rig.state,
        UnlockArgs {
            password: "anything".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn auth_is_locked_default_is_false() {
    let rig = rig(None).await;
    assert!(!auth_is_locked_impl(&rig.state).await.unwrap());
}

// --- users_list ----------------------------------------------------------

#[tokio::test]
async fn users_list_returns_array_with_admin_only_after_bootstrap() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let users = users_list_impl(
        &rig.state,
        UsersListArgs {
            include_inactive: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].email, "admin@idc.io");
}

#[tokio::test]
async fn users_list_response_never_contains_password_hash() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let users = users_list_impl(
        &rig.state,
        UsersListArgs {
            include_inactive: false,
        },
    )
    .await
    .unwrap();
    let json = serde_json::to_string(&users).unwrap();
    assert!(!json.contains("password_hash"));
    assert!(!json.contains("$argon2id$"));
}

// --- users_get -----------------------------------------------------------

#[tokio::test]
async fn users_get_returns_user_response_by_id() {
    let rig = rig(None).await;
    let admin_id = bootstrap_superadmin(&rig).await;
    let user = users_get_impl(
        &rig.state,
        UserIdArgs {
            id: admin_id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(user.id, admin_id);
    assert_eq!(user.email, "admin@idc.io");
}

#[tokio::test]
async fn users_get_returns_not_found_for_unknown_id() {
    let rig = rig(None).await;
    let unknown = uuid::Uuid::now_v7().to_string();
    let err = users_get_impl(&rig.state, UserIdArgs { id: unknown })
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn users_get_rejects_invalid_uuid() {
    let rig = rig(None).await;
    let err = users_get_impl(
        &rig.state,
        UserIdArgs {
            id: "not-a-uuid".into(),
        },
    )
    .await
    .unwrap_err();
    // The Uuid::parse_str -> AppError::Validation("uuid: ...") path.
    assert!(matches!(err, AppError::Validation(_)));
}

// --- users_create --------------------------------------------------------

#[tokio::test]
async fn users_create_returns_user_response_and_persists_under_actor_tenant() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let new_user = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "new@idc.io".into(),
            name: "New User".into(),
            role: UserRole::Receptionist,
            password: "newpass-1234".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(new_user.email, "new@idc.io");
    assert_eq!(new_user.role, UserRole::Receptionist);
    assert_eq!(new_user.entity_id, "tenant-1");

    // Persisted.
    let users = users_list_impl(
        &rig.state,
        UsersListArgs {
            include_inactive: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(users.len(), 2);
}

// DEF-006 (P02-G29) fix: a non-superadmin caller MUST NOT receive inactive
// rows back even when they pass `include_inactive=true`. The IPC layer
// `users_list_impl` forces the flag off based on the caller's role.
#[tokio::test]
async fn users_list_receptionist_caller_never_sees_inactive_rows_def_006_fixed() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let receptionist = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "receptionist@idc.io".into(),
            name: "Mehdi".into(),
            role: UserRole::Receptionist,
            password: "receptpass-1".into(),
        },
    )
    .await
    .unwrap();
    let inactive = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "ghost@idc.io".into(),
            name: "Ghost".into(),
            role: UserRole::Receptionist,
            password: "ghostpass-1".into(),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET is_active = 0 WHERE id = ?")
        .bind(inactive.id.to_string())
        .execute(&rig.pool)
        .await
        .unwrap();

    // Switch session to the receptionist.
    auth_logout_impl(&rig.state).await.unwrap();
    auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "receptionist@idc.io".into(),
            password: "receptpass-1".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();

    // Even though include_inactive=true, the IPC must downgrade it for
    // non-superadmins. The ghost row stays hidden.
    let users = users_list_impl(
        &rig.state,
        UsersListArgs {
            include_inactive: true,
        },
    )
    .await
    .unwrap();
    assert!(
        users.iter().all(|u| u.is_active),
        "DEF-006: receptionist must not see inactive rows even with include_inactive=true"
    );
    assert!(
        users.iter().any(|u| u.id == receptionist.id),
        "active receptionist should still appear"
    );
    assert!(
        users.iter().all(|u| u.id != inactive.id),
        "inactive ghost row must be filtered out for non-superadmin"
    );

    // Sanity: switch back to the superadmin; with the flag set, ghost reappears.
    auth_logout_impl(&rig.state).await.unwrap();
    auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    let users = users_list_impl(
        &rig.state,
        UsersListArgs {
            include_inactive: true,
        },
    )
    .await
    .unwrap();
    assert!(
        users.iter().any(|u| u.id == inactive.id && !u.is_active),
        "DEF-006: superadmin with include_inactive=true must see inactive rows"
    );
}

#[tokio::test]
async fn users_create_rejects_when_signed_out_with_not_authenticated() {
    let rig = rig(None).await;
    let err = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "x@idc.io".into(),
            name: "X".into(),
            role: UserRole::Receptionist,
            password: "newpass-1234".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn users_create_rejects_receptionist_caller_with_validation_error() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    // Force role to receptionist.
    rig.state
        .set_current_user(UserContext {
            user_id: uuid::Uuid::now_v7().to_string(),
            entity_id: "tenant-1".into(),
            email: "rx@idc.io".into(),
            name: Some("RX".into()),
            role: "receptionist".into(),
        })
        .await;
    let err = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "x@idc.io".into(),
            name: "X".into(),
            role: UserRole::Receptionist,
            password: "newpass-1234".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// --- users_update --------------------------------------------------------

#[tokio::test]
async fn users_update_returns_updated_user_response_with_bumped_version() {
    let rig = rig(None).await;
    let admin_id = bootstrap_superadmin(&rig).await;
    let created = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "u@idc.io".into(),
            name: "U".into(),
            role: UserRole::Receptionist,
            password: "newpass-1234".into(),
        },
    )
    .await
    .unwrap();

    let updated = users_update_impl(
        &rig.state,
        UserUpdateArgs {
            id: created.id.clone(),
            email: None,
            name: Some("Renamed".into()),
            role: Some(UserRole::Accountant),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.role, UserRole::Accountant);
    assert!(updated.version > created.version);
    // Admin unchanged.
    let admin = users_get_impl(&rig.state, UserIdArgs { id: admin_id })
        .await
        .unwrap();
    assert_eq!(admin.name, "Mariam");
}

#[tokio::test]
async fn users_update_returns_not_found_for_unknown_id() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = users_update_impl(
        &rig.state,
        UserUpdateArgs {
            id: uuid::Uuid::now_v7().to_string(),
            email: None,
            name: Some("X".into()),
            role: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

// --- users_soft_delete ---------------------------------------------------

#[tokio::test]
async fn users_soft_delete_returns_unit_and_makes_user_invisible_to_get() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let created = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "del@idc.io".into(),
            name: "Del".into(),
            role: UserRole::Receptionist,
            password: "newpass-1234".into(),
        },
    )
    .await
    .unwrap();

    users_soft_delete_impl(
        &rig.state,
        UserIdArgs {
            id: created.id.clone(),
        },
    )
    .await
    .unwrap();
    let err = users_get_impl(
        &rig.state,
        UserIdArgs {
            id: created.id.clone(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn users_soft_delete_rejects_unauthenticated_caller() {
    let rig = rig(None).await;
    let err = users_soft_delete_impl(
        &rig.state,
        UserIdArgs {
            id: uuid::Uuid::now_v7().to_string(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

// --- users_reset_password ------------------------------------------------

#[tokio::test]
async fn users_reset_password_returns_unit_and_rotates_local_hash() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let created = users_create_impl(
        &rig.state,
        UserCreateArgs {
            email: "u@idc.io".into(),
            name: "U".into(),
            role: UserRole::Receptionist,
            password: "old-pass-1234".into(),
        },
    )
    .await
    .unwrap();

    users_reset_password_impl(
        &rig.state,
        UserResetPasswordArgs {
            id: created.id.clone(),
            new_password: "new-pass-1234".into(),
        },
    )
    .await
    .unwrap();

    // Sign out the admin so we can probe the offline-login surface as the
    // user whose password rotated.
    auth_logout_impl(&rig.state).await.unwrap();

    let err = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "u@idc.io".into(),
            password: "old-pass-1234".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));

    let ok = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "u@idc.io".into(),
            password: "new-pass-1234".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(ok.user.email, "u@idc.io");
}

#[tokio::test]
async fn users_reset_password_rejects_when_signed_out() {
    let rig = rig(None).await;
    let err = users_reset_password_impl(
        &rig.state,
        UserResetPasswordArgs {
            id: uuid::Uuid::now_v7().to_string(),
            new_password: "n".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

// --- users_create_first_admin -------------------------------------------

#[tokio::test]
async fn users_create_first_admin_returns_user_response_and_auto_logs_in() {
    let rig = rig(None).await;
    let user = users_create_first_admin_impl(
        &rig.state,
        FirstAdminArgs {
            email: "root@idc.io".into(),
            name: "Root".into(),
            password: "rootpass1".into(),
            entity_id: Some("tenant-X".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(user.role, UserRole::Superadmin);
    assert_eq!(user.entity_id, "tenant-X");
    let ctx = rig.state.get_current_user().await.unwrap();
    assert_eq!(ctx.user_id, user.id);
    assert_eq!(ctx.role, "superadmin");
}

#[tokio::test]
async fn users_create_first_admin_returns_conflict_when_any_user_exists() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = users_create_first_admin_impl(
        &rig.state,
        FirstAdminArgs {
            email: "other@idc.io".into(),
            name: "Other".into(),
            password: "otherpw1".into(),
            entity_id: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn users_create_first_admin_defaults_entity_id_to_unscoped_when_missing() {
    let rig = rig(None).await;
    let user = users_create_first_admin_impl(
        &rig.state,
        FirstAdminArgs {
            email: "root@idc.io".into(),
            name: "Root".into(),
            password: "rootpass1".into(),
            entity_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(user.entity_id, "unscoped");
}

// --- settings_list / settings_get / settings_update ---------------------

#[tokio::test]
async fn settings_list_returns_seeded_v1_keys_for_unscoped_tenant_before_login() {
    let rig = rig(None).await;
    // Migration seeds 10 required keys under entity_id='unscoped'.
    let rows = settings_list_impl(&rig.state).await.unwrap();
    assert_eq!(rows.len(), 10);
    let keys: Vec<_> = rows.iter().map(|s| s.key.as_str()).collect();
    for k in [
        "dye_cost_iqd",
        "report_cost_iqd",
        "internal_doctor_pct",
        "idle_lock_minutes",
        "arabic_numerals",
        "currency_symbol",
        "thermal_width",
        "thermal_printer_name",
        "clinic_display_name_ar",
        "clinic_display_name_en",
    ] {
        assert!(keys.contains(&k), "missing seed key: {k}");
    }
}

#[tokio::test]
async fn settings_get_returns_seeded_value_for_known_key() {
    let rig = rig(None).await;
    let row = settings_get_impl(
        &rig.state,
        SettingKeyArgs {
            key: "dye_cost_iqd".into(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.key, "dye_cost_iqd");
    assert_eq!(row.value, SettingValue::Int(10_000));
}

#[tokio::test]
async fn settings_get_returns_none_for_unknown_key() {
    let rig = rig(None).await;
    let row = settings_get_impl(
        &rig.state,
        SettingKeyArgs {
            key: "ghost_key".into(),
        },
    )
    .await
    .unwrap();
    assert!(row.is_none());
}

#[tokio::test]
async fn settings_update_happy_path_for_superadmin_persists_caches_and_bumps_version() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let updated = settings_update_impl(
        &rig.state,
        SettingUpdateArgs {
            key: "dye_cost_iqd".into(),
            value: SettingValue::Int(12_000),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.value, SettingValue::Int(12_000));
    assert!(updated.version >= 1);

    // settings_cache populated with the BARE scalar (not the tagged-enum
    // serialization) so money/receipt reads via get_setting(..).as_i64()
    // resolve. Storing the tagged object would make as_i64() None and zero
    // every visit-lock money snapshot (the C8 fix).
    let cached = rig.state.get_setting("dye_cost_iqd").await.unwrap();
    assert_eq!(
        cached.as_i64(),
        Some(12_000),
        "cache must hold a bare integer scalar, got: {cached}"
    );
}

#[tokio::test]
async fn settings_update_rejects_unauthenticated_caller() {
    let rig = rig(None).await;
    let err = settings_update_impl(
        &rig.state,
        SettingUpdateArgs {
            key: "dye_cost_iqd".into(),
            value: SettingValue::Int(99_999),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn settings_update_rejects_receptionist_caller_with_validation_error() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    rig.state
        .set_current_user(UserContext {
            user_id: uuid::Uuid::now_v7().to_string(),
            entity_id: "tenant-1".into(),
            email: "rx@idc.io".into(),
            name: Some("RX".into()),
            role: "receptionist".into(),
        })
        .await;
    let err = settings_update_impl(
        &rig.state,
        SettingUpdateArgs {
            key: "dye_cost_iqd".into(),
            value: SettingValue::Int(99_999),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn settings_update_rejects_invalid_value_for_thermal_width() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = settings_update_impl(
        &rig.state,
        SettingUpdateArgs {
            key: "thermal_width".into(),
            value: SettingValue::Int(64),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// --- current_actor helper ------------------------------------------------

#[tokio::test]
async fn current_actor_returns_uuid_role_and_entity_id_from_state_context() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let (id, role, entity_id) = current_actor(&rig.state).await.unwrap();
    assert_eq!(role, UserRole::Superadmin);
    assert_eq!(entity_id, "tenant-1");
    assert!(uuid::Uuid::nil() != id);
}

#[tokio::test]
async fn current_actor_returns_not_authenticated_when_no_context() {
    let rig = rig(None).await;
    let err = current_actor(&rig.state).await.unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn current_actor_rejects_unknown_role_string_with_validation_error() {
    let rig = rig(None).await;
    rig.state
        .set_current_user(UserContext {
            user_id: uuid::Uuid::now_v7().to_string(),
            entity_id: "tenant-1".into(),
            email: "x@idc.io".into(),
            name: Some("X".into()),
            role: "shareholder".into(),
        })
        .await;
    let err = current_actor(&rig.state).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// --- online-mode auth_login round-trip with wiremock --------------------

#[tokio::test]
async fn auth_login_online_with_wiremock_populates_token_and_caches_local_row() {
    let server = MockServer::start().await;
    let rig = rig(Some(&server.uri())).await;

    // Seed offline cache so we can later prove it remains intact too.
    let phc = app_lib::domains::auth::domain::services::hash_password("admin-pass").unwrap();
    let user_id = uuid::Uuid::now_v7();
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": "access.jwt.token",
            "refreshToken": "refresh.token",
            "expiresAt": chrono::Utc::now().to_rfc3339(),
            "user": {
                "id": user_id.to_string(),
                "email": "admin@idc.io",
                "name": "Mariam",
                "role": "superadmin",
                "entityId": "tenant-1",
                "passwordHash": phc,
            }
        })))
        .mount(&server)
        .await;

    let result = auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.mode, LoginMode::Online);
    assert_eq!(result.user.role, UserRole::Superadmin);
    // Token written to state.
    assert_eq!(
        rig.state.get_current_token().await.as_deref(),
        Some("access.jwt.token")
    );
    // Local cache populated.
    use app_lib::domains::auth::domain::repositories::UserRepo;
    let cached = rig
        .user_repo
        .get_by_email("admin@idc.io", "tenant-1")
        .await
        .unwrap();
    assert!(cached.is_some());
    let _ = rig.pool; // keep field alive in this test
}

// --- DEF-007 G16: settings_set_locale IPC --------------------------------
//
// Phase-02 §7 advertised a `set_locale` command that the frontend would call
// from the language toggle in the header. The phase-02 build only landed the
// generic `settings_update` -- so the toggle either had to roll a custom
// validation path (locale not in REQUIRED_KEYS and `validate_value_for_key`
// had no `locale` arm) or accept anything. These tests pin the contract:
//   1. The dedicated IPC validates locale in {'en','ar'} at the boundary.
//   2. `settings_update` direct-call with key="locale" is ALSO validated by
//      `validate_value_for_key` so the locale invariant survives even if
//      someone bypasses the wrapper.
//   3. The write goes through `settings_update_impl`, so the existing
//      cache + audit path is inherited (no second code path to keep in sync).

#[tokio::test]
async fn def_007_g16_settings_set_locale_accepts_en_and_persists_text_value() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let updated = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: "en".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.key, "locale");
    assert_eq!(updated.value, SettingValue::Text("en".into()));
    assert!(updated.version >= 1);
}

#[tokio::test]
async fn def_007_g16_settings_set_locale_accepts_ar_and_persists_text_value() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let updated = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: "ar".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.value, SettingValue::Text("ar".into()));
}

#[tokio::test]
async fn def_007_g16_settings_set_locale_rejects_unknown_locale_with_validation_error() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: "fr".into(),
        },
    )
    .await
    .unwrap_err();
    let msg = match &err {
        AppError::Validation(s) => s.clone(),
        other => panic!("expected Validation, got {other:?}"),
    };
    assert!(
        msg.contains("locale must be one of"),
        "validation message must enumerate allowed locales (got: {msg})"
    );
    assert!(
        msg.contains("fr"),
        "validation message must echo the rejected input (got: {msg})"
    );
}

#[tokio::test]
async fn def_007_g16_settings_set_locale_rejects_empty_string() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: String::new(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn def_007_g16_settings_set_locale_rejects_uppercase_variant_strict_match() {
    // Lower-case enforcement matches the i18next locale code convention --
    // a "EN"/"AR" admitted at the wire boundary would force every consumer
    // to also handle the canonicalised form. Reject at the source.
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: "EN".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn def_007_g16_settings_set_locale_requires_authenticated_caller() {
    let rig = rig(None).await;
    let err = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: "en".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn def_007_g16_settings_set_locale_round_trip_visible_via_settings_get() {
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let _ = settings_set_locale_impl(
        &rig.state,
        SetLocaleArgs {
            locale: "ar".into(),
        },
    )
    .await
    .unwrap();
    let read = settings_get_impl(
        &rig.state,
        SettingKeyArgs {
            key: "locale".into(),
        },
    )
    .await
    .unwrap()
    .expect("locale row must persist");
    assert_eq!(read.value, SettingValue::Text("ar".into()));
}

#[tokio::test]
async fn def_007_g16_settings_update_direct_with_invalid_locale_also_rejected_defense_in_depth() {
    // Even if a future call site bypasses `settings_set_locale` and goes
    // through the generic `settings_update`, `validate_value_for_key`
    // enforces the same enum. A regression that drops the `locale` arm
    // would silently let `settings_update { key: "locale", value: "fr" }`
    // through.
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = settings_update_impl(
        &rig.state,
        SettingUpdateArgs {
            key: "locale".into(),
            value: SettingValue::Text("fr".into()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn def_007_g16_settings_update_direct_with_int_locale_rejected_type_check() {
    // Locale must be Text, never Int. A typo in the frontend that sends
    // SettingValue::Int(1) instead of Text("en") must surface as a validation
    // error -- not silently coerce.
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let err = settings_update_impl(
        &rig.state,
        SettingUpdateArgs {
            key: "locale".into(),
            value: SettingValue::Int(1),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// =========================================================================
// DEF-007 G01: auth::refresh IPC + RefreshResult shape
// =========================================================================

#[tokio::test]
async fn def_007_g01_auth_refresh_200_rotates_tokens_and_returns_refreshed_at() {
    let server = MockServer::start().await;
    let rig = rig(Some(&server.uri())).await;
    bootstrap_superadmin(&rig).await;
    rig.state.set_refresh_token(Some("rt-v1".into())).await;
    let new_exp = chrono::Utc::now() + chrono::Duration::minutes(15);
    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": "access-v2",
            "refreshToken": "rt-v2",
            "expiresAt": new_exp.to_rfc3339(),
        })))
        .mount(&server)
        .await;

    let before = chrono::Utc::now();
    let event = auth_refresh_impl(&rig.state).await.unwrap();
    let after = chrono::Utc::now();
    assert!(
        event.refreshed_at >= before && event.refreshed_at <= after,
        "refreshed_at must reflect when the rotation completed locally"
    );
    assert_eq!(
        rig.state.get_current_token().await.as_deref(),
        Some("access-v2")
    );
    assert_eq!(
        rig.state.get_refresh_token().await.as_deref(),
        Some("rt-v2")
    );
}

#[tokio::test]
async fn def_007_g01_auth_refresh_401_returns_not_authenticated_and_leaves_state_intact() {
    let server = MockServer::start().await;
    let rig = rig(Some(&server.uri())).await;
    bootstrap_superadmin(&rig).await;
    rig.state.set_refresh_token(Some("rt-v1".into())).await;
    rig.state.set_current_token("access-v1".into(), 0).await;
    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "code": "SESSION_EXPIRED",
            "message": "refresh token expired"
        })))
        .mount(&server)
        .await;
    let err = auth_refresh_impl(&rig.state).await.unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
    // No mutation on failure.
    assert_eq!(
        rig.state.get_current_token().await.as_deref(),
        Some("access-v1")
    );
    assert_eq!(
        rig.state.get_refresh_token().await.as_deref(),
        Some("rt-v1")
    );
}

#[tokio::test]
async fn def_007_g01_auth_refresh_without_cached_refresh_token_returns_not_authenticated() {
    let rig = rig(Some("http://127.0.0.1:1")).await; // unreachable URL is fine; we should never call it.
    bootstrap_superadmin(&rig).await;
    // No set_refresh_token call.
    let err = auth_refresh_impl(&rig.state).await.unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn def_007_g01_auth_login_caches_refresh_token_into_app_state() {
    // Sibling sentinel: the new `set_refresh_token` write inside
    // `auth_login_impl` must actually fire on the online path. Without it,
    // `auth_refresh_impl` has no token to rotate.
    let server = MockServer::start().await;
    let rig = rig(Some(&server.uri())).await;
    let phc = app_lib::domains::auth::domain::services::hash_password("admin-pass").unwrap();
    let user_id = uuid::Uuid::now_v7();
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": "access.v1",
            "refreshToken": "rt.v1",
            "expiresAt": chrono::Utc::now().to_rfc3339(),
            "user": {
                "id": user_id.to_string(),
                "email": "admin@idc.io",
                "name": "Mariam",
                "role": "superadmin",
                "entityId": "tenant-1",
                "passwordHash": phc,
            }
        })))
        .mount(&server)
        .await;
    auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        rig.state.get_refresh_token().await.as_deref(),
        Some("rt.v1"),
        "auth_login must cache the refresh token so auth_refresh can rotate it"
    );
}

// =========================================================================
// DEF-007 G31: auth::change_password offline-required + online success
// =========================================================================

#[tokio::test]
async fn def_007_g31_change_password_returns_offline_not_allowed_when_no_server_url() {
    let rig = rig(None).await; // no server_url == offline
    let admin_id = bootstrap_superadmin(&rig).await;
    rig.state
        .set_current_token("access-v1".into(), chrono::Utc::now().timestamp() + 900)
        .await;
    // Capture password hash BEFORE the call.
    use app_lib::domains::auth::domain::repositories::UserRepo;
    let admin = rig
        .user_repo
        .get_by_id(uuid::Uuid::from_str(&admin_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    let phc_before = admin.password_hash.clone();
    // No audit row yet beyond bootstrap's `create`.
    let (audit_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    let err = auth_change_password_impl(
        &rig.state,
        ChangePasswordArgs {
            current_password: "admin-pass".into(),
            new_password: "new-pass-with-12-chars".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::OfflineNotAllowed));
    // Hash unchanged.
    let admin_after = rig
        .user_repo
        .get_by_id(uuid::Uuid::from_str(&admin_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(admin_after.password_hash, phc_before);
    let (audit_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert_eq!(audit_before, audit_after, "no audit row on offline branch");
}

#[tokio::test]
async fn def_007_g31_change_password_returns_offline_not_allowed_when_server_unreachable() {
    // Server URL points at a port that nothing listens on. The HTTP call
    // fails with a connection error; `change_password` MUST surface that
    // as OfflineNotAllowed (not Network) per §4 step 1.
    let rig = rig(Some("http://127.0.0.1:1")).await;
    let admin_id = bootstrap_superadmin(&rig).await;
    rig.state
        .set_current_token("access-v1".into(), chrono::Utc::now().timestamp() + 900)
        .await;
    use app_lib::domains::auth::domain::repositories::UserRepo;
    let admin = rig
        .user_repo
        .get_by_id(uuid::Uuid::from_str(&admin_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    let phc_before = admin.password_hash.clone();
    let err = auth_change_password_impl(
        &rig.state,
        ChangePasswordArgs {
            current_password: "admin-pass".into(),
            new_password: "new-pass-with-12-chars".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::OfflineNotAllowed));
    let admin_after = rig
        .user_repo
        .get_by_id(uuid::Uuid::from_str(&admin_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(admin_after.password_hash, phc_before);
}

#[tokio::test]
async fn def_007_g31_change_password_204_rotates_local_hash_and_writes_audit() {
    let server = MockServer::start().await;
    let rig = rig(Some(&server.uri())).await;
    let admin_id = bootstrap_superadmin(&rig).await;
    rig.state
        .set_current_token("access-v1".into(), chrono::Utc::now().timestamp() + 900)
        .await;
    Mock::given(method("POST"))
        .and(path("/auth/change-password"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    use app_lib::domains::auth::domain::repositories::UserRepo;
    let admin = rig
        .user_repo
        .get_by_id(uuid::Uuid::from_str(&admin_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    let phc_before = admin.password_hash.clone();
    let (audit_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    auth_change_password_impl(
        &rig.state,
        ChangePasswordArgs {
            current_password: "admin-pass".into(),
            new_password: "new-pass-12-chars-12".into(),
        },
    )
    .await
    .unwrap();
    let admin_after = rig
        .user_repo
        .get_by_id(uuid::Uuid::from_str(&admin_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_ne!(admin_after.password_hash, phc_before, "hash must rotate");
    // Verify the new hash actually validates the new password.
    app_lib::domains::auth::domain::services::verify_password(
        "new-pass-12-chars-12",
        &admin_after.password_hash,
    )
    .unwrap();
    let (audit_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert_eq!(audit_before + 1, audit_after);
    let (action,): (String,) =
        sqlx::query_as("SELECT action FROM audit_log ORDER BY at DESC LIMIT 1")
            .fetch_one(&rig.pool)
            .await
            .unwrap();
    assert_eq!(action, "password_change");
}

#[tokio::test]
async fn def_007_g31_change_password_rejects_short_new_password() {
    let rig = rig(Some("http://127.0.0.1:1")).await;
    bootstrap_superadmin(&rig).await;
    rig.state
        .set_current_token("access-v1".into(), chrono::Utc::now().timestamp() + 900)
        .await;
    let err = auth_change_password_impl(
        &rig.state,
        ChangePasswordArgs {
            current_password: "admin-pass".into(),
            new_password: "short".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// =========================================================================
// DEF-007 G08 / G21: bootstrap_jwt_key pin lifecycle
// =========================================================================

fn test_pubkey_pem() -> Vec<u8> {
    // Reuse the test fixture from the verifier module.
    include_bytes!("../src/domains/auth/infrastructure/test_data/jwt_test_public.pem").to_vec()
}
fn other_pubkey_pem() -> Vec<u8> {
    include_bytes!("../src/domains/auth/infrastructure/test_data/jwt_other_public.pem").to_vec()
}

#[tokio::test]
async fn def_007_g08_bootstrap_jwt_key_writes_pem_and_exposes_sha256() {
    let dir = tempfile::tempdir().unwrap();
    let pem = test_pubkey_pem();
    let result = auth_bootstrap_jwt_key_with_pem(dir.path(), &pem)
        .await
        .unwrap();
    assert_eq!(result.outcome, BootstrapOutcome::Bootstrapped);
    assert_eq!(result.pinned_sha256.len(), 64);
    // Pinned file exists at the documented path.
    let bytes = std::fs::read(dir.path().join("jwt_public_key.pem")).unwrap();
    assert_eq!(bytes, pem);
    // Verifier loads from the pinned file.
    let v = JwtVerifier::from_pinned_file(dir.path()).unwrap();
    assert_eq!(v.pinned_bytes_sha256_hex(), result.pinned_sha256);
}

#[tokio::test]
async fn def_007_g21_login_does_not_overwrite_pinned_jwt_public_key() {
    // The auth_login flow MUST NOT touch the pinned PEM. This test
    // demonstrates the architectural invariant: a separate IPC
    // (`auth_bootstrap_jwt_key`) is the SOLE writer of the pinned file,
    // and `auth_login_impl` is structurally incapable of writing to it
    // (it has no app_data_dir parameter, no path I/O).
    let dir = tempfile::tempdir().unwrap();
    let pem = test_pubkey_pem();
    auth_bootstrap_jwt_key_with_pem(dir.path(), &pem)
        .await
        .unwrap();
    let pinned_before = std::fs::read(dir.path().join("jwt_public_key.pem")).unwrap();

    // Drive a complete online auth_login_impl call.
    let server = MockServer::start().await;
    let rig = rig(Some(&server.uri())).await;
    let phc = app_lib::domains::auth::domain::services::hash_password("admin-pass").unwrap();
    let user_id = uuid::Uuid::now_v7();
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": "access.v1",
            "refreshToken": "rt.v1",
            "expiresAt": chrono::Utc::now().to_rfc3339(),
            "user": {
                "id": user_id.to_string(),
                "email": "admin@idc.io",
                "name": "Mariam",
                "role": "superadmin",
                "entityId": "tenant-1",
                "passwordHash": phc,
            }
        })))
        .mount(&server)
        .await;
    auth_login_impl(
        &rig.state,
        LoginArgs {
            email: "admin@idc.io".into(),
            password: "admin-pass".into(),
            entity_id_hint: Some("tenant-1".into()),
        },
    )
    .await
    .unwrap();
    // Pinned file unchanged.
    let pinned_after = std::fs::read(dir.path().join("jwt_public_key.pem")).unwrap();
    assert_eq!(pinned_before, pinned_after, "login must not mutate pin");
}

#[tokio::test]
async fn def_007_g21_bootstrap_jwt_key_refuses_to_overwrite_when_bytes_differ() {
    let dir = tempfile::tempdir().unwrap();
    let pem_a = test_pubkey_pem();
    let pem_b = other_pubkey_pem();
    auth_bootstrap_jwt_key_with_pem(dir.path(), &pem_a)
        .await
        .unwrap();
    let result = auth_bootstrap_jwt_key_with_pem(dir.path(), &pem_b)
        .await
        .unwrap();
    assert_eq!(result.outcome, BootstrapOutcome::PinMismatch);
    // Pin unchanged.
    let stored = std::fs::read(dir.path().join("jwt_public_key.pem")).unwrap();
    assert_eq!(stored, pem_a);
}

#[tokio::test]
async fn def_007_g21_bootstrap_jwt_key_replay_with_same_bytes_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let pem = test_pubkey_pem();
    auth_bootstrap_jwt_key_with_pem(dir.path(), &pem)
        .await
        .unwrap();
    let result = auth_bootstrap_jwt_key_with_pem(dir.path(), &pem)
        .await
        .unwrap();
    assert_eq!(result.outcome, BootstrapOutcome::AlreadyPinned);
}

// =========================================================================
// DEF-007 G23: settings_update_batch atomic multi-key save
// =========================================================================

#[tokio::test]
async fn def_007_g23_settings_update_batch_persists_all_keys_atomically() {
    use app_lib::domains::settings::commands::{
        settings_update_batch_impl, SettingBatchEntry, SettingsUpdateBatchArgs,
    };
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    let updated = settings_update_batch_impl(
        &rig.state,
        SettingsUpdateBatchArgs {
            entries: vec![
                SettingBatchEntry {
                    key: "arabic_numerals".into(),
                    value: SettingValue::Bool(true),
                },
                SettingBatchEntry {
                    key: "currency_symbol".into(),
                    value: SettingValue::Text("IQD".into()),
                },
                SettingBatchEntry {
                    key: "idle_lock_minutes".into(),
                    value: SettingValue::Int(20),
                },
            ],
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.len(), 3);
    assert_eq!(updated[0].key, "arabic_numerals");
    assert_eq!(updated[1].key, "currency_symbol");
    assert_eq!(updated[2].key, "idle_lock_minutes");

    // All three persisted.
    let list = settings_list_impl(&rig.state).await.unwrap();
    let by_key: std::collections::HashMap<_, _> = list.iter().map(|s| (&s.key, s)).collect();
    assert_eq!(
        by_key.get(&"arabic_numerals".to_string()).unwrap().value,
        SettingValue::Bool(true)
    );
    assert_eq!(
        by_key.get(&"currency_symbol".to_string()).unwrap().value,
        SettingValue::Text("IQD".into())
    );
    assert_eq!(
        by_key.get(&"idle_lock_minutes".to_string()).unwrap().value,
        SettingValue::Int(20)
    );
}

#[tokio::test]
async fn def_007_g23_settings_update_batch_rolls_back_all_keys_on_validation_failure() {
    // The third entry has an invalid value; the entire batch must fail
    // and no DB row should mutate.
    use app_lib::domains::settings::commands::{
        settings_update_batch_impl, SettingBatchEntry, SettingsUpdateBatchArgs,
    };
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    // Capture before-state.
    let before = settings_list_impl(&rig.state).await.unwrap();
    let before_arabic = before
        .iter()
        .find(|s| s.key == "arabic_numerals")
        .map(|s| s.value.clone());
    let before_currency = before
        .iter()
        .find(|s| s.key == "currency_symbol")
        .map(|s| s.value.clone());
    let before_idle = before
        .iter()
        .find(|s| s.key == "idle_lock_minutes")
        .map(|s| s.value.clone());

    let err = settings_update_batch_impl(
        &rig.state,
        SettingsUpdateBatchArgs {
            entries: vec![
                SettingBatchEntry {
                    key: "arabic_numerals".into(),
                    value: SettingValue::Bool(true),
                },
                SettingBatchEntry {
                    key: "currency_symbol".into(),
                    value: SettingValue::Text("IQD".into()),
                },
                SettingBatchEntry {
                    key: "idle_lock_minutes".into(),
                    value: SettingValue::Int(-1), // INVALID -- must be positive.
                },
            ],
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));

    // Every key must read back as it was before the failed batch.
    let after = settings_list_impl(&rig.state).await.unwrap();
    let after_arabic = after
        .iter()
        .find(|s| s.key == "arabic_numerals")
        .map(|s| s.value.clone());
    let after_currency = after
        .iter()
        .find(|s| s.key == "currency_symbol")
        .map(|s| s.value.clone());
    let after_idle = after
        .iter()
        .find(|s| s.key == "idle_lock_minutes")
        .map(|s| s.value.clone());
    assert_eq!(before_arabic, after_arabic, "arabic_numerals rolled back");
    assert_eq!(
        before_currency, after_currency,
        "currency_symbol rolled back"
    );
    assert_eq!(before_idle, after_idle, "idle_lock_minutes rolled back");
}

#[tokio::test]
async fn def_007_g23_settings_update_batch_rejects_non_superadmin_caller() {
    use app_lib::domains::settings::commands::{
        settings_update_batch_impl, SettingBatchEntry, SettingsUpdateBatchArgs,
    };
    let rig = rig(None).await;
    bootstrap_superadmin(&rig).await;
    // Flip the current user to receptionist.
    rig.state
        .set_current_user(UserContext {
            user_id: uuid::Uuid::now_v7().to_string(),
            entity_id: "tenant-1".into(),
            email: "rec@idc.io".into(),
            name: Some("Mehdi".into()),
            role: "receptionist".into(),
        })
        .await;
    let err = settings_update_batch_impl(
        &rig.state,
        SettingsUpdateBatchArgs {
            entries: vec![SettingBatchEntry {
                key: "arabic_numerals".into(),
                value: SettingValue::Bool(true),
            }],
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}
