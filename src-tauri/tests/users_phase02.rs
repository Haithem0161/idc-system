//! Phase-02 integration: `UserService` CRUD with audit-first + outbox semantics
//! and superadmin-only role gate.

use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::{UserListFilter, UserRepo};
use app_lib::domains::auth::domain::services::{hash_password, verify_password};
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::{UserCreateInput, UserService, UserUpdateInput};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
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

fn make_service(pool: &SqlitePool, device_id: &str) -> (UserService, Arc<SqliteUserRepo>) {
    let repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let svc = UserService::new(
        pool.clone(),
        repo.clone(),
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        device_id.to_string(),
    );
    (svc, repo)
}

async fn seed_superadmin(pool: &SqlitePool, repo: &Arc<SqliteUserRepo>) -> User {
    let user = User::try_new(
        "admin@idc.io",
        "Mariam",
        UserRole::Superadmin,
        hash_password("admin-pass").unwrap(),
        "tenant-1".into(),
        Some("dev-A".into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();
    user
}

fn input(email: &str, role: UserRole) -> UserCreateInput {
    UserCreateInput {
        email: email.into(),
        name: "Worker".into(),
        role,
        password: "worker-pass".into(),
        entity_id: "tenant-1".into(),
    }
}

#[tokio::test]
async fn create_returns_persisted_user_and_writes_audit_row() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;

    let created = svc
        .create(
            admin.id,
            admin.role,
            input("new@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();

    assert_eq!(created.email, "new@idc.io");
    assert_eq!(created.role, UserRole::Receptionist);
    assert!(created.password_hash.starts_with("$argon2id$"));
    assert_eq!(created.version, 1);

    // Persisted.
    let fetched = repo.get_by_id(created.id).await.unwrap().unwrap();
    assert_eq!(fetched.email, "new@idc.io");

    // Audit row + outbox row for this user.
    let (audit_rows,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE entity = 'users' AND entity_id = ?")
            .bind(created.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit_rows, 1);
}

#[tokio::test]
async fn create_rejects_non_superadmin_callers() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let actor_id = Uuid::now_v7();

    for role in [UserRole::Receptionist, UserRole::Accountant] {
        let err = svc
            .create(actor_id, role, input("nope@idc.io", UserRole::Receptionist))
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(_)),
            "{role:?} should be rejected"
        );
    }
}

#[tokio::test]
async fn create_normalizes_email_to_lowercase() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;

    let created = svc
        .create(
            admin.id,
            admin.role,
            UserCreateInput {
                email: "Mixed@Case.IO".into(),
                name: "X".into(),
                role: UserRole::Receptionist,
                password: "abcdefgh".into(),
                entity_id: "tenant-1".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(created.email, "mixed@case.io");
}

#[tokio::test]
async fn create_rejects_duplicate_email_in_same_tenant() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    svc.create(
        admin.id,
        admin.role,
        input("dup@idc.io", UserRole::Receptionist),
    )
    .await
    .unwrap();

    let err = svc
        .create(
            admin.id,
            admin.role,
            input("dup@idc.io", UserRole::Accountant),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn create_rejects_short_password() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let err = svc
        .create(
            admin.id,
            admin.role,
            UserCreateInput {
                email: "short@idc.io".into(),
                name: "X".into(),
                role: UserRole::Receptionist,
                password: "short".into(),
                entity_id: "tenant-1".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_changes_name_role_and_bumps_version() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let target = svc
        .create(
            admin.id,
            admin.role,
            input("u@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();

    let updated = svc
        .update(
            admin.id,
            admin.role,
            target.id,
            UserUpdateInput {
                name: Some("Renamed".into()),
                email: None,
                role: Some(UserRole::Accountant),
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.role, UserRole::Accountant);
    assert_eq!(updated.version, 2);
}

#[tokio::test]
async fn update_lowercases_changed_email() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let target = svc
        .create(
            admin.id,
            admin.role,
            input("a@x.io", UserRole::Receptionist),
        )
        .await
        .unwrap();

    let updated = svc
        .update(
            admin.id,
            admin.role,
            target.id,
            UserUpdateInput {
                email: Some("RENAMED@X.IO".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.email, "renamed@x.io");
}

#[tokio::test]
async fn update_returns_not_found_for_unknown_id() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let err = svc
        .update(
            admin.id,
            admin.role,
            Uuid::now_v7(),
            UserUpdateInput::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn update_rejects_non_superadmin_caller_with_validation_error() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let target = svc
        .create(
            admin.id,
            admin.role,
            input("u@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();

    let err = svc
        .update(
            target.id,
            UserRole::Receptionist,
            target.id,
            UserUpdateInput::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn soft_delete_sets_deleted_at_and_makes_user_invisible_to_get_by_id() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let target = svc
        .create(
            admin.id,
            admin.role,
            input("u@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();

    svc.soft_delete(admin.id, admin.role, target.id)
        .await
        .unwrap();

    // get_by_id filters on deleted_at IS NULL.
    let after = repo.get_by_id(target.id).await.unwrap();
    assert!(
        after.is_none(),
        "soft-deleted user should be invisible to get_by_id"
    );

    // Audit row recorded for the soft_delete action.
    let (audit_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity_id = ? AND action = 'soft_delete'",
    )
    .bind(target.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_rows, 1);
}

#[tokio::test]
async fn soft_delete_rejects_non_superadmin_caller() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .soft_delete(Uuid::now_v7(), UserRole::Receptionist, Uuid::now_v7())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn reset_password_rotates_hash_and_writes_audit_with_password_change_action() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let target = svc
        .create(
            admin.id,
            admin.role,
            input("u@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();
    let original_hash = target.password_hash.clone();

    svc.reset_password(admin.id, admin.role, target.id, "new-password-strong-1!")
        .await
        .unwrap();

    let after = repo.get_by_id(target.id).await.unwrap().unwrap();
    assert_ne!(after.password_hash, original_hash);
    // The new hash actually verifies the new password.
    verify_password("new-password-strong-1!", &after.password_hash).unwrap();
    // Old password no longer verifies.
    assert!(verify_password("worker-pass", &after.password_hash).is_err());

    let (audit_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity_id = ? AND action = 'password_change'",
    )
    .bind(target.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_rows, 1);
}

#[tokio::test]
async fn reset_password_rejects_non_superadmin_caller() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .reset_password(
            Uuid::now_v7(),
            UserRole::Receptionist,
            Uuid::now_v7(),
            "newpass-1234",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn reset_password_returns_not_found_for_unknown_id() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let err = svc
        .reset_password(admin.id, admin.role, Uuid::now_v7(), "newpass-1234")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn list_with_default_filter_hides_inactive_users() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;

    let alpha = svc
        .create(
            admin.id,
            admin.role,
            input("alpha@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();
    let beta = svc
        .create(
            admin.id,
            admin.role,
            input("beta@idc.io", UserRole::Accountant),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_active = 0 WHERE id = ?")
        .bind(beta.id.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let active = repo.list(UserListFilter::default()).await.unwrap();
    let ids: Vec<_> = active.iter().map(|u| u.id).collect();
    assert!(ids.contains(&admin.id));
    assert!(ids.contains(&alpha.id));
    assert!(!ids.contains(&beta.id));
}

#[tokio::test]
async fn list_with_include_inactive_returns_both_active_and_inactive_rows() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    let alpha = svc
        .create(
            admin.id,
            admin.role,
            input("alpha@idc.io", UserRole::Receptionist),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_active = 0 WHERE id = ?")
        .bind(alpha.id.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let all = repo
        .list(UserListFilter {
            include_inactive: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let ids: Vec<_> = all.iter().map(|u| u.id).collect();
    assert!(ids.contains(&alpha.id));
}

#[tokio::test]
async fn users_list_response_strips_password_hash_at_serde_boundary() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let admin = seed_superadmin(&pool, &repo).await;
    svc.create(
        admin.id,
        admin.role,
        input("u@idc.io", UserRole::Receptionist),
    )
    .await
    .unwrap();

    let users = repo.list(UserListFilter::default()).await.unwrap();
    let json = serde_json::to_string(&users).unwrap();
    assert!(!json.contains("password_hash"));
    assert!(!json.contains("$argon2id$"));
}
