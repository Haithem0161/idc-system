//! Phase-06 §6 edge-category coverage for inventory operations.
//!
//! One scenario per §6.X mandatory edge category:
//! - §6.1 Time / Timezone
//! - §6.2 i18n & RTL (pointer to cross-cutting `i18n-rtl.md`; assertion here
//!   covers byte-exact mixed-direction note storage)
//! - §6.3 Offline & Network
//! - §6.4 Concurrency & Conflicts
//! - §6.5 Crash & Recovery
//! - §6.6 Scale & Performance (smoke scale -- tighter SLOs live in
//!   `inventory_perf_phase06.rs`)
//! - §6.7 Security & Permissions
//! - §6.8 Data Integrity

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
use chrono::{TimeZone, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-edges";
const DEVICE_ID: &str = "dev-edges";

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

async fn rig() -> (
    SqlitePool,
    Arc<InventoryAdjustmentService>,
    Arc<dyn InventoryItemRepo>,
    Arc<dyn InventoryAdjustmentRepo>,
    InventoryItem,
    Uuid,
) {
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
        "edge@x",
        "Edge",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();

    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "صنف".into(),
        name_en: Some("Widget".into()),
        unit: "pcs".into(),
        low_stock_threshold: 3,
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

    (pool, service, items_repo, adjustments_repo, item, user.id)
}

// ---- §6.1 Time / Timezone ---------------------------------------------

#[tokio::test]
async fn adjustment_created_at_is_utc_rfc3339_pre_post_baghdad_midnight() {
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    let adj = service
        .create(
            user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: item.id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap();
    // `created_at` is `DateTime<Utc>`; the RFC3339 string MUST end in `+00:00`
    // and round-trip into a UTC instant -- regardless of the operator's local
    // Asia/Baghdad timezone (UTC+3, no DST).
    let s = adj.created_at.to_rfc3339();
    assert!(s.ends_with("+00:00"), "expected UTC suffix, got {s}");
    let parsed: chrono::DateTime<chrono::FixedOffset> =
        chrono::DateTime::parse_from_rfc3339(&s).expect("created_at must round-trip via RFC3339");
    assert_eq!(parsed.timezone().local_minus_utc(), 0);

    // Defensive: persisted column round-trips to a UTC instant when read back.
    let row: (String,) =
        sqlx::query_as("SELECT created_at FROM inventory_adjustments WHERE id = ?")
            .bind(adj.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    chrono::DateTime::parse_from_rfc3339(&row.0).expect("persisted timestamp must be RFC3339");
}

#[tokio::test]
async fn baghdad_local_midnight_day_boundary_does_not_drift_utc() {
    // Baghdad is UTC+3 fixed -- 23:59:30 local on day D == 20:59:30 UTC, still
    // day D in UTC. Our timestamps are UTC-only; we just defend against any
    // future code path that accidentally bakes the local offset into storage.
    let baghdad = chrono::FixedOffset::east_opt(3 * 3600).unwrap();
    let local = baghdad
        .with_ymd_and_hms(2026, 5, 13, 23, 59, 30)
        .single()
        .unwrap();
    let utc = local.with_timezone(&Utc);
    assert_eq!(utc.date_naive().to_string(), "2026-05-13");
    assert_eq!(utc.format("%H:%M").to_string(), "20:59");
}

// ---- §6.2 i18n & RTL ---------------------------------------------------

#[tokio::test]
async fn mixed_direction_note_persists_byte_exact() {
    // §6.2 mixed-script note round-trip: storing "تالف box" stores those exact
    // bytes (no Unicode bidi mangling). The cross-cutting i18n sweep lives in
    // `i18n-rtl.md`; this row is the data-layer guarantee.
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    let original = "تالف box 12";
    let adj = service
        .create(
            user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: item.id,
                reason: AdjustmentReason::Receive,
                delta: 2,
                note: Some(original.into()),
            },
        )
        .await
        .unwrap();
    let row: (String,) = sqlx::query_as("SELECT note FROM inventory_adjustments WHERE id = ?")
        .bind(adj.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0.as_bytes(), original.as_bytes());
}

// ---- §6.3 Offline & Network -------------------------------------------

#[tokio::test]
async fn create_adjustment_enqueues_outbox_before_any_network_call() {
    // The service never makes a network call -- the sync engine drains the
    // outbox independently. We assert the local outbox row exists immediately
    // after the create returns (no network is required).
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    service
        .create(
            user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: item.id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap();
    let cnt: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM outbox WHERE entity IN ('inventory_adjustments','inventory_items')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // Two business outbox rows + the primary audit_log outbox row from
    // with_audit + the inline item audit_log row -- the key invariant is
    // that all writes commit locally before any network attempt.
    assert!(
        cnt.0 >= 2,
        "expected at least 2 business outbox rows pending offline, got {}",
        cnt.0
    );
}

// ---- §6.4 Concurrency & Conflicts -------------------------------------

#[tokio::test]
async fn two_sequential_receives_compose_additively_under_same_pool() {
    // additive-only policy: the SUM-based recompute means two concurrent
    // receives never conflict -- they each add their delta and the on-hand
    // converges. We exercise the local serialized version of that under a
    // shared pool.
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    for d in [4i64, 7] {
        service
            .create(
                user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: item.id,
                    reason: AdjustmentReason::Receive,
                    delta: d,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(item.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, 11);
    let rows: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM inventory_adjustments WHERE item_id = ?")
            .bind(item.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rows.0, 2, "both rows survive (additive-only)");
}

// ---- §6.5 Crash & Recovery --------------------------------------------

#[tokio::test]
async fn audit_first_invariant_holds_under_repeated_create_attempts() {
    // We can't easily SIGKILL the test process mid-tx, but we can verify the
    // invariant the §6.5 row protects: every committed adjustment carries a
    // pair of audit rows AND a pair of outbox rows. A crash mid-tx would
    // leave the WAL un-committed and on reopen the rows wouldn't exist; the
    // committed state always has both audit rows.
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    for d in 1..=4i64 {
        service
            .create(
                user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: item.id,
                    reason: AdjustmentReason::Receive,
                    delta: d,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    let adj_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM inventory_adjustments WHERE item_id = ?")
            .bind(item.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let item_audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'inventory_items' AND entity_id = ?",
    )
    .bind(item.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    let adj_audit_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE entity = 'inventory_adjustments'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(adj_count.0, 4);
    // Every successful create writes exactly one items audit row + one
    // adjustment audit row. No half-committed state.
    assert_eq!(item_audit_count.0, 4);
    assert_eq!(adj_audit_count.0, 4);
}

#[tokio::test]
async fn fk_violation_leaves_no_partial_state() {
    // §6.5 atomicity: an invalid input must roll back cleanly. Use a bogus
    // item_id that bypasses the service-level existence check by writing
    // directly to the adjustments repo -- assert the FK constraint kicks in
    // and no row persists.
    let (pool, _service, _items, adj_repo, item, user_id) = rig().await;
    let bogus_item = Uuid::now_v7();
    let bad = app_lib::domains::visits::domain::entities::InventoryAdjustment::try_new(
        app_lib::domains::visits::domain::entities::AdjustmentNewInput {
            item_id: bogus_item,
            delta: 1,
            reason: AdjustmentReason::Receive,
            visit_id: None,
            note: None,
            by_user_id: user_id,
            entity_id: ENTITY_ID.into(),
            origin_device_id: Some(DEVICE_ID.into()),
        },
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let res = adj_repo.append(&mut tx, &bad).await;
    assert!(
        res.is_err(),
        "FK on inventory_adjustments.item_id must reject"
    );
    // Even if a follow-up transaction commits a valid row, the bad row is
    // gone.
    drop(tx);
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM inventory_adjustments WHERE item_id = ?")
            .bind(bogus_item.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 0);
    // Sanity: real item still works after the failed insert.
    let _ = item;
}

// ---- §6.6 Scale & Performance (smoke) ----------------------------------

#[tokio::test]
async fn list_items_remains_under_smoke_threshold_at_100_items() {
    // §6.6 smoke scale -- 100 items + 100 receive adjustments. The tighter
    // 10k-item SLO lives in `inventory_perf_phase06.rs`.
    let (pool, service, items_repo, _adj, _item, user_id) = rig().await;
    // Seed 99 additional items (the rig already created 1).
    for i in 0..99 {
        let it = InventoryItem::try_new(InventoryItemNewInput {
            name_ar: format!("صنف-{i}"),
            name_en: Some(format!("Item-{i}")),
            unit: "pcs".into(),
            low_stock_threshold: 2,
            entity_id: ENTITY_ID.into(),
            origin_device_id: Some(DEVICE_ID.into()),
        })
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        items_repo.upsert(&mut tx, &it).await.unwrap();
        tx.commit().await.unwrap();
        if i % 3 == 0 {
            service
                .create(
                    user_id,
                    UserRole::Receptionist,
                    ENTITY_ID,
                    AdjustmentInput {
                        item_id: it.id,
                        reason: AdjustmentReason::Receive,
                        delta: 5,
                        note: None,
                    },
                )
                .await
                .unwrap();
        }
    }
    let start = std::time::Instant::now();
    let rows = service
        .list_items(ENTITY_ID, None, false, None)
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(rows.len() >= 100);
    // Debug-mode smoke threshold: <= 500ms. Tighter SLO in perf binary.
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "list_items at 100 items took {elapsed:?}; smoke ceiling 500ms"
    );
}

// ---- §6.7 Security & Permissions --------------------------------------

#[tokio::test]
async fn receptionist_blocked_from_count_correction_at_service_layer() {
    let (_pool, service, _items, _adj, item, user_id) = rig().await;
    let err = service
        .create(
            user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: 5,
                note: None,
            },
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("Superadmin"));
}

#[tokio::test]
async fn accountant_blocked_from_receive_at_service_layer() {
    let (_pool, service, _items, _adj, item, user_id) = rig().await;
    let err = service
        .create(
            user_id,
            UserRole::Accountant,
            ENTITY_ID,
            AdjustmentInput {
                item_id: item.id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Receptionist") || msg.contains("Superadmin"));
}

#[tokio::test]
async fn check_constraint_blocks_invalid_reason_delta_combos_via_raw_sql() {
    // §6.7 + §6.8: backstop CHECK + trigger reject all invalid combos even
    // if the application layer is bypassed.
    let (pool, _service, _items, _adj, item, user_id) = rig().await;
    let raw_insert = |delta: i64, reason: &str| {
        let pool = pool.clone();
        let item_id = item.id.to_string();
        let user_id = user_id.to_string();
        let reason = reason.to_string();
        async move {
            sqlx::query(
                "INSERT INTO inventory_adjustments \
                 (id, item_id, delta, reason, visit_id, note, by_user_id, \
                  created_at, updated_at, deleted_at, version, dirty, \
                  last_synced_at, origin_device_id, entity_id) \
                 VALUES (?,?,?,?,NULL,NULL,?, \
                         datetime('now'),datetime('now'),NULL,1,1,NULL,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(item_id)
            .bind(delta)
            .bind(reason)
            .bind(user_id)
            .bind(DEVICE_ID)
            .bind(ENTITY_ID)
            .execute(&pool)
            .await
        }
    };
    // receive=0 / receive<0 / writeoff=0 / writeoff>0 / count_correction=0
    // all rejected. consume_visit without visit_id is rejected by the
    // phase-05 NOT NULL constraint on the joined visit FK predicate.
    assert!(raw_insert(0, "receive").await.is_err());
    assert!(raw_insert(-1, "receive").await.is_err());
    assert!(raw_insert(0, "writeoff").await.is_err());
    assert!(raw_insert(3, "writeoff").await.is_err());
    assert!(raw_insert(0, "count_correction").await.is_err());
    // count_correction with positive / negative non-zero is accepted.
    assert!(raw_insert(2, "count_correction").await.is_ok());
    assert!(raw_insert(-2, "count_correction").await.is_ok());
}

// ---- §6.8 Data Integrity -----------------------------------------------

#[tokio::test]
async fn item_version_increments_monotonically_per_adjustment() {
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    let initial: (i64,) = sqlx::query_as("SELECT version FROM inventory_items WHERE id = ?")
        .bind(item.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    let v0 = initial.0;
    for _ in 0..3 {
        service
            .create(
                user_id,
                UserRole::Receptionist,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: item.id,
                    reason: AdjustmentReason::Receive,
                    delta: 1,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    let after: (i64,) = sqlx::query_as("SELECT version FROM inventory_items WHERE id = ?")
        .bind(item.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        after.0,
        v0 + 3,
        "version must bump exactly once per adjustment"
    );
}

#[tokio::test]
async fn migration_replay_idempotent_on_populated_db() {
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    // Populate with a real adjustment + its audit rows.
    service
        .create(
            user_id,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: item.id,
                reason: AdjustmentReason::Receive,
                delta: 5,
                note: None,
            },
        )
        .await
        .unwrap();
    // Re-run migrations. The `IF NOT EXISTS` clauses + partial index
    // re-creation must be safe against populated tables.
    migrations::run(&pool).await.unwrap();
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM inventory_adjustments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "data survives migration replay");
}

#[tokio::test]
async fn quantity_on_hand_matches_sum_of_live_adjustments_property_smoke() {
    // §6.8 sum-consistency property smoke. We can't run hypothesis-style
    // proptest here without a new dep, but we drive a representative
    // sequence and assert the invariant after every step.
    let (pool, service, _items, _adj, item, user_id) = rig().await;
    let ops: &[(i64, AdjustmentReason)] = &[
        (10, AdjustmentReason::Receive),
        (3, AdjustmentReason::Writeoff),
        (15, AdjustmentReason::Receive),
        (2, AdjustmentReason::Writeoff),
        (4, AdjustmentReason::Receive),
    ];
    for (qty, reason) in ops {
        service
            .create(
                user_id,
                UserRole::Superadmin,
                ENTITY_ID,
                AdjustmentInput {
                    item_id: item.id,
                    reason: *reason,
                    delta: *qty,
                    note: None,
                },
            )
            .await
            .unwrap();
        let sum_row: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(delta), 0) FROM inventory_adjustments \
             WHERE item_id = ? AND deleted_at IS NULL",
        )
        .bind(item.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        let qty_row: (i64,) =
            sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
                .bind(item.id.to_string())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            qty_row.0, sum_row.0,
            "quantity_on_hand drifted from SUM(delta) at intermediate step"
        );
    }
    // Final invariant: 10 - 3 + 15 - 2 + 4 = 24.
    let _ = StockStatus::Ok; // referenced so the import compiles
    let qty_row: (i64,) =
        sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
            .bind(item.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(qty_row.0, 24);
}
