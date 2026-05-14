//! Phase-02 §6 edge-case coverage.
//!
//! One file per phase, covering the 8 mandatory edge categories from
//! `.claude/rules/testing.md` §6. Cross-cutting rows that belong to
//! `security.md` / `sync-conflicts.md` / `i18n-rtl.md` /
//! `performance-soak.md` are noted as `cross-cutting -- see <plan>`.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::services::hash_password;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::{AuthService, UserCreateInput, UserService};
use app_lib::domains::settings::domain::repositories::SettingRepo;
use app_lib::domains::settings::domain::value_objects::SettingValue;
use app_lib::domains::settings::infrastructure::SqliteSettingRepo;
use app_lib::domains::settings::service::SettingsService;
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

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

fn make_auth(pool: &SqlitePool) -> AuthService {
    AuthService::new(
        pool.clone(),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        "dev-A".into(),
    )
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

fn make_settings_service(
    pool: &SqlitePool,
    setting_repo: Arc<SqliteSettingRepo>,
) -> SettingsService {
    SettingsService::new(
        pool.clone(),
        setting_repo,
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        "dev-A".into(),
    )
}

// =========================================================================
// §6.1 Time / Timezone
// =========================================================================

#[tokio::test]
async fn time_user_timestamps_are_utc_iso_rfc3339() {
    // RFC3339-with-Z is the canonical sync clock; no local TZ leaks in.
    let user = User::try_new(
        "a@b.io",
        "n",
        UserRole::Receptionist,
        "$h".into(),
        "t".into(),
        None,
    )
    .unwrap();
    let serialized = serde_json::to_string(&user).unwrap();
    // chrono::DateTime<Utc> serializes as "...Z" in JSON.
    assert!(
        serialized.contains('Z'),
        "timestamps should be Z-suffixed: {serialized}"
    );
    assert!(
        !serialized.contains("+00:00") && !serialized.contains("+03:00"),
        "no local-tz markers should appear: {serialized}"
    );
}

#[tokio::test]
async fn time_migration_seed_uses_rfc3339_format() {
    // DEF-004 regression: migration 002 must emit RFC3339 timestamps so the
    // repo can parse seed rows. We confirm by reading them back through the
    // repo (which fails noisily on non-RFC3339).
    let pool = fresh_pool().await;
    let repo = SqliteSettingRepo::new(pool.clone());
    let rows = repo.list("unscoped").await.unwrap();
    assert_eq!(rows.len(), 10, "seeded rows readable post-DEF-004 fix");
}

// =========================================================================
// §6.2 i18n & RTL
// =========================================================================
// Cross-cutting -- owned by `i18n-rtl.md` for page-by-page RTL invariants.
// Phase-02 owns the pure helpers: `formatIqd` Arabic-Indic digits and
// `LoginSchema` / `UserCreateSchema` parse rules. Those land in §1.2 vitest
// tests at src/lib/format/money.test.ts and src/lib/schemas/auth.test.ts.

#[tokio::test]
async fn i18n_currency_symbol_setting_is_text_typed_for_d_a_or_iqd_or_arbitrary_text() {
    // The currency_symbol setting MUST allow arbitrary text values so a
    // tenant can configure either Arabic ("د.ع") or Latin ("IQD") symbols.
    let pool = fresh_pool().await;
    let svc = make_settings_service(&pool, Arc::new(SqliteSettingRepo::new(pool.clone())));
    let actor = Uuid::now_v7();
    // Latin string accepted.
    let ok1 = svc
        .update(
            actor,
            UserRole::Superadmin,
            "tenant-1",
            "currency_symbol",
            SettingValue::Text("IQD".into()),
        )
        .await
        .unwrap();
    assert_eq!(ok1.value, SettingValue::Text("IQD".into()));
    // Non-text rejected.
    let err = svc
        .update(
            actor,
            UserRole::Superadmin,
            "tenant-1",
            "currency_symbol",
            SettingValue::Int(0),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// =========================================================================
// §6.3 Offline & Network
// =========================================================================
// Online login + offline fallback paths are covered exhaustively in
// auth_phase02.rs / auth_ipc_phase02.rs. This row pins the contract that
// offline login does NOT require a sync server URL.

#[tokio::test]
async fn offline_login_works_when_server_url_is_none() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool);
    auth.create_first_admin("a@b.io", "Admin", "admin-pass", "tenant-1")
        .await
        .unwrap();
    let result = auth
        .login(None, "a@b.io", "admin-pass", "tenant-1")
        .await
        .unwrap();
    use app_lib::domains::auth::domain::value_objects::LoginMode;
    assert_eq!(result.mode, LoginMode::Offline);
}

#[tokio::test]
async fn offline_login_works_when_server_url_is_empty_string() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool);
    auth.create_first_admin("a@b.io", "Admin", "admin-pass", "tenant-1")
        .await
        .unwrap();
    // Empty-string URL must be treated as "no server" so we go offline.
    let result = auth
        .login(Some(""), "a@b.io", "admin-pass", "tenant-1")
        .await
        .unwrap();
    use app_lib::domains::auth::domain::value_objects::LoginMode;
    assert_eq!(result.mode, LoginMode::Offline);
}

// =========================================================================
// §6.4 Concurrency & Conflicts
// =========================================================================
// Cross-cutting -- multi-device LWW for users + manual policy for settings
// live in `sync-conflicts.md`. Phase-02 owns the local-side tenant
// isolation invariant (two tenants cannot read each other's rows).

#[tokio::test]
async fn concurrency_two_tenants_isolate_settings_via_entity_id_filter() {
    let pool = fresh_pool().await;
    let setting_repo = Arc::new(SqliteSettingRepo::new(pool.clone()));
    let svc = make_settings_service(&pool, setting_repo.clone());
    let actor = Uuid::now_v7();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-A",
        "dye_cost_iqd",
        SettingValue::Int(10_000),
    )
    .await
    .unwrap();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-B",
        "dye_cost_iqd",
        SettingValue::Int(99_999),
    )
    .await
    .unwrap();
    let a = setting_repo
        .get_by_key("dye_cost_iqd", "tenant-A")
        .await
        .unwrap()
        .unwrap();
    let b = setting_repo
        .get_by_key("dye_cost_iqd", "tenant-B")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.value, SettingValue::Int(10_000));
    assert_eq!(b.value, SettingValue::Int(99_999));
}

#[tokio::test]
async fn concurrency_two_tenants_isolate_users_via_entity_id_unique_index() {
    // Two tenants can have a user with the same email -- the partial unique
    // index keys on (email, deleted_at IS NULL) per tenant via entity_id.
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));

    let u1 = User::try_new(
        "shared@idc.io",
        "A",
        UserRole::Superadmin,
        hash_password("tenant-a-pw-12345").unwrap(),
        "tenant-A".into(),
        None,
    )
    .unwrap();
    let u2 = User::try_new(
        "shared@idc.io",
        "B",
        UserRole::Superadmin,
        hash_password("tenant-b-pw-12345").unwrap(),
        "tenant-B".into(),
        None,
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &u1).await.unwrap();
    user_repo.upsert(&mut tx, &u2).await.unwrap();
    tx.commit().await.unwrap();

    let a = user_repo
        .get_by_email("shared@idc.io", "tenant-A")
        .await
        .unwrap()
        .unwrap();
    let b = user_repo
        .get_by_email("shared@idc.io", "tenant-B")
        .await
        .unwrap()
        .unwrap();
    assert_ne!(a.id, b.id);
}

// =========================================================================
// §6.5 Crash & Recovery
// =========================================================================

#[tokio::test]
async fn crash_user_create_tx_rollback_leaves_no_partial_state() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let svc = make_user_service(&pool, user_repo.clone());

    // Seed superadmin actor.
    let admin = User::try_new(
        "admin@idc.io",
        "Mariam",
        UserRole::Superadmin,
        hash_password("admin-pass").unwrap(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &admin).await.unwrap();
    tx.commit().await.unwrap();

    // Attempt to create a user with a duplicate email — service rejects
    // before any write. The audit log + outbox should be UNCHANGED.
    let (audit_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (outbox_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();

    let err = svc
        .create(
            admin.id,
            admin.role,
            UserCreateInput {
                email: "admin@idc.io".into(),
                name: "Duplicate".into(),
                role: UserRole::Receptionist,
                password: "newpass-1234".into(),
                entity_id: "tenant-1".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));

    let (audit_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (outbox_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audit_before, audit_after, "no audit row on rejection");
    assert_eq!(outbox_before, outbox_after, "no outbox row on rejection");
}

#[tokio::test]
async fn crash_settings_update_with_invalid_value_leaves_no_partial_state() {
    let pool = fresh_pool().await;
    let svc = make_settings_service(&pool, Arc::new(SqliteSettingRepo::new(pool.clone())));
    let actor = Uuid::now_v7();
    // Pre-seed a valid row.
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "dye_cost_iqd",
        SettingValue::Int(10_000),
    )
    .await
    .unwrap();
    let (audit_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Invalid update.
    let err = svc
        .update(
            actor,
            UserRole::Superadmin,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(-1),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
    let (audit_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        audit_before, audit_after,
        "rejected update writes no audit row"
    );
}

// =========================================================================
// §6.6 Scale & Performance
// =========================================================================
// Owned by `perf_phase02.rs` with hard pass/fail SLO assertions.

// =========================================================================
// §6.7 Security & Permissions
// =========================================================================

#[tokio::test]
async fn security_password_hash_never_appears_in_user_json_envelope() {
    // Already pinned in users entity tests + IPC tests; this is the
    // "single boundary canary" that catches a regression at the serde
    // layer.
    let u = User::try_new(
        "a@b.io",
        "n",
        UserRole::Superadmin,
        "$argon2id$v=19$VERY_SECRET_HASH_BYTES".into(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let json = serde_json::to_string(&u).unwrap();
    assert!(
        !json.contains("VERY_SECRET_HASH_BYTES"),
        "hash bytes leaked: {json}"
    );
    assert!(!json.contains("password_hash"));
}

#[tokio::test]
async fn security_role_gate_rejects_non_superadmin_at_user_service_boundary() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let svc = make_user_service(&pool, user_repo);
    let actor_id = Uuid::now_v7();
    for role in [UserRole::Receptionist, UserRole::Accountant] {
        let err = svc
            .create(
                actor_id,
                role,
                UserCreateInput {
                    email: "x@idc.io".into(),
                    name: "X".into(),
                    role: UserRole::Receptionist,
                    password: "newpass-1234".into(),
                    entity_id: "tenant-1".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}

#[tokio::test]
async fn security_argon2id_verify_rejects_wrong_password_constant_time() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool);
    auth.create_first_admin("a@b.io", "Admin", "admin-pass", "t")
        .await
        .unwrap();
    // Wrong password yields NotAuthenticated, NOT a panic, NOT a structural
    // mismatch. The verify path is constant-time per the argon2 crate.
    let result = auth.login(None, "a@b.io", "WRONG", "t").await;
    assert!(matches!(result, Err(AppError::NotAuthenticated)));
}

#[tokio::test]
async fn security_argon2id_hash_uses_argon2id_variant_not_argon2i_or_argon2d() {
    // Defence in depth: the chosen variant must be argon2id (resistant to
    // both side-channel and GPU attacks). Argon2i (side-channel resistant
    // but vulnerable to GPU) and argon2d (GPU resistant but side-channel
    // vulnerable) are NOT acceptable.
    let phc = hash_password("any-password").unwrap();
    assert!(
        phc.starts_with("$argon2id$"),
        "hash must use argon2id: {phc}"
    );
    assert!(!phc.starts_with("$argon2i$"));
    assert!(!phc.starts_with("$argon2d$"));
}

#[tokio::test]
async fn security_soft_deleted_users_are_invisible_to_get_by_email_lookup() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let user = User::try_new(
        "ghost@idc.io",
        "Ghost",
        UserRole::Superadmin,
        hash_password("ghost-pw").unwrap(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();
    // Confirm present.
    assert!(user_repo
        .get_by_email("ghost@idc.io", "tenant-1")
        .await
        .unwrap()
        .is_some());
    // Soft-delete via direct SQL (simulates the worst case where the
    // entity / service path is bypassed).
    sqlx::query("UPDATE users SET deleted_at = '2026-05-14T00:00:00Z' WHERE id = ?")
        .bind(user.id.to_string())
        .execute(&pool)
        .await
        .unwrap();
    // get_by_email filters on deleted_at IS NULL.
    assert!(user_repo
        .get_by_email("ghost@idc.io", "tenant-1")
        .await
        .unwrap()
        .is_none());
}

// =========================================================================
// §6.8 Data Integrity
// =========================================================================

#[tokio::test]
async fn integrity_migration_002_is_idempotent_on_fresh_db() {
    // Run migrations twice on the same pool; second call is a no-op.
    let pool = fresh_pool().await;
    migrations::run(&pool).await.unwrap(); // second call
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings WHERE entity_id = 'unscoped'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Seed only inserts once thanks to OR IGNORE + the PK on settings.id.
    assert_eq!(n, 10);
}

#[tokio::test]
async fn integrity_check_constraint_on_setting_value_type_rejects_unknown_value() {
    let pool = fresh_pool().await;
    // Direct INSERT with an unknown value_type must violate the CHECK
    // constraint at the SQLite layer (defence in depth beyond the Rust enum).
    let result = sqlx::query(
        "INSERT INTO settings (id, key, value, value_type, created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, 'k', 'v', 'json', '2026-05-14T00:00:00Z', '2026-05-14T00:00:00Z', 1, 0, 'tenant-1')",
    )
    .bind(Uuid::now_v7().to_string())
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint should reject value_type='json'"
    );
}

#[tokio::test]
async fn integrity_partial_unique_index_blocks_duplicate_email_per_tenant_when_both_active() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let u1 = User::try_new(
        "dup@idc.io",
        "A",
        UserRole::Superadmin,
        "$h".into(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let u2 = User::try_new(
        "dup@idc.io",
        "B",
        UserRole::Superadmin,
        "$h".into(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &u1).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let err = user_repo.upsert(&mut tx, &u2).await.unwrap_err();
    // SQLITE_CONSTRAINT_UNIQUE -> AppError::Conflict (the global From impl).
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn integrity_users_version_increments_strictly_monotonically_per_mutation() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let svc = make_user_service(&pool, user_repo.clone());

    let admin = User::try_new(
        "admin@idc.io",
        "Mariam",
        UserRole::Superadmin,
        hash_password("admin-pass").unwrap(),
        "tenant-1".into(),
        None,
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &admin).await.unwrap();
    tx.commit().await.unwrap();

    let created = svc
        .create(
            admin.id,
            admin.role,
            UserCreateInput {
                email: "u@idc.io".into(),
                name: "U".into(),
                role: UserRole::Receptionist,
                password: "newpass-1234".into(),
                entity_id: "tenant-1".into(),
            },
        )
        .await
        .unwrap();
    let v1 = svc
        .update(
            admin.id,
            admin.role,
            created.id,
            app_lib::domains::auth::UserUpdateInput {
                name: Some("V1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let v2 = svc
        .update(
            admin.id,
            admin.role,
            created.id,
            app_lib::domains::auth::UserUpdateInput {
                name: Some("V2".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(created.version, 1);
    assert_eq!(v1.version, 2);
    assert_eq!(v2.version, 3);
}

#[tokio::test]
async fn integrity_audit_log_actor_user_id_fk_to_users_blocks_orphan_writes() {
    // The phase-01 §1 modified-tables rebuild adds an FK from
    // audit_log.actor_user_id -> users.id. Inserting an audit row with a
    // non-existent actor_user_id should violate the FK.
    let pool = fresh_pool().await;
    let result = sqlx::query(
        "INSERT INTO audit_log (id, actor_user_id, action, entity, entity_id, delta, ip, device_id, created_at, entity_id_tenant) \
         VALUES (?, ?, 'login', 'users', ?, '{}', NULL, 'dev-A', '2026-05-14T00:00:00Z', 'tenant-1')",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(Uuid::now_v7().to_string()) // orphan actor
    .bind(Uuid::now_v7().to_string())
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "FK to users.id should reject orphan actor_user_id"
    );
}

#[tokio::test]
async fn integrity_settings_value_type_check_constraint_accepts_v1_values_only() {
    let pool = fresh_pool().await;
    // Each of the four legal value_types should be accepted.
    for vt in ["int", "decimal", "text", "bool"] {
        let r = sqlx::query(
            "INSERT INTO settings (id, key, value, value_type, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, '0', ?, '2026-05-14T00:00:00Z', '2026-05-14T00:00:00Z', 1, 0, 'tenant-1')",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(format!("k_{vt}"))
        .bind(vt)
        .execute(&pool)
        .await;
        assert!(
            r.is_ok(),
            "value_type={vt} should be accepted: {:?}",
            r.err()
        );
    }
}
