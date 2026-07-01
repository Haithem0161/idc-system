//! Phase-06 §5 canonical persona script: **P2 Mehdi the Receptionist**.
//!
//! A 10-step end-to-end walk through every IPC surface phase-06 ships.
//! The day-script:
//!  1. Mehdi opens the clinic, navigates to `/inventory`; the list paints
//!     with status pills for the seeded stock.
//!  2. Mehdi receives a delivery (`receive` qty=20) on Lidocaine.
//!  3. He browses the item detail page (Overview + Adjustments).
//!  4. Two more lidocaine consumes during morning visits (manual via the
//!     domain repo to mirror the visits-lock writer from phase-05).
//!  5. A pack arrives damaged -- writeoff qty=3.
//!  6. The pill flips to LOW; Mehdi confirms the visual signal.
//!  7. A receptionist tries `count_correction` -- blocked.
//!  8. Mariam (superadmin) applies the count_correction (-1, audit).
//!  9. Mariam runs the recompute_on_hand debug command; on-hand matches
//!     the SUM of live adjustments.
//! 10. End-of-day audit log carries the full mutation trail.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::catalog::domain::entities::inventory_item::InventoryItemNewInput;
use app_lib::domains::catalog::domain::entities::InventoryItem;
use app_lib::domains::catalog::domain::repositories::{
    InventoryConsumptionRepo, InventoryItemRepo,
};
use app_lib::domains::catalog::infrastructure::{
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo,
};
use app_lib::domains::inventory::service::{
    AdjustmentInput, InventoryAdjustmentService, InventoryAdjustmentServiceConfig, StockStatus,
};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::domains::visits::domain::entities::AdjustmentReason;
use app_lib::domains::visits::domain::repositories::InventoryAdjustmentRepo;
use app_lib::domains::visits::infrastructure::SqliteInventoryAdjustmentRepo;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-clinic";
const DEVICE_ID: &str = "dev-clinic";

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
async fn persona_p2_mehdi_walks_through_phase06_inventory_day() {
    // ---- Step 1: bootstrap ------------------------------------------------
    let pool = fresh_pool().await;
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let items_repo: Arc<dyn InventoryItemRepo> =
        Arc::new(SqliteInventoryItemRepo::new(pool.clone()));
    let consumption_repo: Arc<dyn InventoryConsumptionRepo> =
        Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone()));
    let adj_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(pool.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));

    let mehdi = User::try_new(
        "mehdi@idc.io",
        "Mehdi",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mariam = User::try_new(
        "mariam@idc.io",
        "Mariam",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &mehdi).await.unwrap();
    user_repo.upsert(&mut tx, &mariam).await.unwrap();
    tx.commit().await.unwrap();

    let lidocaine = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "ليدوكائين".into(),
        name_en: Some("Lidocaine".into()),
        unit: "vials".into(),
        low_stock_threshold: 10,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    items_repo.upsert(&mut tx, &lidocaine).await.unwrap();
    tx.commit().await.unwrap();

    let service = Arc::new(InventoryAdjustmentService::new(
        InventoryAdjustmentServiceConfig {
            pool: pool.clone(),
            items_repo: items_repo.clone(),
            consumption_repo,
            adjustments_repo: adj_repo.clone(),
            audit_repo,
            outbox_repo,
            device_id: DEVICE_ID.into(),
        },
    ));

    // List paints initially: Lidocaine on-hand=0, threshold=10 -> LOW pill.
    let initial = service
        .list_items(ENTITY_ID, None, false, None)
        .await
        .unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].status, StockStatus::Low);

    // ---- Step 2: Receive a delivery (qty=20) ------------------------------
    let receive_op = service
        .create(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: lidocaine.id,
                reason: AdjustmentReason::Receive,
                delta: 20,
                note: Some("supplier delivery monday".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(receive_op.delta, 20);

    // ---- Step 3: Browse item detail; expect on-hand=20, status=OK ---------
    let detail = service.get_item(ENTITY_ID, lidocaine.id).await.unwrap();
    assert_eq!(detail.item.quantity_on_hand, 20);
    assert_eq!(detail.status, StockStatus::Ok);
    assert_eq!(detail.recent_adjustments.len(), 1);

    // ---- Step 4: Two consume_visit adjustments (-3, -2) -------------------
    // Mirror what `Visit::lock` would emit when consuming Lidocaine. The FK
    // on inventory_adjustments.visit_id requires real visits rows; we seed
    // minimal `voided`-status placeholders that satisfy every NOT NULL
    // constraint in the visits table (FK targets only -- not exercised by
    // this binary's behavioural assertions).
    let visit_ids = [Uuid::now_v7(), Uuid::now_v7()];
    for v in &visit_ids {
        insert_dummy_visit(&pool, mehdi.id, *v).await;
    }
    let mut tx = pool.begin().await.unwrap();
    for (delta_neg, visit_uuid) in visit_ids
        .iter()
        .enumerate()
        .map(|(i, v)| (if i == 0 { -3i64 } else { -2 }, *v))
    {
        let consume = app_lib::domains::visits::domain::entities::InventoryAdjustment::try_new(
            app_lib::domains::visits::domain::entities::AdjustmentNewInput {
                item_id: lidocaine.id,
                delta: delta_neg,
                reason: AdjustmentReason::ConsumeVisit,
                visit_id: Some(visit_uuid),
                note: Some(format!("consume on lock of visit {visit_uuid}")),
                by_user_id: mehdi.id,
                entity_id: ENTITY_ID.into(),
                origin_device_id: Some(DEVICE_ID.into()),
            },
        )
        .unwrap();
        adj_repo.append(&mut tx, &consume).await.unwrap();
    }
    adj_repo
        .recompute_item_quantity(&mut tx, lidocaine.id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(lidocaine.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, 15, "20 - 3 - 2 = 15");

    // ---- Step 5: A pack arrives damaged; writeoff qty=3 -------------------
    let writeoff = service
        .create(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: lidocaine.id,
                reason: AdjustmentReason::Writeoff,
                delta: 3,
                note: Some("damaged on arrival".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(writeoff.delta, -3);

    // ---- Step 6: After writeoff on-hand=12; threshold=10 -> still OK ------
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(lidocaine.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, 12);
    let detail = service.get_item(ENTITY_ID, lidocaine.id).await.unwrap();
    assert_eq!(detail.status, StockStatus::Ok);
    // Now a follow-up writeoff of 3 puts it at 9 -- which DOES flip LOW.
    service
        .create(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: lidocaine.id,
                reason: AdjustmentReason::Writeoff,
                delta: 3,
                note: None,
            },
        )
        .await
        .unwrap();
    let detail = service.get_item(ENTITY_ID, lidocaine.id).await.unwrap();
    assert_eq!(
        detail.status,
        StockStatus::Low,
        "9 <= threshold(10) -> LOW pill"
    );

    // ---- Step 7: Mehdi (receptionist) tries count_correction -- blocked ---
    let err = service
        .create(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: lidocaine.id,
                reason: AdjustmentReason::CountCorrection,
                delta: -1,
                note: None,
            },
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("Superadmin"));

    // ---- Step 8: Mariam (superadmin) applies count_correction -1 ----------
    service
        .create(
            mariam.id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: lidocaine.id,
                reason: AdjustmentReason::CountCorrection,
                delta: -1,
                note: Some("physical count off by one".into()),
            },
        )
        .await
        .unwrap();
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(lidocaine.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, 8, "after count_correction -1: 9 - 1 = 8");

    // ---- Step 9: Recompute matches SUM of live adjustments ----------------
    let n = service
        .recompute_on_hand(mariam.id, UserRole::Superadmin, ENTITY_ID, lidocaine.id)
        .await
        .unwrap();
    assert_eq!(n, 8);

    // ---- Step 10: end-of-day audit trail completeness ---------------------
    let item_audits: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'inventory_items' AND entity_id = ?",
    )
    .bind(lidocaine.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    // Expected updates: receive, writeoff, writeoff, count_correction,
    // recompute -- 5 updates total. (Two consume_visit writes bypass the
    // service-layer audit but write their own audit row via the lock
    // workflow in phase-05; this binary skips that path so we count just
    // the service-driven mutations.)
    assert!(item_audits.0 >= 5);
    let adj_audits: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE entity = 'inventory_adjustments'")
            .fetch_one(&pool)
            .await
            .unwrap();
    // 4 service-driven adjustments (receive, writeoff, writeoff,
    // count_correction). The two raw-inserted consume rows do NOT get an
    // audit row from this binary (they would in production via the lock
    // workflow which is phase-05 territory).
    assert!(adj_audits.0 >= 4);

    // The outbox shows the same data is queued for the sync engine.
    let outbox_total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        outbox_total.0 > 0,
        "the day's mutations must be queued for sync"
    );
}

async fn insert_dummy_visit(pool: &SqlitePool, user_id: Uuid, visit_id: Uuid) {
    // Mirrors the minimal-visit helper in `inventory_phase06.rs`. Seeds a
    // `voided`-status visit + a placeholder patient + check_type so the FK
    // chain on a `consume_visit` adjustment can resolve. The persona binary
    // never reads these rows back beyond their existence.
    let check_type_id = Uuid::now_v7();
    let patient_id = Uuid::now_v7();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO patients (id, name, created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, 'p', ?, ?, 1, 1, ?)",
    )
    .bind(patient_id.to_string())
    .bind(&now)
    .bind(&now)
    .bind(ENTITY_ID)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO check_types (id, name_ar, has_subtypes, base_price_iqd, dye_supported, \
         sort_order, is_active, created_at, updated_at, version, dirty, \
         entity_id) \
         VALUES (?, 'ct', 0, 10000, 0, 0, 1, ?, ?, 1, 1, ?)",
    )
    .bind(check_type_id.to_string())
    .bind(&now)
    .bind(&now)
    .bind(ENTITY_ID)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO visits ( \
            id, patient_id, status, receptionist_user_id, check_type_id, \
            dye, report, locked_at, voided_at, voided_by_user_id, void_reason, \
            patient_name_snapshot, check_type_name_ar_snapshot, \
            price_snapshot_iqd, dye_cost_snapshot_iqd, report_amount_snapshot_iqd, \
            doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, \
            internal_pct_snapshot, total_amount_iqd_snapshot, \
            operator_id, \
            created_at, updated_at, version, dirty, entity_id \
         ) VALUES (?,?,'voided',?,?,0,0,?,?,?,?,'p','ct',0,0,0,0,0,40,0,NULL,?,?,1,1,?)",
    )
    .bind(visit_id.to_string())
    .bind(patient_id.to_string())
    .bind(user_id.to_string())
    .bind(check_type_id.to_string())
    .bind(&now)
    .bind(&now)
    .bind(user_id.to_string())
    .bind("voided for persona test")
    .bind(&now)
    .bind(&now)
    .bind(ENTITY_ID)
    .execute(pool)
    .await
    .unwrap();
}
