//! Phase-04 §5 canonical persona script: **P2 Mehdi the Receptionist**.
//!
//! 10-step walk through every IPC surface phase-04 ships, exercising the
//! day-script in `personas.md` (open shift -> work -> clock out ->
//! superadmin retro-edit -> overlap surfaced and resolved -> historical
//! read-only review). This is the DoD-canonical persona for phase-04.

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
const DEVICE_A: &str = "dev-mehdi";
const DEVICE_B: &str = "dev-clinic";

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
async fn persona_p2_mehdi_walks_through_phase04_shift_day() {
    // ---- bootstrap ----------------------------------------------------------
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
        DEVICE_A.into(),
    );

    // Mehdi (receptionist), Mariam (superadmin), Kareem (operator).
    let mehdi = User::try_new(
        "mehdi@idc.io",
        "Mehdi",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_A.into()),
    )
    .unwrap();
    let mariam = User::try_new(
        "mariam@idc.io",
        "Mariam",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_A.into()),
    )
    .unwrap();
    let kareem = Operator::try_new(OperatorNewInput {
        name: "Kareem".into(),
        phone: Some("07700000111".into()),
        base_cut_per_check_iqd: 5_000,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_A.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &mehdi).await.unwrap();
    user_repo.upsert(&mut tx, &mariam).await.unwrap();
    operator_repo.upsert(&mut tx, &kareem).await.unwrap();
    tx.commit().await.unwrap();

    // ---- step 1 -- Mehdi opens the day, on-shift list is empty -------------
    let open = service.list_open(ENTITY_ID).await.unwrap();
    assert!(open.is_empty(), "no one on shift at 8 am");

    // ---- step 2 -- Mehdi clocks Kareem in ----------------------------------
    let shift = service
        .clock_in(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            kareem.id,
            Some("starts at 8".into()),
        )
        .await
        .unwrap();
    assert!(shift.is_open());

    // ---- step 3 -- On-shift table shows Kareem with hydrated meta ---------
    let open = service.list_open(ENTITY_ID).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].operator_name, "Kareem");
    assert_eq!(open[0].operator_phone.as_deref(), Some("07700000111"));

    // ---- step 4 -- Double clock-in rejected (operator already on shift) ----
    let err = service
        .clock_in(mehdi.id, UserRole::Receptionist, ENTITY_ID, kareem.id, None)
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Conflict(_)));

    // ---- step 5 -- Mid-day, Mehdi clocks Kareem out ------------------------
    let closed = service
        .clock_out(mehdi.id, UserRole::Receptionist, shift.id)
        .await
        .unwrap();
    assert!(closed.check_out_at.is_some());
    let open = service.list_open(ENTITY_ID).await.unwrap();
    assert!(open.is_empty());

    // ---- step 6 -- Today's history surfaces the closed shift ---------------
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let today_end = today_start + Duration::days(1);
    let history = service
        .history_today(ENTITY_ID, today_start, today_end)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);

    // ---- step 7 -- Mariam (superadmin) retro-edits to shift `in` 15m back -
    let new_in = closed.check_in_at - Duration::minutes(15);
    let edited = service
        .edit(
            mariam.id,
            UserRole::Superadmin,
            ShiftEditInput {
                shift_id: closed.id,
                check_in_at: new_in,
                check_out_at: closed.check_out_at,
                note: Some(Some("started early to set up".into())),
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.check_in_at, new_in);
    assert_eq!(edited.note.as_deref(), Some("started early to set up"));
    // Audit trail records the update.
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity = 'operator_shifts' AND action = 'update' AND entity_id = ?",
    )
    .bind(closed.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);

    // ---- step 8 -- A second device pushed an overlapping shift in the
    //              afternoon while Mehdi was offline. Inject it via the
    //              pull-apply path (raw repo upsert) and verify the
    //              overlap banner surface picks it up.
    // Conflict shift extends a minute earlier and a minute later than the
    // edited window so the intervals strictly overlap. Using the edited
    // shift's check_in_at as the anchor keeps the math invariant against
    // wall-clock drift between `clock_in` and `clock_out` in the test rig.
    let conflict_in = edited.check_in_at - Duration::minutes(1);
    let conflict_out = edited.check_in_at + Duration::minutes(20);
    let conflict_shift = app_lib::domains::shifts::domain::entities::OperatorShift {
        id: Uuid::now_v7(),
        operator_id: kareem.id,
        check_in_at: conflict_in,
        check_out_at: Some(conflict_out),
        check_in_by_user_id: mehdi.id,
        check_out_by_user_id: Some(mehdi.id),
        note: Some("device B push".into()),
        created_at: conflict_in,
        updated_at: conflict_out,
        deleted_at: None,
        version: 2,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some(DEVICE_B.into()),
        entity_id: ENTITY_ID.into(),
    };
    let mut tx = pool.begin().await.unwrap();
    shift_repo.upsert(&mut tx, &conflict_shift).await.unwrap();
    tx.commit().await.unwrap();
    let overlaps = service
        .list_overlaps(ENTITY_ID, Some(kareem.id))
        .await
        .unwrap();
    assert_eq!(overlaps.len(), 1);

    // ---- step 9 -- Mariam soft-deletes the device-B duplicate --------------
    service
        .soft_delete(
            mariam.id,
            UserRole::Superadmin,
            conflict_shift.id,
            "device B duplicate".into(),
        )
        .await
        .unwrap();
    let overlaps = service
        .list_overlaps(ENTITY_ID, Some(kareem.id))
        .await
        .unwrap();
    assert!(overlaps.is_empty());

    // The soft-deleted row still exists in the table (tombstone).
    let (raw_n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM operator_shifts WHERE id = ? AND deleted_at IS NOT NULL",
    )
    .bind(conflict_shift.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw_n, 1);

    // ---- step 10 -- Day-end sanity: history shows one shift, no overlaps,
    //                 audit log carries the four expected mutations.
    let history = service
        .history_today(ENTITY_ID, today_start, today_end)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    let (clock_in_n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'clock_in'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(clock_in_n, 1);
    let (clock_out_n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'clock_out'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(clock_out_n, 1);
    let (update_n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'update'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(update_n, 1);
    let (soft_delete_n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operator_shifts' AND action = 'soft_delete'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(soft_delete_n, 1);
}
