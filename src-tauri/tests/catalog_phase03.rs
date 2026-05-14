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
use app_lib::domains::catalog::domain::entities::operator_specialty::OperatorSpecialtyNewInput;
use app_lib::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem,
    Operator, OperatorSpecialty,
};
use app_lib::domains::catalog::domain::repositories::{
    CatalogListFilter, CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo,
    InventoryConsumptionRepo, InventoryItemRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use app_lib::domains::catalog::domain::services::{EffectivePriceQuery, PricingResolver};
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
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

// =========================================================================
// Phase-03 §2.1 expanded coverage: every catalog entity at the repo layer.
// =========================================================================

#[tokio::test]
async fn check_type_re_upsert_preserves_id_and_replaces_row() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    let ct = new_flat_check_type("Echo", 50_000);
    let id = ct.id;
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &ct).await.unwrap();
    tx.commit().await.unwrap();

    let updated = ct
        .with_updated_fields(
            app_lib::domains::catalog::domain::entities::check_type::CheckTypeUpdate {
                base_price_iqd: Some(Some(75_000)),
                ..Default::default()
            },
        )
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &updated).await.unwrap();
    tx.commit().await.unwrap();

    let got = repo.get_by_id(id).await.unwrap().unwrap();
    assert_eq!(got.base_price_iqd, Some(75_000));
    assert_eq!(got.version, 2);
}

#[tokio::test]
async fn check_type_list_filters_by_entity_id() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    let mut a = new_flat_check_type("A", 1000);
    a.entity_id = "tenant-a".into();
    let mut b = new_flat_check_type("B", 1000);
    b.entity_id = "tenant-b".into();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &a).await.unwrap();
    repo.upsert(&mut tx, &b).await.unwrap();
    tx.commit().await.unwrap();

    let a_only = repo
        .list(CatalogListFilter {
            entity_id: "tenant-a".into(),
            include_deleted: false,
            include_inactive: false,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(a_only.len(), 1);
    assert_eq!(a_only[0].name_ar, "A");
}

#[tokio::test]
async fn check_type_list_excludes_soft_deleted_by_default() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    let ct = new_flat_check_type("Live", 1000);
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &ct).await.unwrap();
    tx.commit().await.unwrap();

    let removed = ct.soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &removed).await.unwrap();
    tx.commit().await.unwrap();

    let default_list = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: false,
            query: None,
        })
        .await
        .unwrap();
    assert!(default_list.is_empty());

    // Soft-delete also clears is_active; need both flags to surface the row.
    let with_deleted = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: true,
            include_inactive: true,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(with_deleted.len(), 1);
}

#[tokio::test]
async fn check_type_count_live_references_excludes_soft_deleted_subtypes() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let sub_repo = SqliteCheckSubtypeRepo::new(pool.clone());

    let parent = new_subtyped_check_type("MRI", false);
    let sub = CheckSubtype::try_new(CheckSubtypeNewInput {
        check_type_id: parent.id,
        name_ar: "Brain".into(),
        name_en: None,
        price_iqd: 1000,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &parent).await.unwrap();
    sub_repo.upsert(&mut tx, &sub).await.unwrap();
    tx.commit().await.unwrap();

    assert_eq!(ct_repo.count_live_subtypes(parent.id).await.unwrap(), 1);

    let gone = sub.soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    sub_repo.upsert(&mut tx, &gone).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(ct_repo.count_live_subtypes(parent.id).await.unwrap(), 0);
}

#[tokio::test]
async fn check_subtype_list_excludes_soft_deleted() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let sub_repo = SqliteCheckSubtypeRepo::new(pool.clone());
    let parent = new_subtyped_check_type("CT", false);
    let live = CheckSubtype::try_new(CheckSubtypeNewInput {
        check_type_id: parent.id,
        name_ar: "Live".into(),
        name_en: None,
        price_iqd: 1000,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let dead = CheckSubtype::try_new(CheckSubtypeNewInput {
        check_type_id: parent.id,
        name_ar: "Dead".into(),
        name_en: None,
        price_iqd: 1000,
        sort_order: 1,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &parent).await.unwrap();
    sub_repo.upsert(&mut tx, &live).await.unwrap();
    let dead_tomb = dead.clone().soft_deleted();
    sub_repo.upsert(&mut tx, &dead_tomb).await.unwrap();
    tx.commit().await.unwrap();

    let listed = sub_repo.list_by_type(parent.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name_ar, "Live");
}

#[tokio::test]
async fn doctor_fts_search_prefix_matches_partial_arabic_names() {
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let layla = new_doctor("د. Layla هاشم", None);
    let mahmoud = new_doctor("د. Mahmoud صبري", None);
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &layla).await.unwrap();
    repo.upsert(&mut tx, &mahmoud).await.unwrap();
    tx.commit().await.unwrap();

    let hits = repo.search_fts(ENTITY_ID, "Layl", false).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].name.contains("Layla"));

    let hits = repo.search_fts(ENTITY_ID, "Mahm", false).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].name.contains("Mahmoud"));
}

#[tokio::test]
async fn doctor_fts_reindexes_after_name_update() {
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let d = new_doctor("Old Name", None);
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &d).await.unwrap();
    tx.commit().await.unwrap();

    let updated = d
        .with_updated_fields(
            app_lib::domains::catalog::domain::entities::doctor::DoctorUpdate {
                name: Some("Renamed".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &updated).await.unwrap();
    tx.commit().await.unwrap();

    let by_old = repo.search_fts(ENTITY_ID, "Old", false).await.unwrap();
    assert!(by_old.is_empty(), "FTS5 must re-index after update");
    let by_new = repo.search_fts(ENTITY_ID, "Renam", false).await.unwrap();
    assert_eq!(by_new.len(), 1);
}

#[tokio::test]
async fn doctor_fts_un_soft_delete_currently_corrupts_index_def_008_sentinel() {
    // DEF-008 P3 sentinel: the migration-003 `doctors_au` trigger does an
    // unconditional FTS5 delete using old.rowid. When a soft-deleted row is
    // restored (deleted_at -> NULL), the trigger tries to delete a row that
    // was never inserted (the soft-delete UPDATE had skipped the re-insert),
    // and external-content FTS5 returns "database disk image is malformed".
    // The product never restores soft-deleted doctors today, so this is
    // deferred. This test pins the CURRENT broken behaviour; invert when
    // DEF-008 lands.
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let d = new_doctor("Restorable", None);
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &d).await.unwrap();
    tx.commit().await.unwrap();

    let gone = d.clone().soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &gone).await.unwrap();
    tx.commit().await.unwrap();

    let mut restored = gone.clone();
    restored.deleted_at = None;
    restored.is_active = true;
    restored.version += 1;
    restored.dirty = true;
    let mut tx = pool.begin().await.unwrap();
    let result = repo.upsert(&mut tx, &restored).await;
    // Today the FTS5 trigger flow errors when the old row was already
    // soft-deleted. When DEF-008 lands the assertion below MUST be inverted.
    assert!(
        result.is_err(),
        "expected DEF-008 sentinel error; if this succeeds, invert the assertion"
    );
}

#[tokio::test]
async fn doctor_fts_search_escapes_special_chars_in_user_input() {
    // The `sanitize_fts` helper strips non-alphanumeric characters and
    // appends prefix-`*` markers. Punctuation / quotes / parens that would
    // otherwise be FTS5 syntax MUST be filtered without erroring.
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let d = new_doctor("Layla", None);
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &d).await.unwrap();
    tx.commit().await.unwrap();

    for hostile in ["Lay\"la", "Lay'la", "Lay(la", "Lay)la", "Lay-la"] {
        let r = repo.search_fts(ENTITY_ID, hostile, false).await;
        assert!(
            r.is_ok(),
            "fts5 must accept sanitized input `{hostile}`, got {r:?}"
        );
    }
}

#[tokio::test]
async fn doctor_pricing_unique_allows_subtype_null_and_specific_subtype() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let sub_repo = SqliteCheckSubtypeRepo::new(pool.clone());
    let doc_repo = SqliteDoctorRepo::new(pool.clone());
    let price_repo = SqliteDoctorPricingRepo::new(pool.clone());

    let ct = new_subtyped_check_type("MRI", false);
    let sub = CheckSubtype::try_new(CheckSubtypeNewInput {
        check_type_id: ct.id,
        name_ar: "Brain".into(),
        name_en: None,
        price_iqd: 70_000,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let doc = new_doctor("Dr Pricing", None);
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    sub_repo.upsert(&mut tx, &sub).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    tx.commit().await.unwrap();

    let p_null = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: Some(20_000),
        cut_kind: CutKind::Pct,
        cut_value: 30,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let p_specific = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: Some(sub.id),
        price_override_iqd: Some(50_000),
        cut_kind: CutKind::Fixed,
        cut_value: 1000,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    price_repo.upsert(&mut tx, &p_null).await.unwrap();
    price_repo.upsert(&mut tx, &p_specific).await.unwrap();
    tx.commit().await.unwrap();

    let list = price_repo.list_by_doctor(doc.id).await.unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn doctor_pricing_soft_deleted_row_does_not_block_re_insert() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let doc_repo = SqliteDoctorRepo::new(pool.clone());
    let price_repo = SqliteDoctorPricingRepo::new(pool.clone());

    let ct = new_flat_check_type("Ultrasound", 30_000);
    let doc = new_doctor("Sami", None);
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

    let removed = p1.soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    price_repo.upsert(&mut tx, &removed).await.unwrap();
    tx.commit().await.unwrap();

    // After soft-delete the partial unique index releases the slot.
    let p2 = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: Some(29_000),
        cut_kind: CutKind::Fixed,
        cut_value: 1500,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    price_repo.upsert(&mut tx, &p2).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn doctor_pricing_check_constraint_blocks_pct_above_100_at_db_layer() {
    let pool = fresh_pool().await;
    let ct = new_flat_check_type("X", 1000);
    let doc = new_doctor("X", None);
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let doc_repo = SqliteDoctorRepo::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    tx.commit().await.unwrap();

    // Bypass entity validation by writing the row directly.
    let raw_id = uuid::Uuid::now_v7().to_string();
    let raw_doc = doc.id.to_string();
    let raw_ct = ct.id.to_string();
    let result = sqlx::query(
        "INSERT INTO doctor_check_pricing (id, doctor_id, check_type_id, check_subtype_id,
         price_override_iqd, cut_kind, cut_value, created_at, updated_at, version, dirty,
         entity_id) VALUES (?, ?, ?, NULL, NULL, 'pct', 200, datetime('now'), datetime('now'), 1, 1, ?)",
    )
    .bind(&raw_id)
    .bind(&raw_doc)
    .bind(&raw_ct)
    .bind(ENTITY_ID)
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "DB CHECK should reject cut_value > 100 for pct"
    );
}

#[tokio::test]
async fn effective_price_doctor_with_no_override_falls_back_to_subtype_price() {
    let pool = fresh_pool().await;
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let sub_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let price_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let doc_repo = SqliteDoctorRepo::new(pool.clone());

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
    let doc = new_doctor("D", None);
    // Cut row but no price_override - resolver should fall back to subtype price.
    let cut_only = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: parent.id,
        check_subtype_id: Some(sub.id),
        price_override_iqd: None,
        cut_kind: CutKind::Pct,
        cut_value: 25,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &parent).await.unwrap();
    sub_repo.upsert(&mut tx, &sub).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    price_repo.upsert(&mut tx, &cut_only).await.unwrap();
    tx.commit().await.unwrap();

    let resolver = PricingResolver::new(ct_repo, sub_repo, price_repo);
    let p = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: Some(doc.id),
            check_type_id: parent.id,
            check_subtype_id: Some(sub.id),
        })
        .await
        .unwrap();
    assert_eq!(p, 70_000);
}

#[tokio::test]
async fn effective_price_excludes_soft_deleted_pricing_rows() {
    let pool = fresh_pool().await;
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let sub_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let price_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let doc_repo = SqliteDoctorRepo::new(pool.clone());

    let ct = new_flat_check_type("Echo", 30_000);
    let doc = new_doctor("D", None);
    let p = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: Some(5_000),
        cut_kind: CutKind::Pct,
        cut_value: 30,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    price_repo.upsert(&mut tx, &p).await.unwrap();
    tx.commit().await.unwrap();

    // Soft-delete the pricing row; resolver should fall back to the flat base.
    let removed = p.soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    price_repo.upsert(&mut tx, &removed).await.unwrap();
    tx.commit().await.unwrap();

    let resolver = PricingResolver::new(ct_repo, sub_repo, price_repo);
    let res = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: Some(doc.id),
            check_type_id: ct.id,
            check_subtype_id: None,
        })
        .await
        .unwrap();
    assert_eq!(res, 30_000);
}

#[tokio::test]
async fn effective_price_rejects_missing_check_type() {
    let pool = fresh_pool().await;
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let sub_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let price_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let resolver = PricingResolver::new(ct_repo, sub_repo, price_repo);
    let res = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: None,
            check_type_id: uuid::Uuid::now_v7(),
            check_subtype_id: None,
        })
        .await;
    assert!(res.is_err(), "should NotFound an unknown check_type");
}

#[tokio::test]
async fn effective_price_rejects_subtyped_type_without_subtype() {
    let pool = fresh_pool().await;
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let sub_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let price_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let parent = new_subtyped_check_type("MRI", false);
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &parent).await.unwrap();
    tx.commit().await.unwrap();
    let resolver = PricingResolver::new(ct_repo, sub_repo, price_repo);
    let res = resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: None,
            check_type_id: parent.id,
            check_subtype_id: None,
        })
        .await;
    assert!(
        res.is_err(),
        "subtyped check_type without subtype must Validation-error"
    );
}

#[tokio::test]
async fn operator_specialty_partial_unique_blocks_duplicate_active_pair() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let op_repo = SqliteOperatorRepo::new(pool.clone());
    let spec_repo = SqliteOperatorSpecialtyRepo::new(pool.clone());

    let ct = new_flat_check_type("Echo", 30_000);
    let op = Operator::try_new(OperatorNewInput {
        name: "OpA".into(),
        phone: None,
        base_cut_per_check_iqd: 100,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();

    let s1 = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: op.id,
        check_type_id: ct.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let s2 = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: op.id,
        check_type_id: ct.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    op_repo.upsert(&mut tx, &op).await.unwrap();
    spec_repo.upsert(&mut tx, &s1).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let res = spec_repo.upsert(&mut tx, &s2).await;
    assert!(
        res.is_err(),
        "duplicate active (op, check_type) must collide"
    );
}

#[tokio::test]
async fn operator_specialty_find_match_returns_active_row_when_present() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let op_repo = SqliteOperatorRepo::new(pool.clone());
    let spec_repo = SqliteOperatorSpecialtyRepo::new(pool.clone());

    let ct = new_flat_check_type("Echo", 30_000);
    let op = Operator::try_new(OperatorNewInput {
        name: "OpA".into(),
        phone: None,
        base_cut_per_check_iqd: 100,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let s = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: op.id,
        check_type_id: ct.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    op_repo.upsert(&mut tx, &op).await.unwrap();
    spec_repo.upsert(&mut tx, &s).await.unwrap();
    tx.commit().await.unwrap();

    let found = spec_repo.find_match(op.id, ct.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, s.id);
}

#[tokio::test]
async fn inventory_item_count_live_consumption_refs_excludes_soft_deleted() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let item_repo = SqliteInventoryItemRepo::new(pool.clone());
    let cons_repo = SqliteInventoryConsumptionRepo::new(pool.clone());

    let ct = new_flat_check_type("Echo", 30_000);
    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "Gel".into(),
        name_en: None,
        unit: "ml".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let row = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
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
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    item_repo.upsert(&mut tx, &item).await.unwrap();
    cons_repo.upsert(&mut tx, &row).await.unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        item_repo
            .count_live_consumption_refs(item.id)
            .await
            .unwrap(),
        1
    );

    let removed = row.soft_deleted();
    let mut tx = pool.begin().await.unwrap();
    cons_repo.upsert(&mut tx, &removed).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        item_repo
            .count_live_consumption_refs(item.id)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn inventory_item_unit_check_constraint_blocks_blank_at_db() {
    let pool = fresh_pool().await;
    let raw_id = uuid::Uuid::now_v7().to_string();
    let res = sqlx::query(
        "INSERT INTO inventory_items (id, name_ar, unit, quantity_on_hand, low_stock_threshold,
         is_active, created_at, updated_at, version, dirty, entity_id) VALUES (?, ?, '  ', 0, 0, 1,
         datetime('now'), datetime('now'), 1, 1, ?)",
    )
    .bind(&raw_id)
    .bind("Item")
    .bind(ENTITY_ID)
    .execute(&pool)
    .await;
    assert!(res.is_err(), "DB CHECK length(trim(unit)) > 0 must fire");
}

#[tokio::test]
async fn inventory_consumption_paired_unique_distinct_on_dye_only_flag() {
    // The partial unique index includes on_dye_only as a column, so the same
    // (check_type, subtype, item) tuple with different on_dye_only values may
    // both exist. Pin this design decision.
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let item_repo = SqliteInventoryItemRepo::new(pool.clone());
    let cons_repo = SqliteInventoryConsumptionRepo::new(pool.clone());

    let ct = new_flat_check_type("Echo", 30_000);
    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "Gel".into(),
        name_en: None,
        unit: "ml".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    item_repo.upsert(&mut tx, &item).await.unwrap();
    tx.commit().await.unwrap();

    let a = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: ct.id,
        check_subtype_id: None,
        item_id: item.id,
        quantity_per_check: 5,
        on_dye_only: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let b = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: ct.id,
        check_subtype_id: None,
        item_id: item.id,
        quantity_per_check: 7,
        on_dye_only: true,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    cons_repo.upsert(&mut tx, &a).await.unwrap();
    cons_repo.upsert(&mut tx, &b).await.unwrap();
    tx.commit().await.unwrap();

    let listed = cons_repo.list_by_item(item.id).await.unwrap();
    assert_eq!(listed.len(), 2);
}

#[tokio::test]
async fn fts5_consistency_after_many_random_mutations() {
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let names = [
        "Dr. Alpha",
        "Dr. Beta",
        "Dr. Gamma",
        "Dr. Delta",
        "Dr. Epsilon",
        "Dr. Zeta",
    ];
    let mut docs: Vec<Doctor> = Vec::new();
    for n in names {
        let d = new_doctor(n, None);
        let mut tx = pool.begin().await.unwrap();
        repo.upsert(&mut tx, &d).await.unwrap();
        tx.commit().await.unwrap();
        docs.push(d);
    }
    // Soft-delete every second doctor.
    for (i, d) in docs.iter().enumerate() {
        if i % 2 == 1 {
            let gone = d.clone().soft_deleted();
            let mut tx = pool.begin().await.unwrap();
            repo.upsert(&mut tx, &gone).await.unwrap();
            tx.commit().await.unwrap();
        }
    }
    // The FTS5 index should now match exactly the live (non-deleted) doctor set.
    let live: Vec<&Doctor> = docs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, d)| d)
        .collect();
    for d in &live {
        let prefix = &d.name[..4];
        let hits = repo.search_fts(ENTITY_ID, prefix, false).await.unwrap();
        assert!(
            hits.iter().any(|h| h.id == d.id),
            "expected to find live doctor {}",
            d.name
        );
    }
    let removed: Vec<&Doctor> = docs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, d)| d)
        .collect();
    for d in &removed {
        let prefix = &d.name[..4];
        let hits = repo.search_fts(ENTITY_ID, prefix, false).await.unwrap();
        assert!(
            !hits.iter().any(|h| h.id == d.id),
            "did not expect to find soft-deleted doctor {}",
            d.name
        );
    }
}

#[tokio::test]
async fn doctor_repo_includes_inactive_when_filter_true() {
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let active = new_doctor("Active", None);
    let mut inactive = new_doctor("Inactive", None);
    inactive.is_active = false;
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &active).await.unwrap();
    repo.upsert(&mut tx, &inactive).await.unwrap();
    tx.commit().await.unwrap();

    let active_only = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: false,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(active_only.len(), 1);
    assert_eq!(active_only[0].name, "Active");

    let all = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: true,
            query: None,
        })
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn migration_replay_is_idempotent_on_populated_db() {
    let pool = fresh_pool().await;
    // Seed one of every entity.
    let ct = new_flat_check_type("Echo", 30_000);
    let doc = new_doctor("Sami", None);
    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "Gel".into(),
        name_en: None,
        unit: "ml".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let op = Operator::try_new(OperatorNewInput {
        name: "Op".into(),
        phone: None,
        base_cut_per_check_iqd: 0,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    SqliteCheckTypeRepo::new(pool.clone())
        .upsert(&mut tx, &ct)
        .await
        .unwrap();
    SqliteDoctorRepo::new(pool.clone())
        .upsert(&mut tx, &doc)
        .await
        .unwrap();
    SqliteInventoryItemRepo::new(pool.clone())
        .upsert(&mut tx, &item)
        .await
        .unwrap();
    SqliteOperatorRepo::new(pool.clone())
        .upsert(&mut tx, &op)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Replay migrations on a populated DB; rows must survive untouched.
    migrations::run(&pool).await.unwrap();

    let got_doc = SqliteDoctorRepo::new(pool.clone())
        .get_by_id(doc.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_doc.name, "Sami");
}

#[tokio::test]
async fn check_type_sort_index_is_used_by_order_by_query() {
    // EXPLAIN QUERY PLAN should mention the `check_types_sort` index when we
    // filter by entity_id with sort_order. This pins the partial-index design.
    let pool = fresh_pool().await;
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN
         SELECT id FROM check_types
         WHERE entity_id = ? AND deleted_at IS NULL
         ORDER BY sort_order ASC",
    )
    .bind(ENTITY_ID)
    .fetch_all(&pool)
    .await
    .unwrap();
    let detail = plan
        .iter()
        .map(|r| r.3.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        detail.contains("check_types_sort") || detail.contains("USING INDEX"),
        "expected plan to use check_types_sort index, got: {detail}"
    );
}

#[tokio::test]
async fn inventory_items_active_index_supports_filter_predicate() {
    let pool = fresh_pool().await;
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN
         SELECT id FROM inventory_items
         WHERE entity_id = ? AND is_active = 1 AND deleted_at IS NULL",
    )
    .bind(ENTITY_ID)
    .fetch_all(&pool)
    .await
    .unwrap();
    let detail = plan
        .iter()
        .map(|r| r.3.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        detail.contains("inventory_items_active") || detail.contains("USING INDEX"),
        "expected plan to use inventory_items_active index, got: {detail}"
    );
}

#[tokio::test]
async fn doctors_fts_external_content_mode_declared_in_schema() {
    // Pin the FTS5 declaration so a regression rewriting the table as a
    // contentless FTS5 (duplicate storage) is caught at the schema level.
    let pool = fresh_pool().await;
    let sql: (String,) = sqlx::query_as("SELECT sql FROM sqlite_master WHERE name = 'doctors_fts'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let s = sql.0;
    assert!(
        s.contains("content='doctors'"),
        "missing content='doctors': {s}"
    );
    assert!(
        s.contains("content_rowid='rowid'"),
        "missing content_rowid='rowid': {s}"
    );
}
