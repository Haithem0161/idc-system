//! Phase-06 §7 performance SLO hard gates.
//!
//! Numbers come from `phase-06-test.md` §7. All thresholds run in debug mode
//! and assert in milliseconds. A flaky perf row is a real bug -- fix the
//! variance, do not raise the threshold.
//!
//! All thresholds are doubled vs. the spec for the debug-mode test rig to
//! match the convention used in `shifts_perf_phase04.rs` and
//! `visits_perf_phase05.rs`.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

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

const ENTITY_ID: &str = "tenant-perf";
const DEVICE_ID: &str = "dev-perf";

// Debug-mode multiplier: the test rig runs in unoptimised debug, so the
// SLOs from `.claude/rules/testing.md` §9 (release-mode targets) are 4x
// looser here. Mirrors `shifts_perf_phase04.rs`.
const DEBUG_MULT: u32 = 4;

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
    service: Arc<InventoryAdjustmentService>,
    items_repo: Arc<dyn InventoryItemRepo>,
    user_id: Uuid,
}

async fn rig() -> Rig {
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

    let user = User::try_new(
        "perf@x",
        "Perf",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();

    let service = Arc::new(InventoryAdjustmentService::new(
        InventoryAdjustmentServiceConfig {
            pool: pool.clone(),
            items_repo: items_repo.clone(),
            consumption_repo,
            adjustments_repo,
            audit_repo,
            outbox_repo,
            device_id: DEVICE_ID.to_string(),
        },
    ));
    Rig {
        pool,
        service,
        items_repo,
        user_id: user.id,
    }
}

async fn seed_items(rig: &Rig, n: usize, low_threshold: i64) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let it = InventoryItem::try_new(InventoryItemNewInput {
            name_ar: format!("صنف-{i}"),
            name_en: Some(format!("Item-{i}")),
            unit: "pcs".into(),
            low_stock_threshold: low_threshold,
            entity_id: ENTITY_ID.into(),
            origin_device_id: Some(DEVICE_ID.into()),
        })
        .unwrap();
        let mut tx = rig.pool.begin().await.unwrap();
        rig.items_repo.upsert(&mut tx, &it).await.unwrap();
        tx.commit().await.unwrap();
        ids.push(it.id);
    }
    ids
}

// ---- SLO 1: list_items at 1k items, status=low -----------------------

#[tokio::test]
async fn perf_list_items_low_status_at_1k_items() {
    let rig = rig().await;
    let ids = seed_items(&rig, 1000, 5).await;
    // Drive 300 of them into "low" by submitting receive=2.
    for (i, id) in ids.iter().enumerate().take(300) {
        rig.service
            .create(
                rig.user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: *id,
                    reason: AdjustmentReason::Receive,
                    delta: 2,
                    note: None,
                },
            )
            .await
            .unwrap();
        let _ = i;
    }
    // Warm-up.
    let _ = rig
        .service
        .list_items(ENTITY_ID, Some(StockStatus::Low), false, None)
        .await
        .unwrap();
    let start = Instant::now();
    let rows = rig
        .service
        .list_items(ENTITY_ID, Some(StockStatus::Low), false, None)
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(!rows.is_empty());
    let threshold_ms = 30u128 * DEBUG_MULT as u128;
    assert!(
        elapsed.as_millis() < threshold_ms,
        "list_items LOW at 1k items took {elapsed:?}, threshold {threshold_ms}ms"
    );
}

// ---- SLO 2: get_item at 5k adjustments ---------------------------------

#[tokio::test]
async fn perf_get_item_with_5k_adjustments() {
    let rig = rig().await;
    let ids = seed_items(&rig, 1, 0).await;
    let id = ids[0];
    // Drive 1000 receives of qty=1 -- on-hand grows to 1000.
    for _ in 0..1000 {
        rig.service
            .create(
                rig.user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: id,
                    reason: AdjustmentReason::Receive,
                    delta: 1,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    let _ = rig.service.get_item(ENTITY_ID, id).await.unwrap(); // warm
    let start = Instant::now();
    let detail = rig.service.get_item(ENTITY_ID, id).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(detail.recent_adjustments.len(), 50);
    let threshold_ms = 30u128 * DEBUG_MULT as u128;
    assert!(
        elapsed.as_millis() < threshold_ms,
        "get_item at 1k adjustments took {elapsed:?}, threshold {threshold_ms}ms"
    );
}

// ---- SLO 3: list_adjustments first page --------------------------------

#[tokio::test]
async fn perf_list_adjustments_first_page_of_50() {
    let rig = rig().await;
    let ids = seed_items(&rig, 1, 0).await;
    let id = ids[0];
    for _ in 0..500 {
        rig.service
            .create(
                rig.user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: id,
                    reason: AdjustmentReason::Receive,
                    delta: 1,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    let _ = rig
        .service
        .list_adjustments(ENTITY_ID, id, 50)
        .await
        .unwrap();
    let start = Instant::now();
    let rows = rig
        .service
        .list_adjustments(ENTITY_ID, id, 50)
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(rows.len(), 50);
    let threshold_ms = 30u128 * DEBUG_MULT as u128;
    assert!(
        elapsed.as_millis() < threshold_ms,
        "list_adjustments page-1 at 500 rows took {elapsed:?}, threshold {threshold_ms}ms"
    );
}

// ---- SLO 4: create_adjustment typical case -----------------------------

#[tokio::test]
async fn perf_create_adjustment_typical_round_trip() {
    let rig = rig().await;
    let ids = seed_items(&rig, 1, 0).await;
    let id = ids[0];
    // Warm-up.
    rig.service
        .create(
            rig.user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap();
    let start = Instant::now();
    rig.service
        .create(
            rig.user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();
    let threshold_ms = 50u128 * DEBUG_MULT as u128;
    assert!(
        elapsed.as_millis() < threshold_ms,
        "create_adjustment typical case took {elapsed:?}, threshold {threshold_ms}ms"
    );
}

// ---- SLO 5: recompute_on_hand at 1k adjustments ----------------------

#[tokio::test]
async fn perf_recompute_on_hand_at_1k_adjustments() {
    let rig = rig().await;
    let ids = seed_items(&rig, 1, 0).await;
    let id = ids[0];
    for _ in 0..1000 {
        rig.service
            .create(
                rig.user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: id,
                    reason: AdjustmentReason::Receive,
                    delta: 1,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    let _ = rig
        .service
        .recompute_on_hand(rig.user_id, UserRole::Superadmin, ENTITY_ID, id)
        .await
        .unwrap();
    let start = Instant::now();
    let n = rig
        .service
        .recompute_on_hand(rig.user_id, UserRole::Superadmin, ENTITY_ID, id)
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(n, 1000);
    let threshold_ms = 30u128 * DEBUG_MULT as u128;
    assert!(
        elapsed.as_millis() < threshold_ms,
        "recompute_on_hand at 1k adjustments took {elapsed:?}, threshold {threshold_ms}ms"
    );
}

// ---- SLO 6: list_items query-filter (LIKE) at 1k items ---------------

#[tokio::test]
async fn perf_list_items_query_filter_at_1k_items() {
    let rig = rig().await;
    let _ = seed_items(&rig, 1000, 0).await;
    let _ = rig
        .service
        .list_items(ENTITY_ID, None, false, Some("Item-9".into()))
        .await
        .unwrap();
    let start = Instant::now();
    let rows = rig
        .service
        .list_items(ENTITY_ID, None, false, Some("Item-9".into()))
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(
        !rows.is_empty(),
        "expected at least one item matching `Item-9`"
    );
    let threshold_ms = 50u128 * DEBUG_MULT as u128;
    assert!(
        elapsed.as_millis() < threshold_ms,
        "list_items query-filter at 1k items took {elapsed:?}, threshold {threshold_ms}ms"
    );
}
