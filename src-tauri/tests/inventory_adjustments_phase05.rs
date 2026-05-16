//! Phase 05 inventory_adjustments integration tests.
//!
//! Exercises the SQLite-layer immutability trigger, the append-only
//! repository, the CHECK constraints from migration 005, the FTS-free
//! chrono index, and the recompute path that updates `inventory_items`.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::catalog::domain::entities::inventory_item::InventoryItemNewInput;
use app_lib::domains::catalog::domain::entities::InventoryItem;
use app_lib::domains::catalog::domain::repositories::InventoryItemRepo;
use app_lib::domains::catalog::infrastructure::SqliteInventoryItemRepo;
use app_lib::domains::visits::domain::entities::{
    AdjustmentNewInput, AdjustmentReason, InventoryAdjustment,
};
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
    repo: Arc<dyn InventoryAdjustmentRepo>,
    item: InventoryItem,
    user: User,
}

async fn seed() -> Fixture {
    let pool = fresh_pool().await;
    let item_repo: Arc<dyn InventoryItemRepo> =
        Arc::new(SqliteInventoryItemRepo::new(pool.clone()));
    let adj_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(pool.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));

    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "غاز".into(),
        name_en: Some("Gas".into()),
        unit: "u".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let user = User::try_new(
        "u@x",
        "U",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    item_repo.upsert(&mut tx, &item).await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();

    Fixture {
        pool,
        repo: adj_repo,
        item,
        user,
    }
}

fn adj(
    item_id: Uuid,
    by: Uuid,
    delta: i64,
    reason: AdjustmentReason,
    visit: Option<Uuid>,
) -> InventoryAdjustment {
    InventoryAdjustment::try_new(AdjustmentNewInput {
        item_id,
        delta,
        reason,
        visit_id: visit,
        note: None,
        by_user_id: by,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap()
}

#[tokio::test]
async fn append_persists_row_with_dirty_flag() {
    let f = seed().await;
    let row = adj(f.item.id, f.user.id, 10, AdjustmentReason::Receive, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &row).await.unwrap();
    tx.commit().await.unwrap();

    let q: (i64,) = sqlx::query_as("SELECT dirty FROM inventory_adjustments WHERE id = ?")
        .bind(row.id.to_string())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(q.0, 1);
}

#[tokio::test]
async fn check_constraint_blocks_consume_without_visit_id_at_raw_sql_layer() {
    let f = seed().await;
    // Raw insert that violates the CHECK (reason='consume_visit' OR visit_id IS NOT NULL).
    let res = sqlx::query(
        "INSERT INTO inventory_adjustments \
         (id, item_id, delta, reason, visit_id, note, by_user_id, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id) \
         VALUES (?, ?, ?, 'consume_visit', NULL, NULL, ?, ?, ?, NULL, 0, 1, NULL, NULL, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(f.item.id.to_string())
    .bind(-2_i64)
    .bind(f.user.id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(ENTITY_ID)
    .execute(&f.pool)
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn check_constraint_blocks_receive_with_non_positive_delta() {
    let f = seed().await;
    let res = sqlx::query(
        "INSERT INTO inventory_adjustments \
         (id, item_id, delta, reason, visit_id, note, by_user_id, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id) \
         VALUES (?, ?, 0, 'receive', NULL, NULL, ?, ?, ?, NULL, 0, 1, NULL, NULL, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(f.item.id.to_string())
    .bind(f.user.id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(ENTITY_ID)
    .execute(&f.pool)
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn check_constraint_blocks_writeoff_with_non_negative_delta() {
    let f = seed().await;
    let res = sqlx::query(
        "INSERT INTO inventory_adjustments \
         (id, item_id, delta, reason, visit_id, note, by_user_id, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id) \
         VALUES (?, ?, 5, 'writeoff', NULL, NULL, ?, ?, ?, NULL, 0, 1, NULL, NULL, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(f.item.id.to_string())
    .bind(f.user.id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(ENTITY_ID)
    .execute(&f.pool)
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn business_field_update_blocked_by_trigger() {
    let f = seed().await;
    let row = adj(f.item.id, f.user.id, 7, AdjustmentReason::Receive, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &row).await.unwrap();
    tx.commit().await.unwrap();

    for column in ["delta = 1", "reason = 'writeoff'", "by_user_id = ?"] {
        let q = format!("UPDATE inventory_adjustments SET {} WHERE id = ?", column);
        let mut builder = sqlx::query(&q);
        if column.contains("by_user_id") {
            builder = builder.bind(Uuid::now_v7().to_string());
        }
        let res = builder.bind(row.id.to_string()).execute(&f.pool).await;
        assert!(res.is_err(), "expected ABORT updating {}", column);
    }
}

#[tokio::test]
async fn sync_metadata_update_allowed_via_trigger_carve_out() {
    let f = seed().await;
    let row = adj(f.item.id, f.user.id, 7, AdjustmentReason::Receive, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &row).await.unwrap();
    tx.commit().await.unwrap();

    let res = sqlx::query(
        "UPDATE inventory_adjustments SET dirty = 0, version = version + 1, last_synced_at = ?, origin_device_id = 'other' WHERE id = ?",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(row.id.to_string())
    .execute(&f.pool)
    .await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn recompute_item_quantity_sums_across_non_deleted_rows() {
    let f = seed().await;
    let r1 = adj(f.item.id, f.user.id, 5, AdjustmentReason::Receive, None);
    let r2 = adj(f.item.id, f.user.id, 3, AdjustmentReason::Receive, None);
    let r3 = adj(f.item.id, f.user.id, -2, AdjustmentReason::Writeoff, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &r1).await.unwrap();
    f.repo.append(&mut tx, &r2).await.unwrap();
    f.repo.append(&mut tx, &r3).await.unwrap();
    let total = f
        .repo
        .recompute_item_quantity(&mut tx, f.item.id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(total, 6);

    let q: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(f.item.id.to_string())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(q.0, 6);
}

#[tokio::test]
async fn list_consume_for_visit_returns_only_consume_rows_for_visit() {
    let f = seed().await;
    let receive = adj(f.item.id, f.user.id, 10, AdjustmentReason::Receive, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &receive).await.unwrap();
    tx.commit().await.unwrap();
    // The visit_id field is nullable when not consume_visit; for a real
    // consume_visit row the caller upstream provides a valid visit row;
    // this test validates only the negative branch (visit_id None case).
    let rows = f.repo.list_consume_for_visit(Uuid::now_v7()).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn list_by_item_returns_appended_rows_in_chrono_order() {
    let f = seed().await;
    let one = adj(f.item.id, f.user.id, 4, AdjustmentReason::Receive, None);
    let two = adj(f.item.id, f.user.id, 6, AdjustmentReason::Receive, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &one).await.unwrap();
    f.repo.append(&mut tx, &two).await.unwrap();
    tx.commit().await.unwrap();
    let rows = f.repo.list_by_item(ENTITY_ID, f.item.id, 50).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].delta + rows[1].delta, 10);
}

#[tokio::test]
async fn trigger_blocks_setting_deleted_at_from_null_to_non_null() {
    let f = seed().await;
    let row = adj(f.item.id, f.user.id, 7, AdjustmentReason::Receive, None);
    let mut tx = f.pool.begin().await.unwrap();
    f.repo.append(&mut tx, &row).await.unwrap();
    tx.commit().await.unwrap();
    let res = sqlx::query("UPDATE inventory_adjustments SET deleted_at = ? WHERE id = ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(row.id.to_string())
        .execute(&f.pool)
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn adjustments_chrono_index_present_in_sqlite_master() {
    let pool = fresh_pool().await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'inventory_adjustments_chrono'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn adjustments_visit_index_present_in_sqlite_master() {
    let pool = fresh_pool().await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'inventory_adjustments_visit'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn adjustments_immutability_trigger_present_in_sqlite_master() {
    let pool = fresh_pool().await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'inventory_adjustments_no_update'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn fk_enforces_existing_item_id() {
    let f = seed().await;
    let bogus = adj(
        Uuid::now_v7(),
        f.user.id,
        5,
        AdjustmentReason::Receive,
        None,
    );
    let mut tx = f.pool.begin().await.unwrap();
    let err = f.repo.append(&mut tx, &bogus).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn version_starts_at_one_and_dirty_one() {
    let f = seed().await;
    let row = adj(f.item.id, f.user.id, 3, AdjustmentReason::Receive, None);
    assert_eq!(row.version, 1);
    assert!(row.dirty);
}
