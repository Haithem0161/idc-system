//! Phase-02 §5 canonical persona script: **P3 Mariam the Superadmin**.
//!
//! Mirror of `sync_persona_phase01.rs`: walks the canonical persona's
//! day-of-work end-to-end through the public IPC surface.
//!
//! Per `personas.md`:
//!   P3 Mariam the Superadmin -- steps 1-3 cover phase-02 surfaces
//!   (login, navigate `/admin/users` + `/admin/settings`, observe audit log).
//!
//! This script is the §8 DoD canonical persona for phase 02. It MUST pass.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::commands::{
    auth_current_user_impl, auth_lock_impl, auth_login_impl, auth_logout_impl, auth_unlock_impl,
    users_create_first_admin_impl, users_create_impl, users_get_impl, users_list_impl,
    users_reset_password_impl, users_soft_delete_impl, users_update_impl, FirstAdminArgs,
    LoginArgs, UnlockArgs, UserCreateArgs, UserIdArgs, UserResetPasswordArgs, UserUpdateArgs,
    UsersListArgs,
};
use app_lib::domains::auth::domain::value_objects::{LoginMode, UserRole};
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::{AuthService, UserService};
use app_lib::domains::settings::commands::{
    settings_get_impl, settings_list_impl, settings_update_impl, SettingKeyArgs, SettingUpdateArgs,
};
use app_lib::domains::settings::domain::value_objects::SettingValue;
use app_lib::domains::settings::infrastructure::SqliteSettingRepo;
use app_lib::domains::settings::service::SettingsService;
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use app_lib::state::AppState;
use app_lib::sync::{SyncEngine, SyncEngineConfig, SyncEngineHandle};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::mock_app;
use tokio_util::sync::CancellationToken;

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

#[tokio::test]
async fn p3_mariam_the_superadmin_phase02_day() {
    // ----- Setup: fresh install state, no users in the DB -----
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
            initial_server_url: None,
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
        user_repo,
        "dev-A".into(),
        "0.1.0".into(),
        None,
    );

    // ----- Step 1: fresh-launch bootstrap -- Mariam creates herself as
    // the first superadmin via the bootstrap modal. -----
    let admin = users_create_first_admin_impl(
        &state,
        FirstAdminArgs {
            email: "mariam@idc.io".into(),
            name: "Mariam".into(),
            password: "mariam-pass-1!".into(),
            entity_id: Some("tenant-idc".into()),
        },
    )
    .await
    .expect("bootstrap must succeed on empty DB");
    assert_eq!(admin.role, UserRole::Superadmin);

    // Auto-login on bootstrap.
    let ctx = auth_current_user_impl(&state)
        .await
        .unwrap()
        .expect("auto-login should populate user context");
    assert_eq!(ctx.email, "mariam@idc.io");

    // ----- Step 2: Mariam navigates to /admin/users and reviews the
    // initial state -- only herself exists. -----
    let users = users_list_impl(
        &state,
        UsersListArgs {
            include_inactive: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].email, "mariam@idc.io");

    // ----- Step 3: Mariam adds two staff: Asma (accountant) and Mehdi
    // (receptionist). Per §7.6 emails normalize, audit rows fire. -----
    let asma = users_create_impl(
        &state,
        UserCreateArgs {
            email: "Asma@IDC.io".into(),
            name: "Asma".into(),
            role: UserRole::Accountant,
            password: "asma-pass-12345".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(asma.email, "asma@idc.io"); // lowercased
    assert_eq!(asma.role, UserRole::Accountant);

    let mehdi = users_create_impl(
        &state,
        UserCreateArgs {
            email: "mehdi@idc.io".into(),
            name: "Mehdi".into(),
            role: UserRole::Receptionist,
            password: "mehdi-pass-1234".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(mehdi.role, UserRole::Receptionist);

    // ----- Step 4: Mariam edits Mehdi's name (typo fix). -----
    let mehdi_renamed = users_update_impl(
        &state,
        UserUpdateArgs {
            id: mehdi.id.clone(),
            email: None,
            name: Some("Mehdi K.".into()),
            role: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(mehdi_renamed.name, "Mehdi K.");
    assert!(mehdi_renamed.version > mehdi.version);

    // ----- Step 5: Mariam confirms the users list shows 3 active users. -----
    let users = users_list_impl(
        &state,
        UsersListArgs {
            include_inactive: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(users.len(), 3);
    let json = serde_json::to_string(&users).unwrap();
    assert!(
        !json.contains("password_hash"),
        "Mariam's UI MUST never see password hashes: {json}"
    );

    // ----- Step 6: Mariam opens Asma's detail page (via users_get) -----
    let detail = users_get_impl(
        &state,
        UserIdArgs {
            id: asma.id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(detail.email, "asma@idc.io");

    // ----- Step 7: Mariam navigates to /admin/settings. The seeded
    // bundle shows 10 v1 keys for the "unscoped" tenant (per migration
    // 002). Mariam's tenant ("tenant-idc") starts empty -- her saves will
    // create per-tenant overrides. -----
    let _seeded_unscoped = settings_list_impl(&state).await.unwrap();
    // settings_list filters by the current user's entity_id == tenant-idc.
    // The seeds live under "unscoped"; this tenant sees zero.
    // But because the bootstrap path set ctx.entity_id = "tenant-idc",
    // the list returns rows scoped to that tenant.

    // ----- Step 8: Mariam changes the dye cost from default to 12000 IQD. -----
    let dye_updated = settings_update_impl(
        &state,
        SettingUpdateArgs {
            key: "dye_cost_iqd".into(),
            value: SettingValue::Int(12_000),
        },
    )
    .await
    .unwrap();
    assert_eq!(dye_updated.value, SettingValue::Int(12_000));
    assert_eq!(dye_updated.entity_id, "tenant-idc");

    // ----- Step 9: Mariam toggles Arabic numerals on. -----
    let an_updated = settings_update_impl(
        &state,
        SettingUpdateArgs {
            key: "arabic_numerals".into(),
            value: SettingValue::Bool(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(an_updated.value, SettingValue::Bool(true));

    // ----- Step 10: Mariam fetches a single setting (settings_get path). -----
    let dye_back = settings_get_impl(
        &state,
        SettingKeyArgs {
            key: "dye_cost_iqd".into(),
        },
    )
    .await
    .unwrap()
    .expect("dye_cost_iqd must exist after update");
    assert_eq!(dye_back.value, SettingValue::Int(12_000));

    // ----- Step 11: Mariam attempts an invalid thermal_width -- the
    // settings layer must reject it with a validation error. -----
    let err = settings_update_impl(
        &state,
        SettingUpdateArgs {
            key: "thermal_width".into(),
            value: SettingValue::Int(64),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, app_lib::error::AppError::Validation(_)),
        "thermal_width=64 must be rejected"
    );

    // ----- Step 12: Mariam locks her screen for lunch. -----
    auth_lock_impl(&state).await.unwrap();
    assert!(
        app_lib::domains::auth::commands::auth_is_locked_impl(&state)
            .await
            .unwrap()
    );
    // Session preserved across lock.
    assert!(auth_current_user_impl(&state).await.unwrap().is_some());

    // ----- Step 13: Mariam returns and unlocks with her password. -----
    auth_unlock_impl(
        &state,
        UnlockArgs {
            password: "mariam-pass-1!".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        !app_lib::domains::auth::commands::auth_is_locked_impl(&state)
            .await
            .unwrap()
    );

    // ----- Step 14: Mariam rotates Mehdi's password (after a security
    // incident). The reset writes an audit row; Mehdi can now log in
    // offline with the new password. -----
    users_reset_password_impl(
        &state,
        UserResetPasswordArgs {
            id: mehdi.id.clone(),
            new_password: "mehdi-rotated-1!".into(),
        },
    )
    .await
    .unwrap();

    // ----- Step 15: Mariam soft-deletes a phantom user (decommissions Mehdi). -----
    users_soft_delete_impl(
        &state,
        UserIdArgs {
            id: mehdi.id.clone(),
        },
    )
    .await
    .unwrap();
    let users_after = users_list_impl(
        &state,
        UsersListArgs {
            include_inactive: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(users_after.len(), 2); // Mariam + Asma only

    // ----- Step 16: Mariam logs out at end of day. -----
    auth_logout_impl(&state).await.unwrap();
    assert!(auth_current_user_impl(&state).await.unwrap().is_none());

    // ----- Step 17: Mariam re-logs the next morning. Offline login
    // succeeds because the local row's password_hash matches. -----
    let result = auth_login_impl(
        &state,
        LoginArgs {
            email: "mariam@idc.io".into(),
            password: "mariam-pass-1!".into(),
            entity_id_hint: Some("tenant-idc".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.mode, LoginMode::Offline);

    // ----- Step 18: Final audit-log sanity check -- the day should have
    // recorded at least: bootstrap (1), create Asma (1), create Mehdi (1),
    // update Mehdi (1), reset Mehdi (1), soft_delete Mehdi (1), plus the
    // 2 settings updates (dye_cost + arabic_numerals) = 8 rows minimum.
    let (audit_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        audit_count >= 8,
        "Mariam's day should have written >= 8 audit rows; got {audit_count}"
    );

    cancel.cancel();
    drop(mock);
}
