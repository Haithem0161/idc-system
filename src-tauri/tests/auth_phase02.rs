//! Phase-02 integration: AuthService (online/offline login fallback, bootstrap,
//! verify-on-unlock), against an in-memory SQLite with migrations applied.
//!
//! Network paths exercised through a wiremock server (online_login). Offline
//! paths bypass the network entirely.

use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::services::hash_password;
use app_lib::domains::auth::domain::value_objects::{LoginMode, UserRole};
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::AuthService;
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use wiremock::matchers::{header_exists, method, path};
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

fn make_auth(pool: &SqlitePool, device_id: &str) -> AuthService {
    AuthService::new(
        pool.clone(),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        device_id.to_string(),
    )
}

#[tokio::test]
async fn create_first_admin_succeeds_on_empty_db_and_writes_audit_plus_outbox() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");

    let user = auth
        .create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    assert_eq!(user.role, UserRole::Superadmin);
    assert_eq!(user.email, "root@idc.io");
    assert!(!user.password_hash.is_empty(), "password should be hashed");
    assert!(user.password_hash.starts_with("$argon2id$"));

    // Audit row recorded against this user.
    let (audit_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE entity = 'users'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit_count, 1);

    // Both audit + user outbox rows enqueued (per audit-first + side-effect-second contract).
    let (outbox_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(outbox_count, 2);
}

#[tokio::test]
async fn create_first_admin_returns_conflict_when_any_user_exists() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    auth.create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let err = auth
        .create_first_admin("second@idc.io", "Other", "12345678", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn create_first_admin_rejects_short_password_via_hash_password() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let err = auth
        .create_first_admin("root@idc.io", "Mariam", "short", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn create_first_admin_rejects_invalid_email() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let err = auth
        .create_first_admin("not-an-email", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn offline_login_succeeds_against_cached_user_when_no_server_url() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let user = auth
        .create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let result = auth
        .login(None, "root@idc.io", "12345678", &user.entity_id)
        .await
        .unwrap();
    assert_eq!(result.mode, LoginMode::Offline);
    assert_eq!(result.user_id, user.id);
    assert!(result.access_token.is_none());
    assert!(result.refresh_token.is_none());
}

#[tokio::test]
async fn offline_login_normalizes_mixed_case_email_to_lowercase() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let user = auth
        .create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let result = auth
        .login(None, "ROOT@IDC.IO", "12345678", &user.entity_id)
        .await
        .unwrap();
    assert_eq!(result.user_id, user.id);
}

#[tokio::test]
async fn offline_login_rejects_wrong_password_with_not_authenticated() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    auth.create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let err = auth
        .login(None, "root@idc.io", "WRONG-PASSWORD", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn offline_login_rejects_unknown_email_without_revealing_existence() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    auth.create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let err = auth
        .login(None, "ghost@idc.io", "12345678", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn offline_login_rejects_deactivated_user() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let u = auth
        .create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_active = 0 WHERE id = ?")
        .bind(u.id.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let err = auth
        .login(None, "root@idc.io", "12345678", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn verify_user_password_succeeds_with_correct_password_for_unlock() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let u = auth
        .create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();
    auth.verify_user_password(u.id, "12345678").await.unwrap();
}

#[tokio::test]
async fn verify_user_password_rejects_wrong_password_at_unlock() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let u = auth
        .create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();
    let err = auth
        .verify_user_password(u.id, "wrong-pw")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn verify_user_password_returns_not_authenticated_for_unknown_user_id() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    let unknown = uuid::Uuid::now_v7();
    let err = auth.verify_user_password(unknown, "x").await.unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn online_login_against_wiremock_succeeds_caches_local_row_and_returns_online_mode() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");

    let server = MockServer::start().await;
    let phc = hash_password("12345678").unwrap();
    let user_id = uuid::Uuid::now_v7();
    let body = serde_json::json!({
        "accessToken": "access.jwt.token",
        "refreshToken": "refresh.token",
        "expiresAt": chrono::Utc::now().to_rfc3339(),
        "user": {
            "id": user_id.to_string(),
            "email": "root@idc.io",
            "name": "Mariam",
            "role": "superadmin",
            "entityId": "tenant-1",
            "passwordHash": phc,
        }
    });
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .and(header_exists("X-Device-Id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let result = auth
        .login(Some(&server.uri()), "root@idc.io", "12345678", "tenant-1")
        .await
        .unwrap();

    assert_eq!(result.mode, LoginMode::Online);
    assert_eq!(result.role, UserRole::Superadmin);
    assert_eq!(result.access_token.as_deref(), Some("access.jwt.token"));
    assert_eq!(result.refresh_token.as_deref(), Some("refresh.token"));

    // Local cache populated so offline relaunch works.
    let user_repo = SqliteUserRepo::new(pool.clone());
    use app_lib::domains::auth::domain::repositories::UserRepo;
    let cached = user_repo
        .get_by_email("root@idc.io", "tenant-1")
        .await
        .unwrap()
        .expect("local row should exist post-online-login");
    assert_eq!(cached.id, user_id);
}

#[tokio::test]
async fn online_login_returning_401_short_circuits_to_not_authenticated_no_offline_fallback() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");

    // Seed an offline row whose password DOES match so we can prove no fallback happened.
    auth.create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = auth
        .login(Some(&server.uri()), "root@idc.io", "12345678", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}

#[tokio::test]
async fn online_login_5xx_falls_back_to_offline_when_local_cache_is_present() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    auth.create_first_admin("root@idc.io", "Mariam", "12345678", "tenant-1")
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let result = auth
        .login(Some(&server.uri()), "root@idc.io", "12345678", "tenant-1")
        .await
        .unwrap();
    assert_eq!(result.mode, LoginMode::Offline);
}

#[tokio::test]
async fn online_login_5xx_with_no_local_cache_returns_not_authenticated_via_offline_path() {
    let pool = fresh_pool().await;
    let auth = make_auth(&pool, "dev-A");
    // No local user seeded.

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let err = auth
        .login(Some(&server.uri()), "ghost@idc.io", "12345678", "tenant-1")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotAuthenticated));
}
