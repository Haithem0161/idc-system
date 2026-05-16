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

// ---------------------------------------------------------------------------
// Plan §2.1 + gap-derived scenarios (§9-§12). One assertion focus per test.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clock_in_rejects_when_operator_belongs_to_other_tenant() {
    let fx = seed().await;
    let err = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            "tenant-other",
            fx.operator.id,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM operator_shifts")
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let (outbox_n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'operator_shifts'")
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!(outbox_n, 0);
}

#[tokio::test]
async fn clock_in_rejects_when_operator_soft_deleted() {
    let fx = seed().await;
    let deleted = fx.operator.clone().soft_deleted();
    let repo = SqliteOperatorRepo::new(fx.pool.clone());
    let mut tx = fx.pool.begin().await.unwrap();
    repo.upsert(&mut tx, &deleted).await.unwrap();
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
async fn clock_out_rejects_already_closed_shift() {
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
    let err = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Conflict(_)));
    // outbox carries one row per mutation -> 2 total for the original
    // clock_in + the first clock_out, NOT 3.
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'operator_shifts'")
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn clock_out_rejects_soft_deleted_shift() {
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
            "tidy".into(),
        )
        .await
        .unwrap();
    let err = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
}

#[tokio::test]
async fn edit_rejects_when_target_shift_soft_deleted() {
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
            "tidy".into(),
        )
        .await
        .unwrap();
    let err = fx
        .service
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
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
}

// P04-G01: edit rejects future check_in_at.
#[tokio::test]
async fn edit_rejects_when_new_check_in_at_in_future() {
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
    let closed = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    let future = Utc::now() + Duration::hours(2);
    let err = fx
        .service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: future,
                check_out_at: Some(future + Duration::minutes(5)),
                note: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
    // No version bump beyond what clock_in + clock_out did.
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert_eq!(reloaded.version, closed.version);
}

#[tokio::test]
async fn edit_clears_note_when_replace_with_none() {
    let fx = seed().await;
    let opened = fx
        .service
        .clock_in(
            fx.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            fx.operator.id,
            Some("am".into()),
        )
        .await
        .unwrap();
    fx.service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: opened.check_in_at,
                check_out_at: None,
                note: Some(None),
            },
        )
        .await
        .unwrap();
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert!(reloaded.note.is_none());
}

#[tokio::test]
async fn soft_delete_rejects_non_superadmin() {
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
        .soft_delete(
            fx.receptionist.id,
            UserRole::Receptionist,
            opened.id,
            "x".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert!(reloaded.deleted_at.is_none());
}

#[tokio::test]
async fn list_open_filters_by_entity_id() {
    let fx = seed().await;
    // Open one shift in tenant-x.
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
    // Inject a hand-rolled row in another tenant directly via repo.
    let other_op = app_lib::domains::catalog::domain::entities::Operator::try_new(
        app_lib::domains::catalog::domain::entities::operator::OperatorNewInput {
            name: "Other Tenant Op".into(),
            phone: None,
            base_cut_per_check_iqd: 1_000,
            notes: None,
            entity_id: "tenant-other".into(),
            origin_device_id: None,
        },
    )
    .unwrap();
    let other_user = User::try_new(
        "other@example.com",
        "Other",
        UserRole::Receptionist,
        "x".into(),
        "tenant-other".into(),
        None,
    )
    .unwrap();
    let op_repo = SqliteOperatorRepo::new(fx.pool.clone());
    let user_repo = SqliteUserRepo::new(fx.pool.clone());
    let mut tx = fx.pool.begin().await.unwrap();
    op_repo.upsert(&mut tx, &other_op).await.unwrap();
    user_repo.upsert(&mut tx, &other_user).await.unwrap();
    let alien = app_lib::domains::shifts::domain::entities::OperatorShift {
        id: Uuid::now_v7(),
        operator_id: other_op.id,
        check_in_at: Utc::now() - Duration::minutes(20),
        check_out_at: None,
        check_in_by_user_id: other_user.id,
        check_out_by_user_id: None,
        note: None,
        created_at: Utc::now() - Duration::minutes(20),
        updated_at: Utc::now() - Duration::minutes(20),
        deleted_at: None,
        version: 1,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some("dev-other".into()),
        entity_id: "tenant-other".into(),
    };
    fx.shift_repo.upsert(&mut tx, &alien).await.unwrap();
    tx.commit().await.unwrap();

    let here = fx.service.list_open(ENTITY_ID).await.unwrap();
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].shift.entity_id, ENTITY_ID);
    let alien_list = fx.service.list_open("tenant-other").await.unwrap();
    assert_eq!(alien_list.len(), 1);
    assert_eq!(alien_list[0].shift.entity_id, "tenant-other");
}

#[tokio::test]
async fn list_open_excludes_soft_deleted() {
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
            "x".into(),
        )
        .await
        .unwrap();
    let listed = fx.service.list_open(ENTITY_ID).await.unwrap();
    assert!(listed.is_empty());
}

#[tokio::test]
async fn history_today_excludes_yesterday_and_tomorrow() {
    let fx = seed().await;
    // Inject a shift dated yesterday and one dated tomorrow.
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let yesterday = today_start - Duration::hours(2);
    let tomorrow = today_start + Duration::days(1) + Duration::hours(2);

    let mk = |id, in_at: chrono::DateTime<Utc>, out_at| {
        app_lib::domains::shifts::domain::entities::OperatorShift {
            id,
            operator_id: fx.operator.id,
            check_in_at: in_at,
            check_out_at: out_at,
            check_in_by_user_id: fx.receptionist.id,
            check_out_by_user_id: Some(fx.receptionist.id),
            note: None,
            created_at: in_at,
            updated_at: in_at,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: None,
            origin_device_id: Some(DEVICE_ID.into()),
            entity_id: ENTITY_ID.into(),
        }
    };
    let y = mk(
        Uuid::now_v7(),
        yesterday,
        Some(yesterday + Duration::hours(1)),
    );
    let t = mk(
        Uuid::now_v7(),
        tomorrow,
        Some(tomorrow + Duration::hours(1)),
    );
    let mut tx = fx.pool.begin().await.unwrap();
    fx.shift_repo.upsert(&mut tx, &y).await.unwrap();
    fx.shift_repo.upsert(&mut tx, &t).await.unwrap();
    tx.commit().await.unwrap();

    let history = fx
        .service
        .history_today(ENTITY_ID, today_start, today_start + Duration::days(1))
        .await
        .unwrap();
    let ids: Vec<_> = history.iter().map(|h| h.shift.id).collect();
    assert!(!ids.contains(&y.id));
    assert!(!ids.contains(&t.id));
}

#[tokio::test]
async fn list_overlaps_returns_empty_when_no_overlap() {
    let fx = seed().await;
    let base = Utc::now() - Duration::hours(5);
    let mk = |start: chrono::DateTime<Utc>, end: chrono::DateTime<Utc>| {
        app_lib::domains::shifts::domain::entities::OperatorShift {
            id: Uuid::now_v7(),
            operator_id: fx.operator.id,
            check_in_at: start,
            check_out_at: Some(end),
            check_in_by_user_id: fx.receptionist.id,
            check_out_by_user_id: Some(fx.receptionist.id),
            note: None,
            created_at: start,
            updated_at: end,
            deleted_at: None,
            version: 2,
            dirty: false,
            last_synced_at: None,
            origin_device_id: Some(DEVICE_ID.into()),
            entity_id: ENTITY_ID.into(),
        }
    };
    let a = mk(base, base + Duration::hours(1));
    let b = mk(base + Duration::hours(2), base + Duration::hours(3));
    let mut tx = fx.pool.begin().await.unwrap();
    fx.shift_repo.upsert(&mut tx, &a).await.unwrap();
    fx.shift_repo.upsert(&mut tx, &b).await.unwrap();
    tx.commit().await.unwrap();
    let pairs = fx
        .service
        .list_overlaps(ENTITY_ID, Some(fx.operator.id))
        .await
        .unwrap();
    assert!(pairs.is_empty());
}

#[tokio::test]
async fn partial_unique_index_blocks_concurrent_open_shifts_at_db_layer() {
    let fx = seed().await;
    let mk_open = |dev: &str| app_lib::domains::shifts::domain::entities::OperatorShift {
        id: Uuid::now_v7(),
        operator_id: fx.operator.id,
        check_in_at: Utc::now(),
        check_out_at: None,
        check_in_by_user_id: fx.receptionist.id,
        check_out_by_user_id: None,
        note: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
        version: 1,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some(dev.into()),
        entity_id: ENTITY_ID.into(),
    };
    let first = mk_open("dev-1");
    let second = mk_open("dev-2");
    let mut tx = fx.pool.begin().await.unwrap();
    fx.shift_repo.upsert(&mut tx, &first).await.unwrap();
    let res = fx.shift_repo.upsert(&mut tx, &second).await;
    tx.rollback().await.unwrap();
    assert!(
        res.is_err(),
        "second open row for same operator MUST trip the partial unique index"
    );
}

#[tokio::test]
async fn history_today_index_used_by_query_plan() {
    let fx = seed().await;
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let today_end = today_start + Duration::days(1);
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN \
         SELECT id FROM operator_shifts \
         WHERE entity_id = ? AND check_in_at >= ? AND check_in_at < ? AND deleted_at IS NULL",
    )
    .bind(ENTITY_ID)
    .bind(today_start.to_rfc3339())
    .bind(today_end.to_rfc3339())
    .fetch_all(&fx.pool)
    .await
    .unwrap();
    let used_index = plan
        .iter()
        .any(|(_, _, _, detail)| detail.contains("operator_shifts_today"));
    assert!(
        used_index,
        "history_today plan should use operator_shifts_today: {:?}",
        plan
    );
}

// P04-G02: DB-layer CHECK constraint
#[tokio::test]
async fn db_check_constraint_blocks_check_out_before_check_in() {
    let pool = fresh_pool().await;
    // Seed a user + operator so the FK row exists.
    let user_repo = SqliteUserRepo::new(pool.clone());
    let op_repo = SqliteOperatorRepo::new(pool.clone());
    let user = User::try_new(
        "u@example.com",
        "U",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        None,
    )
    .unwrap();
    let op = app_lib::domains::catalog::domain::entities::Operator::try_new(
        app_lib::domains::catalog::domain::entities::operator::OperatorNewInput {
            name: "Op".into(),
            phone: None,
            base_cut_per_check_iqd: 1,
            notes: None,
            entity_id: ENTITY_ID.into(),
            origin_device_id: None,
        },
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    op_repo.upsert(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let id = Uuid::now_v7().to_string();
    let in_at = "2026-05-14T10:00:00+00:00";
    let bad_out_at = "2026-05-14T09:00:00+00:00";
    let err = sqlx::query(
        "INSERT INTO operator_shifts \
        (id, operator_id, check_in_at, check_out_at, check_in_by_user_id, \
         check_out_by_user_id, note, created_at, updated_at, deleted_at, version, dirty, \
         last_synced_at, origin_device_id, entity_id) \
         VALUES (?, ?, ?, ?, ?, NULL, NULL, ?, ?, NULL, 1, 1, NULL, NULL, ?)",
    )
    .bind(&id)
    .bind(op.id.to_string())
    .bind(in_at)
    .bind(bad_out_at)
    .bind(user.id.to_string())
    .bind(in_at)
    .bind(in_at)
    .bind(ENTITY_ID)
    .execute(&pool)
    .await
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK constraint failure, got {msg}"
    );

    // Positive control: NULL check_out_at succeeds.
    let id_ok = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO operator_shifts \
        (id, operator_id, check_in_at, check_out_at, check_in_by_user_id, \
         check_out_by_user_id, note, created_at, updated_at, deleted_at, version, dirty, \
         last_synced_at, origin_device_id, entity_id) \
         VALUES (?, ?, ?, NULL, ?, NULL, NULL, ?, ?, NULL, 1, 1, NULL, NULL, ?)",
    )
    .bind(&id_ok)
    .bind(op.id.to_string())
    .bind(in_at)
    .bind(user.id.to_string())
    .bind(in_at)
    .bind(in_at)
    .bind(ENTITY_ID)
    .execute(&pool)
    .await
    .unwrap();
}

// P04-G18: index created in migration 004 with the expected column order.
#[tokio::test]
async fn migration_004_creates_operator_shifts_today_index() {
    let pool = fresh_pool().await;
    let names: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master \
         WHERE type='index' AND tbl_name='operator_shifts'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let set: std::collections::HashSet<_> = names.into_iter().map(|(n,)| n).collect();
    assert!(
        set.contains("operator_shifts_today"),
        "missing today index: {:?}",
        set
    );
    assert!(
        set.contains("operator_shifts_open"),
        "missing open index: {:?}",
        set
    );

    let info: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT seqno, cid, name FROM pragma_index_info('operator_shifts_today')")
            .fetch_all(&pool)
            .await
            .unwrap();
    let cols: Vec<String> = info.into_iter().map(|(_, _, n)| n).collect();
    assert_eq!(cols, vec!["entity_id", "check_in_at"]);
}

// P04-G25: operator_shifts_open is unique + partial with the expected predicate.
#[tokio::test]
async fn migration_004_creates_operator_shifts_open_partial_index_with_exact_where_clause() {
    let pool = fresh_pool().await;
    let row: (String,) = sqlx::query_as(
        "SELECT COALESCE(sql, '') FROM sqlite_master \
         WHERE type='index' AND name='operator_shifts_open'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let sql = row.0.to_lowercase();
    assert!(
        sql.contains("unique"),
        "operator_shifts_open MUST be unique: {sql}"
    );
    assert!(
        sql.contains("where check_out_at is null and deleted_at is null"),
        "operator_shifts_open MUST carry the exact WHERE predicate: {sql}"
    );
    let info: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT seqno, cid, name FROM pragma_index_info('operator_shifts_open')")
            .fetch_all(&pool)
            .await
            .unwrap();
    let cols: Vec<String> = info.into_iter().map(|(_, _, n)| n).collect();
    assert_eq!(cols, vec!["operator_id"]);
}

// P04-G11 / P04-G26: audit-first rollback per callsite.
#[tokio::test]
async fn edit_rolls_back_business_write_when_audit_insert_fails() {
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
    let closed = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    sqlx::query("DROP TABLE audit_log")
        .execute(&fx.pool)
        .await
        .unwrap();
    let new_in = closed.check_in_at - Duration::minutes(10);
    let new_out = Some(closed.check_in_at + Duration::minutes(10));
    let res = fx
        .service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: new_in,
                check_out_at: new_out,
                note: None,
            },
        )
        .await;
    assert!(res.is_err());
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert_eq!(reloaded.version, closed.version);
    assert_eq!(reloaded.check_in_at, closed.check_in_at);
}

#[tokio::test]
async fn clock_out_rolls_back_business_write_when_audit_insert_fails() {
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
    sqlx::query("DROP TABLE audit_log")
        .execute(&fx.pool)
        .await
        .unwrap();
    let res = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await;
    assert!(res.is_err());
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert!(reloaded.check_out_at.is_none());
    assert_eq!(reloaded.version, opened.version);
}

#[tokio::test]
async fn soft_delete_rolls_back_business_write_when_audit_insert_fails() {
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
    sqlx::query("DROP TABLE audit_log")
        .execute(&fx.pool)
        .await
        .unwrap();
    let res = fx
        .service
        .soft_delete(
            fx.superadmin.id,
            UserRole::Superadmin,
            opened.id,
            "x".into(),
        )
        .await;
    assert!(res.is_err());
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert!(reloaded.deleted_at.is_none());
}

// P04-G15: soft_delete outbox row carries additive-update envelope (no tombstone).
#[tokio::test]
async fn soft_delete_outbox_row_carries_additive_update_envelope_not_tombstone() {
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
            "tidy".into(),
        )
        .await
        .unwrap();
    let payload: (Vec<u8>,) = sqlx::query_as(
        "SELECT payload FROM outbox WHERE entity = 'operator_shifts' AND entity_id = ? \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(opened.id.to_string())
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    // Outbox payload is serde_json::to_vec'd JSON bytes.
    let json: serde_json::Value = serde_json::from_slice(&payload.0).unwrap();
    assert!(
        json.get("deleted_at").and_then(|v| v.as_str()).is_some(),
        "soft_delete payload must carry deleted_at, got {json}"
    );
    assert!(
        json.get("tombstone").is_none(),
        "additive-only contract forbids tombstone field on the wire"
    );
}

// Plan §2.1: edit audit delta records old + new times.
#[tokio::test]
async fn edit_audit_delta_records_old_and_new_times() {
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
    let new_out = opened.check_in_at + Duration::minutes(45);
    fx.service
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
    let (delta,): (String,) = sqlx::query_as(
        "SELECT delta FROM audit_log \
         WHERE entity = 'operator_shifts' AND action = 'update' AND entity_id = ?",
    )
    .bind(opened.id.to_string())
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&delta).unwrap();
    let obj = parsed.as_object().expect("delta must be an object");
    let check_in = obj
        .get("check_in_at")
        .expect("check_in_at must appear in delta");
    assert!(check_in.get("from").is_some());
    assert!(check_in.get("to").is_some());
    let check_out = obj
        .get("check_out_at")
        .expect("check_out_at must appear in delta");
    assert!(check_out.get("from").is_some());
    assert!(check_out.get("to").is_some());
    // version always bumps -> shows up too
    assert!(obj.contains_key("version"));
}

// sync_version monotonicity, see §6.8.
#[tokio::test]
async fn version_increments_monotonically_per_mutation() {
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
    assert_eq!(opened.version, 1);
    let closed = fx
        .service
        .clock_out(fx.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    assert_eq!(closed.version, 2);
    let edited = fx
        .service
        .edit(
            fx.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: opened.check_in_at,
                check_out_at: Some(opened.check_in_at + Duration::minutes(10)),
                note: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.version, 3);
    fx.service
        .soft_delete(
            fx.superadmin.id,
            UserRole::Superadmin,
            opened.id,
            "tidy".into(),
        )
        .await
        .unwrap();
    let reloaded = fx.shift_repo.get_by_id(opened.id).await.unwrap().unwrap();
    assert_eq!(reloaded.version, 4);
}

// P04-G17: ON DELETE RESTRICT differentiated per FK column.
#[tokio::test]
async fn restrict_user_hard_delete_when_referenced_as_check_in_by() {
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
    let _ = opened;
    let err = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(fx.receptionist.id.to_string())
        .execute(&fx.pool)
        .await
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.to_lowercase().contains("foreign") || msg.to_lowercase().contains("constraint"));
}

#[tokio::test]
async fn restrict_user_hard_delete_when_referenced_as_check_out_by() {
    let fx = seed().await;
    // Clock in with receptionist, clock out with superadmin so the two FKs
    // reference distinct users.
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
        .clock_out(fx.superadmin.id, UserRole::Superadmin, opened.id)
        .await
        .unwrap();
    let err = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(fx.superadmin.id.to_string())
        .execute(&fx.pool)
        .await
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.to_lowercase().contains("foreign") || msg.to_lowercase().contains("constraint"));
}

// §6.8: FK enforcement
#[tokio::test]
async fn fk_enforcement_blocks_orphan_shifts() {
    let pool = fresh_pool().await;
    let id = Uuid::now_v7().to_string();
    let now = "2026-05-14T10:00:00+00:00";
    let res = sqlx::query(
        "INSERT INTO operator_shifts \
        (id, operator_id, check_in_at, check_out_at, check_in_by_user_id, \
         check_out_by_user_id, note, created_at, updated_at, deleted_at, version, dirty, \
         last_synced_at, origin_device_id, entity_id) \
         VALUES (?, ?, ?, NULL, ?, NULL, NULL, ?, ?, NULL, 1, 1, NULL, NULL, ?)",
    )
    .bind(&id)
    .bind(Uuid::now_v7().to_string()) // bogus operator_id
    .bind(now)
    .bind(Uuid::now_v7().to_string()) // bogus user_id
    .bind(now)
    .bind(now)
    .bind(ENTITY_ID)
    .execute(&pool)
    .await;
    assert!(res.is_err(), "FK should reject orphan operator/user");
}
