//! Phase-03 §5 Canonical persona walk: **P3 Mariam the Superadmin**.
//!
//! Mariam logs in (phase-02 done), then walks the admin surface end-to-end:
//!   1. Create flat + subtyped check types (with XOR enforced).
//! 2. Add subtypes to the subtyped check type.
//! 3. Toggle a flat check type to subtyped (clears base price).
//! 4. Create doctors with FTS search.
//! 5. Add per-doctor pricing rows (one no-subtype, one with subtype).
//! 6. Verify `effective_price` resolves to the override / fallback chain.
//! 7. Create operators and assign specialties.
//! 8. Create inventory items + consumption maps.
//! 9. Re-list everything to confirm the day's edits stick.
//! 10. Soft-delete one doctor and confirm cascade.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::AuthService;
use app_lib::domains::catalog::domain::services::EffectivePriceQuery;
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::catalog::service::operator_specialty_service::OperatorSpecialtyInput;
use app_lib::domains::catalog::service::{
    CatalogServices, CatalogServicesConfig, CheckSubtypeCreateInput, CheckTypeCreateInput,
    ConsumptionCreateInput, DoctorCreateInput, DoctorPricingUpsertInput, InventoryItemCreateInput,
    OperatorCreateInput,
};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::{mock_app, MockRuntime};

const ENTITY_ID: &str = "tenant-mariam";
const DEVICE_ID: &str = "dev-mariam";

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
async fn p3_mariam_superadmin_catalog_day_walks_every_phase_03_ipc() {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let outbox_repo = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo = Arc::new(SqliteAuditRepo::new(pool.clone()));

    let auth_service = AuthService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        DEVICE_ID.into(),
    );

    let mock = mock_app();
    let handle = mock.handle().clone();

    let svc: CatalogServices<MockRuntime> = CatalogServices::new(CatalogServicesConfig {
        pool: pool.clone(),
        check_type_repo: Arc::new(SqliteCheckTypeRepo::new(pool.clone())),
        check_subtype_repo: Arc::new(SqliteCheckSubtypeRepo::new(pool.clone())),
        doctor_repo: Arc::new(SqliteDoctorRepo::new(pool.clone())),
        doctor_pricing_repo: Arc::new(SqliteDoctorPricingRepo::new(pool.clone())),
        operator_repo: Arc::new(SqliteOperatorRepo::new(pool.clone())),
        operator_specialty_repo: Arc::new(SqliteOperatorSpecialtyRepo::new(pool.clone())),
        inventory_item_repo: Arc::new(SqliteInventoryItemRepo::new(pool.clone())),
        consumption_repo: Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone())),
        audit_repo,
        outbox_repo,
        device_id: DEVICE_ID.into(),
        app_handle: handle,
    });

    // ---- Bootstrap superadmin (carry-over from phase-02) ----
    let mariam = auth_service
        .create_first_admin("mariam@idc.io", "Mariam", "mariam-strong-99", ENTITY_ID)
        .await
        .unwrap();
    let uid = mariam.id;
    let role = UserRole::Superadmin;

    // Step 1: create one flat + one subtyped check type, XOR enforced.
    let flat = svc
        .check_types
        .create(
            uid,
            role,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "تخطيط القلب".into(),
                name_en: Some("ECG".into()),
                has_subtypes: false,
                base_price_iqd: Some(20_000),
                dye_supported: false,
                report_supported: true,
                sort_order: 1,
            },
        )
        .await
        .unwrap();
    let mri = svc
        .check_types
        .create(
            uid,
            role,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "رنين مغناطيسي".into(),
                name_en: Some("MRI".into()),
                has_subtypes: true,
                base_price_iqd: None,
                dye_supported: true,
                report_supported: true,
                sort_order: 2,
            },
        )
        .await
        .unwrap();

    // Step 2: add 2 subtypes to MRI.
    let brain = svc
        .check_subtypes
        .create(
            uid,
            role,
            ENTITY_ID,
            CheckSubtypeCreateInput {
                check_type_id: mri.id,
                name_ar: "دماغ".into(),
                name_en: Some("Brain".into()),
                price_iqd: 70_000,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    let _spine = svc
        .check_subtypes
        .create(
            uid,
            role,
            ENTITY_ID,
            CheckSubtypeCreateInput {
                check_type_id: mri.id,
                name_ar: "عمود فقري".into(),
                name_en: Some("Spine".into()),
                price_iqd: 65_000,
                sort_order: 1,
            },
        )
        .await
        .unwrap();

    // Step 3: toggle flat -> subtyped (must clear base price).
    let toggled = svc
        .check_types
        .toggle_has_subtypes(uid, role, flat.id, true, None)
        .await
        .unwrap();
    assert!(toggled.has_subtypes);
    assert!(toggled.base_price_iqd.is_none());

    // Step 4: doctors with mixed scripts + FTS search.
    let layla = svc
        .doctors
        .create(
            uid,
            role,
            ENTITY_ID,
            DoctorCreateInput {
                name: "د. Layla هاشم".into(),
                specialty: Some("Cardiology".into()),
                phone: Some("0770-100".into()),
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let sami = svc
        .doctors
        .create(
            uid,
            role,
            ENTITY_ID,
            DoctorCreateInput {
                name: "د. Sami الكفائي".into(),
                specialty: Some("Radiology".into()),
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let layla_hits = svc
        .doctors
        .list(ENTITY_ID, false, Some("Layla".into()))
        .await
        .unwrap();
    assert!(layla_hits.iter().any(|d| d.id == layla.id));

    // Step 5: per-doctor pricing rows. MRI is subtyped so each pricing must
    // target a specific subtype. We add one with override and one with cut
    // only so the resolver chain exercises both forks.
    svc.doctor_pricing
        .upsert(
            uid,
            role,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: layla.id,
                check_type_id: mri.id,
                check_subtype_id: Some(brain.id),
                price_override_iqd: Some(65_000),
                cut_kind: CutKind::Pct,
                cut_value: 25,
            },
        )
        .await
        .unwrap();
    svc.doctor_pricing
        .upsert(
            uid,
            role,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: sami.id,
                check_type_id: mri.id,
                check_subtype_id: Some(brain.id),
                price_override_iqd: None,
                cut_kind: CutKind::Pct,
                cut_value: 30,
            },
        )
        .await
        .unwrap();

    // Step 6: effective_price resolves the override and the fallback chain.
    let p_with_override = svc
        .pricing_resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: Some(layla.id),
            check_type_id: mri.id,
            check_subtype_id: Some(brain.id),
        })
        .await
        .unwrap();
    assert_eq!(p_with_override, 65_000);
    let p_fallback_to_subtype = svc
        .pricing_resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: Some(sami.id),
            check_type_id: mri.id,
            check_subtype_id: Some(brain.id),
        })
        .await
        .unwrap();
    assert_eq!(p_fallback_to_subtype, 70_000);

    // Step 7: operators + specialties.
    let hassan = svc
        .operators
        .create(
            uid,
            role,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 1_500,
                notes: None,
            },
        )
        .await
        .unwrap();
    svc.operator_specialties
        .upsert(
            uid,
            role,
            ENTITY_ID,
            OperatorSpecialtyInput {
                operator_id: hassan.id,
                check_type_id: mri.id,
            },
        )
        .await
        .unwrap();
    let hassan_detail = svc.operators.get_with_specialties(hassan.id).await.unwrap();
    assert_eq!(hassan_detail.1.len(), 1);

    // Step 8: inventory items + consumption map.
    let gel = svc
        .inventory_items
        .create(
            uid,
            role,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "جل".into(),
                name_en: Some("Gel".into()),
                unit: "ml".into(),
                low_stock_threshold: 100,
            },
        )
        .await
        .unwrap();
    let _consumption = svc
        .consumption
        .create(
            uid,
            role,
            ENTITY_ID,
            ConsumptionCreateInput {
                check_type_id: mri.id,
                check_subtype_id: Some(brain.id),
                item_id: gel.id,
                quantity_per_check: 5,
                on_dye_only: false,
            },
        )
        .await
        .unwrap();

    // Step 9: re-list to confirm day's edits stick.
    let cts = svc.check_types.list(ENTITY_ID, false, None).await.unwrap();
    assert!(cts.iter().any(|c| c.id == mri.id));
    assert!(cts.iter().any(|c| c.id == flat.id));
    let docs = svc.doctors.list(ENTITY_ID, false, None).await.unwrap();
    assert_eq!(docs.len(), 2);
    let ops = svc.operators.list(ENTITY_ID, false, None).await.unwrap();
    assert_eq!(ops.len(), 1);
    let items = svc
        .inventory_items
        .list(ENTITY_ID, false, None)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);

    // Step 10: soft-delete a doctor cascades pricings.
    svc.doctors.soft_delete(uid, role, layla.id).await.unwrap();
    let pricings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM doctor_check_pricing WHERE doctor_id = ? AND deleted_at IS NULL",
    )
    .bind(layla.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pricings, 0, "cascade must soft-delete pricings");

    // Final sanity: audit log carries multiple actions across catalog entities.
    let actions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE entity IN ('check_types','check_subtypes','doctors','doctor_check_pricing',
                          'operators','operator_specialties','inventory_items',
                          'inventory_consumption_map')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        actions >= 12,
        "expected >= 12 audit rows across the catalog day, got {actions}"
    );
}
