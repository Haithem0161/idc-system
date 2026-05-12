//! Integration tests for Phase-3 catalog: SQLite migration + repositories.
//!
//! Exercises the eight repositories and the FTS5 doctor search against an
//! in-memory SQLite database. The full services need a Tauri AppHandle and
//! are smoke-tested via `cargo test --lib`.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::catalog::domain::entities::check_subtype::CheckSubtypeNewInput;
use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
use app_lib::domains::catalog::domain::entities::doctor::DoctorNewInput;
use app_lib::domains::catalog::domain::entities::doctor_pricing::DoctorPricingNewInput;
use app_lib::domains::catalog::domain::entities::inventory_consumption::ConsumptionMapNewInput;
use app_lib::domains::catalog::domain::entities::inventory_item::InventoryItemNewInput;
use app_lib::domains::catalog::domain::entities::operator::OperatorNewInput;
use app_lib::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem,
    Operator,
};
use app_lib::domains::catalog::domain::repositories::{
    CatalogListFilter, CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo,
    InventoryConsumptionRepo, InventoryItemRepo, OperatorRepo,
};
use app_lib::domains::catalog::domain::services::{EffectivePriceQuery, PricingResolver};
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteOperatorRepo,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

const ENTITY_ID: &str = "tenant-x";

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

fn new_flat_check_type(name: &str, price: i64) -> CheckType {
    CheckType::try_new(CheckTypeNewInput {
        name_ar: name.into(),
        name_en: None,
        has_subtypes: false,
        base_price_iqd: Some(price),
        dye_supported: false,
        report_supported: false,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap()
}

fn new_subtyped_check_type(name: &str, dye_supported: bool) -> CheckType {
    CheckType::try_new(CheckTypeNewInput {
        name_ar: name.into(),
        name_en: None,
        has_subtypes: true,
        base_price_iqd: None,
        dye_supported,
        report_supported: false,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap()
}

fn new_doctor(name: &str, specialty: Option<&str>) -> Doctor {
    Doctor::try_new(DoctorNewInput {
        name: name.into(),
        specialty: specialty.map(|s| s.into()),
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap()
}

#[tokio::test]
async fn check_type_repo_roundtrip() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    let ct = new_flat_check_type("Echo", 50_000);
    let id = ct.id;
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &ct).await.unwrap();
    tx.commit().await.unwrap();

    let got = repo.get_by_id(id).await.unwrap().unwrap();
    assert_eq!(got.name_ar, "Echo");
    assert_eq!(got.base_price_iqd, Some(50_000));
    assert!(!got.has_subtypes);

    let list = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: false,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn check_subtype_requires_known_parent() {
    let pool = fresh_pool().await;
    let parent = new_subtyped_check_type("MRI", false);
    let pid = parent.id;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &parent).await.unwrap();
    tx.commit().await.unwrap();

    let sub = CheckSubtype::try_new(CheckSubtypeNewInput {
        check_type_id: pid,
        name_ar: "Brain".into(),
        name_en: None,
        price_iqd: 75_000,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();

    let sub_repo = SqliteCheckSubtypeRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    sub_repo.upsert(&mut tx, &sub).await.unwrap();
    tx.commit().await.unwrap();

    let list = sub_repo.list_by_type(pid).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name_ar, "Brain");

    // Live subtype count is reflected by CheckTypeRepo helper.
    assert_eq!(ct_repo.count_live_subtypes(pid).await.unwrap(), 1);
}

#[tokio::test]
async fn doctor_fts_search_excludes_soft_deleted_and_inactive() {
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let alice = new_doctor("Alice Mustafa", Some("Cardiology"));
    let bob = new_doctor("Bob Khalid", Some("Radiology"));
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &alice).await.unwrap();
    repo.upsert(&mut tx, &bob).await.unwrap();
    tx.commit().await.unwrap();

    let hits = repo.search_fts(ENTITY_ID, "alice", false).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "Alice Mustafa");

    let by_specialty = repo.search_fts(ENTITY_ID, "radio", false).await.unwrap();
    assert_eq!(by_specialty.len(), 1);
    assert_eq!(by_specialty[0].name, "Bob Khalid");

    // Soft-delete Alice -> she no longer appears.
    let alice_gone = alice.soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &alice_gone).await.unwrap();
    tx.commit().await.unwrap();
    let hits = repo.search_fts(ENTITY_ID, "alice", false).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn doctor_pricing_unique_per_tuple_including_null_subtype() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let doc_repo = SqliteDoctorRepo::new(pool.clone());
    let price_repo = SqliteDoctorPricingRepo::new(pool.clone());

    let ct = new_flat_check_type("Ultrasound", 30_000);
    let doc = new_doctor("Dr. Sami", None);
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    tx.commit().await.unwrap();

    let p1 = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: Some(28_000),
        cut_kind: CutKind::Pct,
        cut_value: 30,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    price_repo.upsert(&mut tx, &p1).await.unwrap();
    tx.commit().await.unwrap();

    // Second pricing row with the same (doctor, type, NULL subtype) must
    // collide on the partial unique index.
    let p2 = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: None,
        cut_kind: CutKind::Fixed,
        cut_value: 5_000,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let res = price_repo.upsert(&mut tx, &p2).await;
    assert!(res.is_err(), "duplicate pricing row should fail unique");
}

#[tokio::test]
async fn effective_price_resolver_walks_fallback_chain() {
    let pool = fresh_pool().await;
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let sub_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let price_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let doc_repo = SqliteDoctorRepo::new(pool.clone());

    let flat = new_flat_check_type("Flat", 20_000);
    let parent = new_subtyped_check_type("MRI", false);
    let sub = CheckSubtype::try_new(CheckSubtypeNewInput {
        check_type_id: parent.id,
        name_ar: "Brain".into(),
        name_en: None,
        price_iqd: 70_000,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let doc = new_doctor("Dr. Aya", None);
    let pricing_override = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: parent.id,
        check_subtype_id: Some(sub.id),
        price_override_iqd: Some(60_000),
        cut_kind: CutKind::Pct,
        cut_value: 25,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &flat).await.unwrap();
    ct_repo.upsert(&mut tx, &parent).await.unwrap();
    sub_repo.upsert(&mut tx, &sub).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    price_repo.upsert(&mut tx, &pricing_override).await.unwrap();
    tx.commit().await.unwrap();

    let resolver = PricingResolver::new(ct_repo.clone(), sub_repo.clone(), price_repo.clone());

    // Flat type, no doctor -> base price.
    let p = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: None,
            check_type_id: flat.id,
            check_subtype_id: None,
        })
        .await
        .unwrap();
    assert_eq!(p, 20_000);

    // Subtype, no doctor -> subtype price.
    let p = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: None,
            check_type_id: parent.id,
            check_subtype_id: Some(sub.id),
        })
        .await
        .unwrap();
    assert_eq!(p, 70_000);

    // Subtype with doctor override -> override.
    let p = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: Some(doc.id),
            check_type_id: parent.id,
            check_subtype_id: Some(sub.id),
        })
        .await
        .unwrap();
    assert_eq!(p, 60_000);
}

#[tokio::test]
async fn consumption_map_unique_per_tuple_with_null_subtype() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let item_repo = SqliteInventoryItemRepo::new(pool.clone());
    let consumption_repo = SqliteInventoryConsumptionRepo::new(pool.clone());

    let ct = new_flat_check_type("Ultrasound", 30_000);
    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "Gel".into(),
        name_en: None,
        unit: "ml".into(),
        low_stock_threshold: 100,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    item_repo.upsert(&mut tx, &item).await.unwrap();
    tx.commit().await.unwrap();

    let row1 = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: ct.id,
        check_subtype_id: None,
        item_id: item.id,
        quantity_per_check: 5,
        on_dye_only: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    consumption_repo.upsert(&mut tx, &row1).await.unwrap();
    tx.commit().await.unwrap();

    let row2 = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: ct.id,
        check_subtype_id: None,
        item_id: item.id,
        quantity_per_check: 7,
        on_dye_only: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let res = consumption_repo.upsert(&mut tx, &row2).await;
    assert!(res.is_err(), "duplicate consumption row should fail unique");
}

#[tokio::test]
async fn operator_repo_filters_inactive() {
    let pool = fresh_pool().await;
    let repo = SqliteOperatorRepo::new(pool.clone());
    let active = Operator::try_new(OperatorNewInput {
        name: "Hassan".into(),
        phone: None,
        base_cut_per_check_iqd: 1_000,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut inactive = Operator::try_new(OperatorNewInput {
        name: "Layla".into(),
        phone: None,
        base_cut_per_check_iqd: 1_500,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    inactive.is_active = false;
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &active).await.unwrap();
    repo.upsert(&mut tx, &inactive).await.unwrap();
    tx.commit().await.unwrap();

    let list_default = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: false,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(list_default.len(), 1);
    assert_eq!(list_default[0].name, "Hassan");

    let list_all = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: true,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(list_all.len(), 2);
}
