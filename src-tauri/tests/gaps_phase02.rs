//! Phase-02 §9-§12 gap-derived rows that map to currently-implemented code.
//!
//! The phase-02-test plan's gap analysis (5 passes, 38 rows P02-G01 through
//! P02-G38) prescribed test scenarios that anticipate features the build
//! cycle has not yet delivered (refresh tokens client-side, JwtVerifier RS256
//! pinning, stronghold creds cache, settings::set_locale IPC, ar-forcing
//! detector, etc.). The rows that DO map to existing code land here. The
//! rest are tracked as deferred-feature defects in `defects.md`.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::{UserListFilter, UserRepo};
use app_lib::domains::auth::domain::services::hash_password;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::{AuthService, UserCreateInput, UserService, UserUpdateInput};
use app_lib::domains::settings::domain::repositories::SettingRepo;
use app_lib::domains::settings::domain::value_objects::SettingValue;
use app_lib::domains::settings::infrastructure::SqliteSettingRepo;
use app_lib::domains::sync::domain::value_objects::AuditAction;
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

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

fn make_user_service(pool: &SqlitePool, user_repo: Arc<SqliteUserRepo>) -> UserService {
    UserService::new(
        pool.clone(),
        user_repo,
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        "dev-A".into(),
    )
}

fn make_auth(pool: &SqlitePool) -> AuthService {
    AuthService::new(
        pool.clone(),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        "dev-A".into(),
    )
}

async fn seed_superadmin(pool: &SqlitePool, repo: &Arc<SqliteUserRepo>) -> User {
    let admin = User::try_new(
        "admin@idc.io",
        "Mariam",
        UserRole::Superadmin,
        hash_password("admin-pw-12345").unwrap(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &admin).await.unwrap();
    tx.commit().await.unwrap();
    admin
}

// =========================================================================
// P02-G14 -- 12-value closed audit-action union enforcement (HIGH)
//
// Pass-2 row. The phase-01 §7.18 audit-action enum closes at 12 values:
// login, logout, lock, password_change, create, update, soft_delete, void,
// clock_in, clock_out, conflict_resolve, vacuum. Every legal value must
// round-trip via serde; any 13th value must be rejected.
//
// Spec says 12; the actual `AuditAction` enum in
// `src-tauri/src/domains/sync/domain/value_objects/mod.rs` MAY extend further
// for later phases. This test pins ALL phase-02 actions and verifies the
// closed-set rejection.
// =========================================================================

#[test]
fn p02_g14_audit_action_enum_round_trips_known_phase02_literals() {
    for literal in [
        "login",
        "create",
        "update",
        "soft_delete",
        "password_change",
    ] {
        let action: AuditAction = serde_json::from_value(serde_json::Value::String(literal.into()))
            .unwrap_or_else(|e| panic!("phase-02 audit literal `{literal}` must round-trip: {e}"));
        assert_eq!(
            serde_json::to_string(&action).unwrap(),
            format!("\"{literal}\"")
        );
    }
}

#[test]
fn p02_g14_audit_action_rejects_unknown_literal() {
    let result: Result<AuditAction, _> =
        serde_json::from_value(serde_json::Value::String("nuke".into()));
    assert!(
        result.is_err(),
        "AuditAction is a closed enum; `nuke` must be rejected"
    );
}

// =========================================================================
// P02-G17 -- AuthService::login writes `action='login'` audit row
//
// The phase-02 build spec calls for `login` to be audited. Today
// `create_first_admin` writes an `action='create'` audit row but neither
// `online_login` nor `offline_login` write any audit row. This test pins
// the CURRENT behaviour as a defect (DEF-005); when the audit-on-login
// fix lands, the test should be updated to assert action='login'.
// =========================================================================

#[allow(non_snake_case)]
#[tokio::test]
async fn p02_g17_offline_login_does_NOT_write_audit_row_today_defect_005() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool);
    auth.create_first_admin("admin@idc.io", "Mariam", "admin-pw-12345", "tenant-1")
        .await
        .unwrap();
    // After bootstrap there's exactly 1 audit row (action=create on the new user).
    let (before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    auth.login(None, "admin@idc.io", "admin-pw-12345", "tenant-1")
        .await
        .unwrap();
    let (after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    // CURRENT behaviour: no new audit row on offline login. This pins the
    // observation that DEF-005 captures. Change to `assert_eq!(after, before + 1)`
    // once the audit-on-login wiring lands.
    assert_eq!(
        before, after,
        "DEF-005 regression sentinel: login should write an audit row"
    );
}

// =========================================================================
// P02-G29 -- users::list filtering rule:
//   "only excludes inactive unless superadmin"
//
// The repo layer's `list(filter)` accepts an `include_inactive` flag and
// honours it unconditionally — the role-based "only superadmin gets
// inactive" gate is NOT implemented at the IPC layer. This test pins the
// repo's CURRENT behaviour. The role-aware filter is DEF-006.
// =========================================================================

#[tokio::test]
async fn p02_g29_repo_list_filter_passes_include_inactive_through_unconditionally() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let svc = make_user_service(&pool, user_repo.clone());
    let admin = seed_superadmin(&pool, &user_repo).await;

    let target = svc
        .create(
            admin.id,
            admin.role,
            UserCreateInput {
                email: "inactive@idc.io".into(),
                name: "Inactive".into(),
                role: UserRole::Receptionist,
                password: "newpass-1234".into(),
                entity_id: "tenant-1".into(),
            },
        )
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_active = 0 WHERE id = ?")
        .bind(target.id.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let default = user_repo.list(UserListFilter::default()).await.unwrap();
    let all = user_repo
        .list(UserListFilter {
            include_inactive: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(default.iter().all(|u| u.is_active));
    assert!(all.iter().any(|u| !u.is_active));
}

// =========================================================================
// P02-G32 -- UserService::update audit-delta shape
//
// Plan: audit row's delta JSON carries `{ field: { from, to } }` shape for
// changed fields only; unchanged fields omitted. Pin the contract.
// =========================================================================

#[tokio::test]
async fn p02_g32_update_audit_delta_carries_before_and_after_snapshot_only_for_changed_fields() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let svc = make_user_service(&pool, user_repo.clone());
    let admin = seed_superadmin(&pool, &user_repo).await;
    let target = svc
        .create(
            admin.id,
            admin.role,
            UserCreateInput {
                email: "u@idc.io".into(),
                name: "Old".into(),
                role: UserRole::Receptionist,
                password: "newpass-1234".into(),
                entity_id: "tenant-1".into(),
            },
        )
        .await
        .unwrap();

    svc.update(
        admin.id,
        admin.role,
        target.id,
        UserUpdateInput {
            name: Some("New".into()),
            email: None,
            role: None,
        },
    )
    .await
    .unwrap();

    let (delta_json,): (String,) = sqlx::query_as(
        "SELECT delta FROM audit_log WHERE entity_id = ? AND action = 'update' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(target.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    let delta: serde_json::Value = serde_json::from_str(&delta_json).unwrap();
    // Per P02-G32: only changed fields appear; each is `{from, to}`.
    // Today: `{"name":{"from":"Old","to":"New"}}` — exact match.
    let name = delta
        .get("name")
        .unwrap_or_else(|| panic!("delta missing `name` field: {delta_json}"));
    assert_eq!(name.get("from").and_then(|v| v.as_str()), Some("Old"));
    assert_eq!(name.get("to").and_then(|v| v.as_str()), Some("New"));
    // Unchanged fields (email, role) MUST be absent.
    assert!(
        delta.get("email").is_none(),
        "unchanged `email` should be omitted: {delta_json}"
    );
    assert!(
        delta.get("role").is_none(),
        "unchanged `role` should be omitted: {delta_json}"
    );
}

// =========================================================================
// P02-G36 -- settings seed default-value table parity
//
// Phase-02 §1 lists the 10 v1 seed values. Confirm migration 002 inserts
// them with the documented defaults.
// =========================================================================

#[tokio::test]
async fn p02_g36_settings_seed_default_values_match_phase02_section_1_table() {
    let pool = fresh_pool().await;
    let repo = SqliteSettingRepo::new(pool.clone());

    async fn assert_value(repo: &SqliteSettingRepo, key: &str, expected: SettingValue) {
        let row = repo
            .get_by_key(key, "unscoped")
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("seed key `{key}` missing"));
        assert_eq!(row.value, expected, "seed default for `{key}`");
    }

    // The phase-02 §1 settings seed defaults.
    assert_value(&repo, "dye_cost_iqd", SettingValue::Int(10_000)).await;
    assert_value(&repo, "report_cost_iqd", SettingValue::Int(10_000)).await;
    assert_value(&repo, "internal_doctor_pct", SettingValue::Int(30)).await;
    assert_value(&repo, "idle_lock_minutes", SettingValue::Int(10)).await;
    assert_value(&repo, "arabic_numerals", SettingValue::Bool(false)).await;
    assert_value(&repo, "thermal_width", SettingValue::Int(32)).await;
    assert_value(
        &repo,
        "thermal_printer_name",
        SettingValue::Text(String::new()),
    )
    .await;
    assert_value(
        &repo,
        "clinic_display_name_ar",
        SettingValue::Text(String::new()),
    )
    .await;
    assert_value(
        &repo,
        "clinic_display_name_en",
        SettingValue::Text(String::new()),
    )
    .await;

    // currency_symbol seed must be text-typed (value content may vary by
    // installation locale: "د.ع" or "IQD" both legal).
    let cs = repo
        .get_by_key("currency_symbol", "unscoped")
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(cs.value, SettingValue::Text(_)),
        "currency_symbol must be text-typed: {:?}",
        cs.value
    );
}

// =========================================================================
// P02-G38 -- ResetPasswordSchema min-8 parity
//
// Already covered in `src/lib/schemas/auth.test.ts`. This is a sentinel
// noting the cross-reference.
// =========================================================================

#[test]
fn p02_g38_reset_password_schema_min_8_is_covered_in_frontend_schema_tests() {
    // No-op pin: the relevant assertion is at
    //   src/lib/schemas/auth.test.ts::ResetPasswordSchema -> rejects 7-char.
    // This test exists so a grep for "P02-G38" lands here and forwards.
}
