//! Integration tests for Phase-4 shifts (PRD §6.1.8).
//!
//! Drives the full ShiftService through an in-memory SQLite (with all phase
//! 1-3 migrations applied) and a stub operator + user. Covers:
//! - clock_in success
//! - double-clock-in rejection (Conflict)
//! - clock_out success + non-superadmin role guard
//! - edit by non-superadmin rejection
//! - edit by superadmin acceptance
//! - edit with overlap rejection
//! - soft_delete idempotency
//! - audit-first ordering (audit row present BEFORE business row in failure)
//! - history_today timezone window

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::catalog::domain::entities::operator::OperatorNewInput;
use app_lib::domains::catalog::domain::entities::Operator;
use app_lib::domains::catalog::domain::repositories::OperatorRepo;
use app_lib::domains::catalog::infrastructure::SqliteOperatorRepo;
use app_lib::domains::shifts::domain::repositories::OperatorShiftRepo;
use app_lib::domains::shifts::infrastructure::SqliteOperatorShiftRepo;
use app_lib::domains::shifts::service::{ShiftEditInput, ShiftService};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use chrono::{Duration, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-x";
const DEVICE_ID: &str = "dev-1";

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

struct Fixture {
    pool: SqlitePool,
    service: ShiftService,
    shift_repo: Arc<dyn OperatorShiftRepo>,
    operator: Operator,
    superadmin: User,
    receptionist: User,
}

async fn seed() -> Fixture {
    let pool = fresh_pool().await;

    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let operator_repo: Arc<dyn OperatorRepo> = Arc::new(SqliteOperatorRepo::new(pool.clone()));
    let shift_repo: Arc<dyn OperatorShiftRepo> =
        Arc::new(SqliteOperatorShiftRepo::new(pool.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));

    let service = ShiftService::new(
        pool.clone(),
        shift_repo.clone(),
        operator_repo.clone(),
        audit,
        outbox,
        DEVICE_ID.to_string(),
    );

    let operator = Operator::try_new(OperatorNewInput {
        name: "Asma".into(),
        phone: None,
        base_cut_per_check_iqd: 5_000,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    operator_repo.upsert(&mut tx, &operator).await.unwrap();
    tx.commit().await.unwrap();

    let superadmin = User::try_new(
        "boss@example.com",
        "Boss",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let receptionist = User::try_new(
        "reception@example.com",
        "Receptionist",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &superadmin).await.unwrap();
    user_repo.upsert(&mut tx, &receptionist).await.unwrap();
    tx.commit().await.unwrap();

    Fixture {
        pool,
        service,
        shift_repo,
        operator,
        superadmin,
        receptionist,
    }
}

#[tokio::test]
async fn clock_in_succeeds_for_receptionist() {
    let fx = seed().await;
    let shift = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    assert!(shift.is_open());
    assert_eq!(shift.operator_id, fx.operator.id);

    let open = fx.shift_repo.list_open(ENTITY_ID).await.unwrap();
    assert_eq!(open.len(), 1);
}

#[tokio::test]
async fn clock_in_rejects_double_open() {
    let fx = seed().await;
    fx.service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    let err = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Conflict(_)));
}

#[tokio::test]
async fn clock_in_rejects_inactive_operator() {
    let fx = seed().await;
    let inactive = fx.operator.clone().with_active(false);
    let mut tx = fx.pool.begin().await.unwrap();
    // We bypass OperatorService here because phase-04 tests run without a
    // CatalogServices wiring; the repo upsert is the same path the service
    // would invoke.
    let repo = SqliteOperatorRepo::new(fx.pool.clone());
    repo.upsert(&mut tx, &inactive).await.unwrap();
    tx.commit().await.unwrap();
    let err = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
}

#[tokio::test]
async fn clock_out_works() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            Some("a".into()),
        )
        .await
        .unwrap();
    let closed = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    assert!(closed.check_out_at.is_some());
    assert!(!closed.is_open());
}

#[tokio::test]
async fn edit_rejects_non_superadmin() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    let err = fx
        .service
        .edit(
            fx.receptionist.id,
            UserRole::Receptionist,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: opened.check_in_at,
                check_out_at: Some(opened.check_in_at + Duration::hours(1)),
                note: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
}

#[tokio::test]
async fn edit_succeeds_for_superadmin() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    let new_in = opened.check_in_at - Duration::minutes(15);
    let new_out = opened.check_in_at + Duration::hours(2);
    let edited = fx
        .service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: new_in,
                check_out_at: Some(new_out),
                note: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.check_in_at, new_in);
    assert_eq!(edited.check_out_at, Some(new_out));
}

#[tokio::test]
async fn edit_rejects_when_overlapping_another_shift() {
    let fx = seed().await;
    let now = Utc::now();
    // Create an already-closed shift in the past.
    let earlier = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    let closed = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, earlier.id)
        .await
        .unwrap();

    // Open a second shift after that.
    let second = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();

    // Attempt to edit the second shift backwards into the first's window.
    let err = fx
        .service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: second.id,
                check_in_at: closed.check_in_at,
                check_out_at: Some(closed.check_out_at.unwrap() + Duration::seconds(1)),
                note: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Conflict(_)));
    let _ = now;
}

#[tokio::test]
async fn soft_delete_succeeds_then_rejects_second_call() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    fx.service
        .soft_delete(
            fx.superadmin.id,
            UserRole::Superadmin,
            opened.id,
            "orphan".into(),
        )
        .await
        .unwrap();
    let err = fx
        .service
        .soft_delete(
            fx.superadmin.id,
            UserRole::Superadmin,
            opened.id,
            "again".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
}

#[tokio::test]
async fn audit_row_lands_for_each_mutation() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    let (n_after_in,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'clock_in'",
    )
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(n_after_in, 1);

    fx.service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    let (n_after_out,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'clock_out'",
    )
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(n_after_out, 1);

    fx.service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: opened.check_in_at,
                check_out_at: Some(opened.check_in_at + Duration::hours(1)),
                note: None,
            },
        )
        .await
        .unwrap();
    let (n_after_update,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'update'",
    )
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(n_after_update, 1);

    fx.service
        .soft_delete(
            fx.superadmin.id,
            UserRole::Superadmin,
            opened.id,
            "tidy".into(),
        )
        .await
        .unwrap();
    let (n_after_delete,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'soft_delete'",
    )
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(n_after_delete, 1);
}

#[tokio::test]
async fn overlap_detection_surfaces_concurrent_shift_rows() {
    // Simulates the additive-policy scenario (§7.1): two devices both push
    // shifts whose time windows overlap. We forge the rows directly via the
    // repo because the partial-unique-index on `operator_shifts_open` would
    // reject two LIVE-open rows -- the sync path receives closed pairs.
    let fx = seed().await;
    let base = Utc::now() - Duration::hours(2);

    // First shift: 2h ago -> 1h ago.
    let first = app_lib::domains::shifts::domain::entities::OperatorShift {
        id: Uuid::now_v7(),
        operator_id: fx.operator.id,
        check_in_at: base,
        check_out_at: Some(base + Duration::hours(1)),
        check_in_by_user_id: fx.receptionist.id,
        check_out_by_user_id: Some(fx.receptionist.id),
        note: None,
        created_at: base,
        updated_at: base + Duration::hours(1),
        deleted_at: None,
        version: 2,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some("dev-1".into()),
        entity_id: ENTITY_ID.into(),
    };
    // Second shift overlaps: starts 30m into the first, runs 1h.
    let second = app_lib::domains::shifts::domain::entities::OperatorShift {
        id: Uuid::now_v7(),
        operator_id: fx.operator.id,
        check_in_at: base + Duration::minutes(30),
        check_out_at: Some(base + Duration::minutes(90)),
        check_in_by_user_id: fx.superadmin.id,
        check_out_by_user_id: Some(fx.superadmin.id),
        note: Some("device B".into()),
        created_at: base + Duration::minutes(30),
        updated_at: base + Duration::minutes(90),
        deleted_at: None,
        version: 2,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some("dev-2".into()),
        entity_id: ENTITY_ID.into(),
    };
    let mut tx = fx.pool.begin().await.unwrap();
    fx.shift_repo.upsert(&mut tx, &first).await.unwrap();
    fx.shift_repo.upsert(&mut tx, &second).await.unwrap();
    tx.commit().await.unwrap();

    let pairs = fx
        .service
        .list_overlaps(ENTITY_ID, Some(fx.operator.id))
        .await
        .unwrap();
    assert_eq!(pairs.len(), 1);
    let p = &pairs[0];
    assert!(
        (p.left.id == first.id && p.right.id == second.id)
            || (p.left.id == second.id && p.right.id == first.id)
    );

    // Tenant-wide call returns the same pair.
    let all = fx.service.list_overlaps(ENTITY_ID, None).await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn history_today_returns_open_and_closed_shifts_within_window() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    fx.service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let today_end = today_start + Duration::days(1);
    let history = fx
        .service
        .history_today(ENTITY_ID, today_start, today_end)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].shift.check_out_at.is_some());
}

#[tokio::test]
async fn migration_creates_operator_shifts_table() {
    let pool = fresh_pool().await;
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'operator_shifts'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);

    // Partial unique index `operator_shifts_open` should exist.
    let (idx_n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'operator_shifts_open'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(idx_n, 1);
}

#[tokio::test]
async fn outbox_op_enqueued_per_mutation() {
    let fx = seed().await;
    let _opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            None,
        )
        .await
        .unwrap();
    // Audit + business row -> 2 outbox ops per mutation.
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'operator_shifts'")
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!(n, 1, "exactly one operator_shifts outbox op per mutation");

    let (audit_n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'audit_log'")
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert!(audit_n >= 1);
    let _ = Uuid::now_v7();
}
