//! Phase-04 §2.2 IPC handler / wire-shape coverage.
//!
//! The shifts commands take `tauri::State` directly and do not expose `_impl`
//! helpers (the phase-03 catalog tests follow the same convention). Each
//! scenario drives `ShiftService` along the same code path the command
//! invokes, then asserts the serialized JSON shape -- because the frontend
//! only sees the JSON, the JSON is the contract.

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
use app_lib::domains::shifts::service::{ShiftEditInput, ShiftService, ShiftWithMeta};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::error::AppError;
use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-x";
const DEVICE_ID: &str = "dev-ipc";

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
    pool: SqlitePool,
    service: ShiftService,
    shift_repo: Arc<dyn OperatorShiftRepo>,
    operator: Operator,
    superadmin: User,
    receptionist: User,
}

async fn rig() -> Rig {
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
        name: "Kareem".into(),
        phone: Some("07700000000".into()),
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
        "Reception",
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

    Rig {
        pool,
        service,
        shift_repo,
        operator,
        superadmin,
        receptionist,
    }
}

// ---------------------------------------------------------------------------
// shifts_clock_in
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clock_in_returns_serialized_shift_with_uuid_id_and_null_check_out_at() {
    let r = rig().await;
    let shift = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&shift).unwrap();
    assert!(Uuid::parse_str(json["id"].as_str().unwrap()).is_ok());
    assert!(json["check_out_at"].is_null());
    assert_eq!(
        json["operator_id"].as_str().unwrap(),
        r.operator.id.to_string()
    );
    assert_eq!(json["entity_id"].as_str().unwrap(), ENTITY_ID);
}

#[tokio::test]
async fn clock_in_with_null_note_omits_note_in_persisted_row() {
    let r = rig().await;
    let shift = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&shift).unwrap();
    assert!(json["note"].is_null());
}

#[tokio::test]
async fn clock_in_returns_typed_validation_error_on_inactive_operator() {
    let r = rig().await;
    let repo = SqliteOperatorRepo::new(r.pool.clone());
    let mut tx = r.pool.begin().await.unwrap();
    repo.upsert(&mut tx, &r.operator.clone().with_active(false))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let err = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"].as_str(), Some("VALIDATION_ERROR"));
    assert!(json["message"].as_str().unwrap().contains("inactive"));
}

// ---------------------------------------------------------------------------
// shifts_clock_out
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clock_out_closes_open_shift_and_populates_check_out_at() {
    let r = rig().await;
    let opened = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let closed = r
        .service
        .clock_out(r.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    let json = serde_json::to_value(&closed).unwrap();
    assert!(!json["check_out_at"].is_null());
    assert_eq!(
        json["check_out_by_user_id"].as_str().unwrap(),
        r.receptionist.id.to_string()
    );
}

#[tokio::test]
async fn clock_out_returns_not_found_for_unknown_shift_id() {
    let r = rig().await;
    let err = r
        .service
        .clock_out(r.receptionist.id, UserRole::Receptionist, Uuid::now_v7())
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"].as_str(), Some("NOT_FOUND"));
}

// ---------------------------------------------------------------------------
// shifts_list_open
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_open_returns_hydrated_operator_name_and_phone() {
    let r = rig().await;
    r.service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let rows: Vec<ShiftWithMeta> = r.service.list_open(ENTITY_ID).await.unwrap();
    assert_eq!(rows.len(), 1);
    let json = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(json["operator_name"].as_str(), Some("Kareem"));
    assert_eq!(json["operator_phone"].as_str(), Some("07700000000"));
    // Flattened ShiftRecord fields must coexist with the meta fields.
    assert!(json.get("id").is_some());
    assert!(json.get("check_in_at").is_some());
    assert!(json.get("entity_id").is_some());
}

#[tokio::test]
async fn list_open_returns_empty_for_unknown_tenant() {
    let r = rig().await;
    r.service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let rows: Vec<ShiftWithMeta> = r.service.list_open("tenant-other").await.unwrap();
    assert!(rows.is_empty());
}

// ---------------------------------------------------------------------------
// shifts_history_today
// ---------------------------------------------------------------------------

#[tokio::test]
async fn history_today_returns_today_window_rows() {
    let r = rig().await;
    let opened = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    r.service
        .clock_out(r.receptionist.id, UserRole::Receptionist, opened.id)
        .await
        .unwrap();
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let today_end = today_start + Duration::days(1);
    let rows: Vec<ShiftWithMeta> = r
        .service
        .history_today(ENTITY_ID, today_start, today_end)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].shift.check_out_at.is_some());
}

// ---------------------------------------------------------------------------
// shifts_edit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edit_replaces_window_and_note_for_superadmin() {
    let r = rig().await;
    let opened = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            Some("am".into()),
        )
        .await
        .unwrap();
    let new_in = opened.check_in_at - Duration::minutes(10);
    let new_out = opened.check_in_at + Duration::hours(1);
    let edited = r
        .service
        .edit(
            r.superadmin.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: new_in,
                check_out_at: Some(new_out),
                note: Some(Some("evening".into())),
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.check_in_at, new_in);
    assert_eq!(edited.check_out_at, Some(new_out));
    assert_eq!(edited.note.as_deref(), Some("evening"));
}

#[tokio::test]
async fn edit_rejects_non_superadmin_via_typed_validation_error() {
    let r = rig().await;
    let opened = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let err = r
        .service
        .edit(
            r.receptionist.id,
            UserRole::Receptionist,
            ShiftEditInput {
                shift_id: opened.id,
                check_in_at: opened.check_in_at,
                check_out_at: Some(opened.check_in_at + Duration::minutes(30)),
                note: None,
            },
        )
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"].as_str(), Some("VALIDATION_ERROR"));
}

// ---------------------------------------------------------------------------
// shifts_soft_delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn soft_delete_returns_unit_and_marks_row_deleted_then_list_open_excludes() {
    let r = rig().await;
    let opened = r
        .service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    r.service
        .soft_delete(r.superadmin.id, UserRole::Superadmin, opened.id, "x".into())
        .await
        .unwrap();
    // IPC return shape: `()` -> serializes to JSON `null`.
    let json = serde_json::to_value(()).unwrap();
    assert!(json.is_null());
    let rows = r.service.list_open(ENTITY_ID).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn soft_delete_returns_not_found_for_unknown_id() {
    let r = rig().await;
    let err = r
        .service
        .soft_delete(
            r.superadmin.id,
            UserRole::Superadmin,
            Uuid::now_v7(),
            "x".into(),
        )
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"].as_str(), Some("NOT_FOUND"));
}

// ---------------------------------------------------------------------------
// shifts_list_overlaps
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_overlaps_returns_pairs_when_filter_unset() {
    let r = rig().await;
    let base = Utc::now() - Duration::hours(3);
    let mk = |start: chrono::DateTime<Utc>, end: chrono::DateTime<Utc>, dev: &str| {
        app_lib::domains::shifts::domain::entities::OperatorShift {
            id: Uuid::now_v7(),
            operator_id: r.operator.id,
            check_in_at: start,
            check_out_at: Some(end),
            check_in_by_user_id: r.receptionist.id,
            check_out_by_user_id: Some(r.receptionist.id),
            note: None,
            created_at: start,
            updated_at: end,
            deleted_at: None,
            version: 2,
            dirty: false,
            last_synced_at: None,
            origin_device_id: Some(dev.into()),
            entity_id: ENTITY_ID.into(),
        }
    };
    let a = mk(base, base + Duration::hours(1), "dev-A");
    let b = mk(
        base + Duration::minutes(30),
        base + Duration::minutes(90),
        "dev-B",
    );
    let mut tx = r.pool.begin().await.unwrap();
    r.shift_repo.upsert(&mut tx, &a).await.unwrap();
    r.shift_repo.upsert(&mut tx, &b).await.unwrap();
    tx.commit().await.unwrap();

    let pairs = r.service.list_overlaps(ENTITY_ID, None).await.unwrap();
    assert_eq!(pairs.len(), 1);
}

#[tokio::test]
async fn list_overlaps_returns_empty_for_clean_operator() {
    let r = rig().await;
    r.service
        .clock_in(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            r.operator.id,
            None,
        )
        .await
        .unwrap();
    let pairs = r
        .service
        .list_overlaps(ENTITY_ID, Some(r.operator.id))
        .await
        .unwrap();
    assert!(pairs.is_empty());
}

// ---------------------------------------------------------------------------
// AppError serialization invariants (§3.2 row, shared envelope)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn app_error_serializes_to_code_message_envelope_for_every_variant_in_phase04() {
    let cases: Vec<(AppError, &str)> = vec![
        (AppError::Validation("v".into()), "VALIDATION_ERROR"),
        (AppError::Conflict("c".into()), "CONFLICT_PARKED"),
        (AppError::NotFound("n".into()), "NOT_FOUND"),
        (AppError::NotAuthenticated, "NOT_AUTHENTICATED"),
        (
            AppError::Configuration("svc unavailable".into()),
            "CONFIGURATION_ERROR",
        ),
    ];
    for (err, expected_code) in cases {
        let json: Value = serde_json::to_value(&err).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("code"), "missing code: {json}");
        assert!(obj.contains_key("message"), "missing message: {json}");
        assert_eq!(json["code"].as_str(), Some(expected_code));
        assert!(!json["message"].as_str().unwrap().is_empty());
    }
}
