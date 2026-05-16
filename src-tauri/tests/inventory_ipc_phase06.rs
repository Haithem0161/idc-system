//! Phase-06 §2.2 IPC wire-shape coverage for the inventory commands.
//!
//! Drives `InventoryAdjustmentService` along the path each Tauri command
//! takes, then serializes the response through the per-command DTOs from
//! `domains::inventory::commands`. The frontend only sees the JSON, so the
//! JSON is the contract -- these assertions are the IPC contract.
//!
//! Covers the five Phase-06 commands (`inventory_list_items`,
//! `inventory_get_item`, `inventory_list_adjustments`,
//! `inventory_create_adjustment`, `inventory_recompute_on_hand`) plus the
//! shared `AppError` envelope shape for every error path.

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
use app_lib::domains::inventory::commands::{
    InventoryAdjustmentDto, InventoryItemDto, ItemDetailDto, RecomputeResult,
};
use app_lib::domains::inventory::service::{
    AdjustmentInput, InventoryAdjustmentService, InventoryAdjustmentServiceConfig, StockStatus,
};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::domains::visits::domain::entities::AdjustmentReason;
use app_lib::domains::visits::domain::repositories::InventoryAdjustmentRepo;
use app_lib::domains::visits::infrastructure::SqliteInventoryAdjustmentRepo;
use app_lib::error::AppError;
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-ipc";
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
    service: Arc<InventoryAdjustmentService>,
    item: InventoryItem,
    actor: Uuid,
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
        "boss@x",
        "Boss",
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
            items_repo,
            consumption_repo,
            adjustments_repo,
            audit_repo,
            outbox_repo,
            device_id: DEVICE_ID.to_string(),
        },
    ));

    Rig {
        service,
        item,
        actor: user.id,
    }
}

// ---- inventory_list_items ----------------------------------------------

#[tokio::test]
async fn list_items_returns_typed_dto_with_status_and_dirty_fields() {
    let r = rig().await;
    let rows = r
        .service
        .list_items(ENTITY_ID, None, false, None)
        .await
        .unwrap();
    let dtos: Vec<InventoryItemDto> = rows.iter().map(InventoryItemDto::from).collect();
    let json = serde_json::to_value(&dtos).unwrap();
    let arr = json.as_array().expect("list_items returns a JSON array");
    assert_eq!(arr.len(), 1);
    let row = &arr[0];
    for key in [
        "id",
        "name_ar",
        "name_en",
        "unit",
        "quantity_on_hand",
        "low_stock_threshold",
        "is_active",
        "status",
        "updated_at",
        "created_at",
        "version",
        "dirty",
        "last_synced_at",
        "entity_id",
    ] {
        assert!(
            row.get(key).is_some(),
            "missing required key `{key}` in InventoryItemDto JSON: {row}"
        );
    }
    assert_eq!(row.get("status").and_then(Value::as_str), Some("low"));
    assert_eq!(row.get("dirty").and_then(Value::as_bool), Some(true));
    assert_eq!(
        row.get("entity_id").and_then(Value::as_str),
        Some(ENTITY_ID)
    );
}

#[tokio::test]
async fn list_items_filtered_by_status_serializes_only_matching_rows() {
    let r = rig().await;
    // Seed an Ok-status item via a receive +10 on the existing widget.
    r.service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 10,
                note: None,
            },
        )
        .await
        .unwrap();
    let rows = r
        .service
        .list_items(ENTITY_ID, Some(StockStatus::Ok), false, None)
        .await
        .unwrap();
    let json =
        serde_json::to_value(rows.iter().map(InventoryItemDto::from).collect::<Vec<_>>()).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].get("status").and_then(Value::as_str), Some("ok"));
}

// ---- inventory_get_item -----------------------------------------------

#[tokio::test]
async fn get_item_returns_three_part_envelope_with_typed_keys() {
    let r = rig().await;
    r.service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 7,
                note: None,
            },
        )
        .await
        .unwrap();
    let detail = r.service.get_item(ENTITY_ID, r.item.id).await.unwrap();
    let dto = ItemDetailDto::from(&detail);
    let v = serde_json::to_value(&dto).unwrap();
    for key in ["item", "consumption_map", "recent_adjustments"] {
        assert!(
            v.get(key).is_some(),
            "missing required key `{key}` in ItemDetailDto JSON: {v}"
        );
    }
    let item_v = v.get("item").unwrap();
    assert_eq!(item_v.get("status").and_then(Value::as_str), Some("ok"));
    assert_eq!(
        item_v.get("quantity_on_hand").and_then(Value::as_i64),
        Some(7)
    );
    let adjs = v.get("recent_adjustments").unwrap().as_array().unwrap();
    assert_eq!(adjs.len(), 1);
    for key in [
        "id",
        "item_id",
        "delta",
        "reason",
        "visit_id",
        "note",
        "by_user_id",
        "created_at",
        "updated_at",
        "version",
        "entity_id",
        "is_reversal",
    ] {
        assert!(
            adjs[0].get(key).is_some(),
            "missing required key `{key}` in InventoryAdjustmentDto JSON: {}",
            adjs[0]
        );
    }
    assert_eq!(
        adjs[0].get("is_reversal").and_then(Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn get_item_not_found_serializes_as_typed_app_error_envelope() {
    let r = rig().await;
    let err = r
        .service
        .get_item(ENTITY_ID, Uuid::now_v7())
        .await
        .unwrap_err();
    let v = serde_json::to_value(&err).unwrap();
    // The shared AppError envelope is `{ code: "...", message: "..." }`
    // per `src-tauri/src/error.rs` Serialize impl.
    assert_eq!(
        v.get("code").and_then(Value::as_str),
        Some("NOT_FOUND"),
        "AppError JSON envelope must carry the canonical code: {v}"
    );
    assert!(v
        .get("message")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false));
}

// ---- inventory_list_adjustments ----------------------------------------

#[tokio::test]
async fn list_adjustments_returns_array_with_is_reversal_flag() {
    let r = rig().await;
    let adj = r
        .service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 5,
                note: None,
            },
        )
        .await
        .unwrap();
    let rows = r
        .service
        .list_adjustments(ENTITY_ID, r.item.id, 50)
        .await
        .unwrap();
    let dtos: Vec<InventoryAdjustmentDto> = rows.iter().map(InventoryAdjustmentDto::from).collect();
    let v = serde_json::to_value(&dtos).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0].get("id").and_then(Value::as_str),
        Some(adj.id.to_string()).as_deref()
    );
    assert_eq!(
        arr[0].get("is_reversal").and_then(Value::as_bool),
        Some(false)
    );
}

// ---- inventory_create_adjustment --------------------------------------

#[tokio::test]
async fn create_adjustment_serializes_typed_dto_with_canonical_reason_string() {
    let r = rig().await;
    let adj = r
        .service
        .create(
            r.actor,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: -3,
                note: Some("annual count".into()),
            },
        )
        .await
        .unwrap();
    let dto = InventoryAdjustmentDto::from(&adj);
    let v = serde_json::to_value(&dto).unwrap();
    assert_eq!(
        v.get("reason").and_then(Value::as_str),
        Some("count_correction"),
        "reason MUST serialize as snake_case: {v}"
    );
    assert_eq!(v.get("delta").and_then(Value::as_i64), Some(-3));
    assert_eq!(v.get("note").and_then(Value::as_str), Some("annual count"));
}

#[tokio::test]
async fn create_adjustment_validation_error_serializes_as_app_error_validation() {
    let r = rig().await;
    let err = r
        .service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 0,
                note: None,
            },
        )
        .await
        .unwrap_err();
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(
        v.get("code").and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
}

#[tokio::test]
async fn create_adjustment_forbidden_serializes_as_validation_kind() {
    // The service currently surfaces role-gate failures as `Validation` per
    // its `Self::require_role` implementation in service/mod.rs. Document
    // the wire-shape so the frontend can parse it consistently.
    let r = rig().await;
    let err = r
        .service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::CountCorrection,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap_err();
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(
        v.get("code").and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    let msg = v.get("message").and_then(Value::as_str).unwrap_or_default();
    assert!(
        msg.contains("Superadmin"),
        "forbidden error must surface a useful message: {msg}"
    );
}

#[tokio::test]
async fn create_adjustment_consume_visit_returns_typed_validation_error() {
    let r = rig().await;
    let err = r
        .service
        .create(
            r.actor,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::ConsumeVisit,
                delta: -1,
                note: None,
            },
        )
        .await
        .unwrap_err();
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(
        v.get("code").and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    let msg = v.get("message").and_then(Value::as_str).unwrap_or_default();
    assert!(msg.to_lowercase().contains("lock workflow"));
}

// ---- inventory_recompute_on_hand --------------------------------------

#[tokio::test]
async fn recompute_on_hand_returns_typed_recompute_result_struct() {
    let r = rig().await;
    r.service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 12,
                note: None,
            },
        )
        .await
        .unwrap();
    let n = r
        .service
        .recompute_on_hand(r.actor, UserRole::Superadmin, ENTITY_ID, r.item.id)
        .await
        .unwrap();
    let dto = RecomputeResult { new_on_hand: n };
    let v = serde_json::to_value(&dto).unwrap();
    // The wire shape MUST be `{ "new_on_hand": <int> }` -- the frontend
    // hooks expect snake_case (`useInventoryRecompute` in queries.ts).
    assert_eq!(v.get("new_on_hand").and_then(Value::as_i64), Some(12));
    assert_eq!(v.as_object().unwrap().len(), 1, "no extra fields: {v}");
}

#[tokio::test]
async fn recompute_on_hand_non_superadmin_serializes_validation_envelope() {
    let r = rig().await;
    let err = r
        .service
        .recompute_on_hand(r.actor, UserRole::Receptionist, ENTITY_ID, r.item.id)
        .await
        .unwrap_err();
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(
        v.get("code").and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
}

// ---- Shared AppError envelope sanity ----------------------------------

#[tokio::test]
async fn app_error_envelope_carries_code_and_message_only() {
    let v = serde_json::to_value(AppError::NotFound("x".into())).unwrap();
    let obj = v.as_object().expect("AppError must serialize as an object");
    let keys: std::collections::HashSet<_> = obj.keys().cloned().collect();
    assert!(keys.contains("code"), "AppError must carry a `code`: {v}");
    assert!(
        keys.contains("message"),
        "AppError must carry a `message`: {v}"
    );
    assert_eq!(v.get("code").and_then(Value::as_str), Some("NOT_FOUND"));
    // Other variants stay shape-stable and use the documented codes.
    for (sample, code) in [
        (AppError::Validation("v".into()), "VALIDATION_ERROR"),
        (AppError::Configuration("c".into()), "CONFIGURATION_ERROR"),
        (AppError::NotAuthenticated, "NOT_AUTHENTICATED"),
        (AppError::SessionExpired, "SESSION_EXPIRED"),
        (AppError::Conflict("c".into()), "CONFLICT_PARKED"),
        (AppError::Network("n".into()), "NETWORK_OFFLINE"),
        (AppError::Database("d".into()), "DATABASE_ERROR"),
    ] {
        let v = serde_json::to_value(&sample).unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o.get("code").and_then(Value::as_str), Some(code));
        assert!(o.contains_key("message"));
    }
}

// ---- Round-trip through serde_json::Value for invoke()-style call -----

#[tokio::test]
async fn create_adjustment_returns_round_trip_compatible_payload() {
    let r = rig().await;
    let adj = r
        .service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 4,
                note: Some("box".into()),
            },
        )
        .await
        .unwrap();
    let dto = InventoryAdjustmentDto::from(&adj);
    let v = serde_json::to_value(&dto).unwrap();
    // Re-parse as a generic object and check every leaf is JSON-safe (no
    // `undefined`-like artifacts, no boxed binary payloads).
    let s = v.to_string();
    let again: Value = serde_json::from_str(&s).unwrap();
    assert_eq!(again.get("reason").and_then(Value::as_str), Some("receive"));
    assert_eq!(again.get("delta").and_then(Value::as_i64), Some(4));
    // ISO-8601 datetime strings.
    let created = again.get("created_at").and_then(Value::as_str).unwrap();
    assert!(created.contains('T') && created.ends_with("+00:00"));
}

#[tokio::test]
async fn create_adjustment_serializes_note_null_when_omitted() {
    let r = rig().await;
    let adj = r
        .service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap();
    let v = serde_json::to_value(InventoryAdjustmentDto::from(&adj)).unwrap();
    // `note` is `Option<String>` -> must serialize as JSON null when None,
    // not as the empty string and not as missing key.
    assert!(matches!(v.get("note"), Some(Value::Null)));
}

#[tokio::test]
async fn list_items_dto_visit_id_field_is_absent_on_items() {
    // Defence against accidental schema bleed: InventoryItemDto must NOT
    // carry visit_id (that lives on InventoryAdjustmentDto).
    let r = rig().await;
    let rows = r
        .service
        .list_items(ENTITY_ID, None, false, None)
        .await
        .unwrap();
    let v =
        serde_json::to_value(rows.iter().map(InventoryItemDto::from).collect::<Vec<_>>()).unwrap();
    let row = &v.as_array().unwrap()[0];
    assert!(row.get("visit_id").is_none());
    assert!(row.get("reason").is_none());
}

#[tokio::test]
async fn create_adjustment_dto_serializes_visit_id_null_for_non_consume() {
    let r = rig().await;
    let adj = r
        .service
        .create(
            r.actor,
            UserRole::Receptionist,
            ENTITY_ID,
            AdjustmentInput {
                item_id: r.item.id,
                reason: AdjustmentReason::Receive,
                delta: 1,
                note: None,
            },
        )
        .await
        .unwrap();
    let v = serde_json::to_value(InventoryAdjustmentDto::from(&adj)).unwrap();
    assert!(matches!(v.get("visit_id"), Some(Value::Null)));
    // Make sure the canonical envelope sample matches downstream snapshots.
    let canonical = json!({
        "delta": 1,
        "reason": "receive",
        "is_reversal": false,
    });
    for (k, expected) in canonical.as_object().unwrap() {
        assert_eq!(
            v.get(k),
            Some(expected),
            "field `{k}` drifted in InventoryAdjustmentDto canonical shape"
        );
    }
}
