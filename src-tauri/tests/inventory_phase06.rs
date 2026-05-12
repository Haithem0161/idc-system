//! Integration tests for Phase 6 inventory operations.
//!
//! Drives `InventoryAdjustmentService` end-to-end against an in-memory SQLite
//! with all migrations applied. Covers:
//! - per-reason delta-sign validation (receive > 0, writeoff < 0,
//!   count_correction != 0)
//! - role gates (receptionist for receive/writeoff, superadmin-only for
//!   count_correction, consume_visit rejected at the IPC boundary)
//! - audit-first ordering (two audit rows per adjustment: one on
//!   `inventory_adjustments` create, one on the `inventory_items` update)
//! - quantity recompute correctness across receive + writeoff +
//!   count_correction sequences
//! - voided-visit reversal rendering (positive consume_visit row in list)
//! - outbox enqueue: one row for the adjustment + one for the affected item

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
    CatalogListFilter, InventoryConsumptionRepo, InventoryItemRepo,
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

const ENTITY_ID: &str = "tenant-i";
const DEVICE_ID: &str = "dev-i";

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
    service: Arc<InventoryAdjustmentService>,
    items_repo: Arc<dyn InventoryItemRepo>,
    adjustments_repo: Arc<dyn InventoryAdjustmentRepo>,
    item: InventoryItem,
    actor_user_id: Uuid,
}

async fn seed_one_item(low_threshold: i64) -> Fixture {
    let pool = fresh_pool().await;
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let items_repo: Arc<dyn InventoryItemRepo> =
        Arc::new(SqliteInventoryItemRepo::new(pool.clone()));
    let consumption_repo: Arc<dyn InventoryConsumptionRepo> =
        Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone()));
    let adjustments_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(pool.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));

    // Seed a user so the FK on `inventory_adjustments.by_user_id` is satisfied.
    let actor = User::try_new(
        "ops@example.com",
        "Ops",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &actor).await.unwrap();
    tx.commit().await.unwrap();

    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "صنف".into(),
        name_en: Some("Widget".into()),
        unit: "pcs".into(),
        low_stock_threshold: low_threshold,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    items_repo.upsert(&mut tx, &item).await.unwrap();
    tx.commit().await.unwrap();

    let service = Arc::new(InventoryAdjustmentService::new(
        InventoryAdjustmentServiceConfig {
            pool: pool.clone(),
            items_repo: items_repo.clone(),
            consumption_repo,
            adjustments_repo: adjustments_repo.clone(),
            audit_repo,
            outbox_repo,
            device_id: DEVICE_ID.to_string(),
        },
    ));

    Fixture {
        pool,
        service,
        items_repo,
        adjustments_repo,
        item,
        actor_user_id: actor.id,
    }
}

async fn item_on_hand(pool: &SqlitePool, item_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(item_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

async fn count_audit_rows(pool: &SqlitePool, entity: &str, entity_id: &str) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE entity = ? AND entity_id = ?")
            .bind(entity)
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

async fn count_outbox_rows(pool: &SqlitePool, entity: &str, entity_id: &str) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = ? AND entity_id = ?")
            .bind(entity)
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

#[tokio::test]
async fn receive_increases_on_hand_and_writes_two_audit_rows() {
    let f = seed_one_item(5).await;
    let adj = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: 12,
                note: Some("box from supplier".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(adj.delta, 12);
    assert_eq!(adj.reason, AdjustmentReason::Receive);

    assert_eq!(item_on_hand(&f.pool, f.item.id).await, 12);

    // Two audit rows: one for the adjustment create, one for the item update.
    assert_eq!(
        count_audit_rows(&f.pool, "inventory_adjustments", &adj.id.to_string()).await,
        1
    );
    assert_eq!(
        count_audit_rows(&f.pool, "inventory_items", &f.item.id.to_string()).await,
        1
    );

    // Outbox: one for the adjustment, one for the item.
    assert_eq!(
        count_outbox_rows(&f.pool, "inventory_adjustments", &adj.id.to_string()).await,
        1
    );
    assert_eq!(
        count_outbox_rows(&f.pool, "inventory_items", &f.item.id.to_string()).await,
        1
    );
}

#[tokio::test]
async fn writeoff_stores_negative_delta_and_decreases_on_hand() {
    let f = seed_one_item(0).await;
    // Receive 10 first.
    f.service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: 10,
                note: None,
            },
        )
        .await
        .unwrap();
    // Writeoff 3 (UI submits positive 3; service stores -3).
    let wo = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Writeoff,
                delta: 3,
                note: Some("damaged".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(wo.delta, -3);
    assert_eq!(item_on_hand(&f.pool, f.item.id).await, 7);
}

#[tokio::test]
async fn count_correction_requires_superadmin() {
    let f = seed_one_item(0).await;
    let err = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: -5,
                note: None,
            },
        )
        .await
        .expect_err("receptionist must be rejected");
    assert!(format!("{}", err).contains("Superadmin"));
}

#[tokio::test]
async fn count_correction_superadmin_signed_delta_succeeds() {
    let f = seed_one_item(0).await;
    let adj = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: 4,
                note: Some("annual count".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(adj.delta, 4);
    assert_eq!(item_on_hand(&f.pool, f.item.id).await, 4);

    // Negative signed delta also works.
    let neg = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: -2,
                note: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(neg.delta, -2);
    assert_eq!(item_on_hand(&f.pool, f.item.id).await, 2);
}

#[tokio::test]
async fn count_correction_zero_is_rejected() {
    let f = seed_one_item(0).await;
    let err = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: 0,
                note: None,
            },
        )
        .await
        .expect_err("zero delta must be rejected");
    assert!(
        format!("{}", err).contains("non-zero"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn receive_with_non_positive_delta_is_rejected() {
    let f = seed_one_item(0).await;
    let zero = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: 0,
                note: None,
            },
        )
        .await
        .expect_err("receive 0 must reject");
    assert!(format!("{}", zero).contains("positive"));

    let neg = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: -1,
                note: None,
            },
        )
        .await
        .expect_err("receive -1 must reject");
    assert!(format!("{}", neg).contains("positive"));
}

#[tokio::test]
async fn writeoff_with_non_positive_qty_is_rejected() {
    let f = seed_one_item(0).await;
    let err = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Writeoff,
                delta: 0,
                note: None,
            },
        )
        .await
        .expect_err("writeoff 0 must reject");
    assert!(format!("{}", err).contains("positive"));
}

#[tokio::test]
async fn consume_visit_is_not_emitted_by_ipc() {
    let f = seed_one_item(0).await;
    let err = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::ConsumeVisit,
                delta: -1,
                note: None,
            },
        )
        .await
        .expect_err("consume_visit must be rejected by the IPC service");
    assert!(format!("{}", err).contains("lock workflow"));
}

#[tokio::test]
async fn recompute_after_mixed_sequence_matches_sum() {
    let f = seed_one_item(3).await;
    for delta in [10, 5, -2] {
        let reason = if delta > 0 {
            AdjustmentReason::Receive
        } else {
            AdjustmentReason::Writeoff
        };
        let raw = if delta < 0 { -delta } else { delta };
        f.service
            .create(
                f.actor_user_id,
                UserRole::Superadmin,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: f.item.id,
                    reason,
                    delta: raw,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    // 10 + 5 - 2 = 13.
    assert_eq!(item_on_hand(&f.pool, f.item.id).await, 13);

    // Debug recompute is a no-op on consistent data but should return 13.
    let n = f
        .service
        .recompute_on_hand(f.actor_user_id, UserRole::Superadmin, ENTITY_ID, f.item.id)
        .await
        .unwrap();
    assert_eq!(n, 13);
}

#[tokio::test]
async fn recompute_requires_superadmin() {
    let f = seed_one_item(0).await;
    let err = f
        .service
        .recompute_on_hand(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            f.item.id,
        )
        .await
        .expect_err("receptionist must be rejected");
    assert!(format!("{}", err).contains("Superadmin"));
}

#[tokio::test]
async fn list_items_status_filter_returns_low_and_neg() {
    let f = seed_one_item(5).await;
    // Receive 3 (still <= threshold of 5 -> low).
    f.service
        .create(
            f.actor_user_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: 3,
                note: None,
            },
        )
        .await
        .unwrap();
    let low_items = f
        .service
        .list_items(ENTITY_ID, Some(StockStatus::Low), false, None)
        .await
        .unwrap();
    assert_eq!(low_items.len(), 1);
    assert_eq!(low_items[0].status, StockStatus::Low);

    // Writeoff 10 -> 3 - 10 = -7 (NEG even though over-consumption).
    f.service
        .create(
            f.actor_user_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Writeoff,
                delta: 10,
                note: None,
            },
        )
        .await
        .unwrap();
    let neg_items = f
        .service
        .list_items(ENTITY_ID, Some(StockStatus::Neg), false, None)
        .await
        .unwrap();
    assert_eq!(neg_items.len(), 1);
    assert_eq!(neg_items[0].status, StockStatus::Neg);
    assert_eq!(item_on_hand(&f.pool, f.item.id).await, -7);
}

#[tokio::test]
async fn get_item_returns_consumption_map_and_adjustments() {
    let f = seed_one_item(0).await;
    f.service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: 7,
                note: None,
            },
        )
        .await
        .unwrap();
    let detail = f.service.get_item(ENTITY_ID, f.item.id).await.unwrap();
    assert_eq!(detail.item.quantity_on_hand, 7);
    assert_eq!(detail.recent_adjustments.len(), 1);
    assert_eq!(detail.status, StockStatus::Ok);
}

#[tokio::test]
async fn cannot_adjust_item_from_another_tenant() {
    let f = seed_one_item(0).await;
    let other_tenant = "tenant-other";
    let err = f
        .service
        .create(
            f.actor_user_id,
            UserRole::Receptionist,
            other_tenant,
            AdjustmentInput {
                item_id: f.item.id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .expect_err("cross-tenant must be rejected");
    assert!(format!("{}", err).to_lowercase().contains("not found"));
}

async fn insert_dummy_visit(pool: &SqlitePool, user_id: Uuid) -> Uuid {
    // Insert a minimal `visits` row in `voided` status so a consume_visit
    // adjustment can carry its FK to it. Bypasses VisitService to keep the
    // test focused on the inventory side.
    let visit_id = Uuid::now_v7();
    let check_type_id = Uuid::now_v7();
    let patient_id = Uuid::now_v7();
    // Patient + check_type FKs need a row too.
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
         report_supported, sort_order, is_active, created_at, updated_at, version, dirty, \
         entity_id) \
         VALUES (?, 'ct', 0, 10000, 0, 0, 0, 1, ?, ?, 1, 1, ?)",
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
            price_snapshot_iqd, dye_cost_snapshot_iqd, report_cost_snapshot_iqd, \
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
    .bind("voided for test")
    .bind(&now)
    .bind(&now)
    .bind(ENTITY_ID)
    .execute(pool)
    .await
    .unwrap();
    visit_id
}

#[tokio::test]
async fn voided_visit_offset_renders_as_positive_consume_visit_row() {
    let f = seed_one_item(0).await;
    // Build a manual consume_visit (negative) + manual offset (positive) by
    // touching the repo directly. This mirrors the void path: VisitService
    // emits these inline and `<ItemAdjustmentsList>` distinguishes them via
    // the `is_reversal` flag on the DTO.
    let visit_id = insert_dummy_visit(&f.pool, f.actor_user_id).await;
    let consume = app_lib::domains::visits::domain::entities::InventoryAdjustment::try_new(
        app_lib::domains::visits::domain::entities::AdjustmentNewInput {
            item_id: f.item.id,
            delta: -3,
            reason: AdjustmentReason::ConsumeVisit,
            visit_id: Some(visit_id),
            note: Some(format!("consume on lock of visit {}", visit_id)),
            by_user_id: f.actor_user_id,
            entity_id: ENTITY_ID.into(),
            origin_device_id: Some(DEVICE_ID.into()),
        },
    )
    .unwrap();
    let offset = app_lib::domains::visits::domain::entities::InventoryAdjustment::try_new(
        app_lib::domains::visits::domain::entities::AdjustmentNewInput {
            item_id: f.item.id,
            delta: 3,
            reason: AdjustmentReason::ConsumeVisit,
            visit_id: Some(visit_id),
            note: Some(format!("void offset of {}", consume.id)),
            by_user_id: f.actor_user_id,
            entity_id: ENTITY_ID.into(),
            origin_device_id: Some(DEVICE_ID.into()),
        },
    )
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    f.adjustments_repo.append(&mut tx, &consume).await.unwrap();
    f.adjustments_repo.append(&mut tx, &offset).await.unwrap();
    f.adjustments_repo
        .recompute_item_quantity(&mut tx, f.item.id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let rows = f
        .service
        .list_adjustments(ENTITY_ID, f.item.id, 50)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    let has_reversal = rows
        .iter()
        .any(|r| matches!(r.reason, AdjustmentReason::ConsumeVisit) && r.delta > 0);
    assert!(has_reversal, "expected positive consume_visit reversal row");
    // Net should be zero.
    assert_eq!(item_on_hand(&f.pool, f.item.id).await, 0);
}

#[tokio::test]
async fn list_items_query_filter_is_substring_insensitive() {
    let f = seed_one_item(0).await;
    let filtered = f
        .service
        .list_items(ENTITY_ID, None, false, Some("Widget".into()))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    let other = f
        .service
        .list_items(ENTITY_ID, None, false, Some("zzz".into()))
        .await
        .unwrap();
    assert!(other.is_empty());
}

#[tokio::test]
async fn count_correction_nonzero_trigger_blocks_direct_zero_insert() {
    // Defense-in-depth: even if the service layer were bypassed, the
    // migration 006 trigger should still reject a count_correction with
    // delta == 0.
    let f = seed_one_item(0).await;
    let res = sqlx::query(
        "INSERT INTO inventory_adjustments \
         (id, item_id, delta, reason, visit_id, note, by_user_id, \
          created_at, updated_at, deleted_at, version, dirty, \
          last_synced_at, origin_device_id, entity_id) \
         VALUES (?,?,0,'count_correction',NULL,NULL,?, \
                 datetime('now'),datetime('now'),NULL,1,1,NULL,?,?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(f.item.id.to_string())
    .bind(f.actor_user_id.to_string())
    .bind(DEVICE_ID)
    .bind(ENTITY_ID)
    .execute(&f.pool)
    .await;
    assert!(res.is_err(), "trigger should block zero-delta insert");

    // Sanity: the catalog list filter only returns the seeded row.
    let items = f
        .items_repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: false,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
}
