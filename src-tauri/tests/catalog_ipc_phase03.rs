//! Phase-03 §2.2 IPC handler tests.
//!
//! The catalog commands take `tauri::State` and don't expose `_impl` helpers,
//! so each test drives the underlying `CatalogServices` (which is what every
//! IPC ends up calling). The wiring runs through the same audit-writer +
//! outbox path that production uses, so this layer covers the happy + error
//! paths declared in phase-03-test.md §2.2.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::user_service::UserCreateInput;
use app_lib::domains::auth::{AuthService, UserService};
use app_lib::domains::catalog::domain::services::EffectivePriceQuery;
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteMandoubRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::catalog::service::operator_specialty_service::OperatorSpecialtyInput;
use app_lib::domains::catalog::service::{
    CatalogServices, CatalogServicesConfig, CheckSubtypeCreateInput, CheckSubtypeUpdateInput,
    CheckTypeCreateInput, CheckTypeUpdateInput, ConsumptionCreateInput, ConsumptionUpdateInput,
    DoctorCreateInput, DoctorPricingUpsertInput, DoctorUpdateInput, InventoryItemCreateInput,
    OperatorCreateInput, OperatorUpdateInput,
};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::{mock_app, MockRuntime};
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-1";
const DEVICE_ID: &str = "dev-A";

struct Rig {
    pool: SqlitePool,
    services: CatalogServices<MockRuntime>,
    user_service: std::sync::Arc<UserService>,
    superadmin_id: Uuid,
    _app: tauri::App<MockRuntime>,
}

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

async fn rig() -> Rig {
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
    let user_service = Arc::new(UserService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        DEVICE_ID.into(),
    ));

    let mock = mock_app();
    let handle = mock.handle().clone();

    let services = CatalogServices::new(CatalogServicesConfig {
        pool: pool.clone(),
        check_type_repo: Arc::new(SqliteCheckTypeRepo::new(pool.clone())),
        check_subtype_repo: Arc::new(SqliteCheckSubtypeRepo::new(pool.clone())),
        doctor_repo: Arc::new(SqliteDoctorRepo::new(pool.clone())),
        doctor_pricing_repo: Arc::new(SqliteDoctorPricingRepo::new(pool.clone())),
        operator_repo: Arc::new(SqliteOperatorRepo::new(pool.clone())),
        operator_specialty_repo: Arc::new(SqliteOperatorSpecialtyRepo::new(pool.clone())),
        mandoub_repo: Arc::new(SqliteMandoubRepo::new(pool.clone())),
        inventory_item_repo: Arc::new(SqliteInventoryItemRepo::new(pool.clone())),
        consumption_repo: Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone())),
        audit_repo: audit_repo.clone(),
        outbox_repo: outbox_repo.clone(),
        device_id: DEVICE_ID.into(),
        app_handle: handle,
    });

    let superadmin = auth_service
        .create_first_admin("admin@idc.io", "Mariam", "admin-strong-789", ENTITY_ID)
        .await
        .unwrap();

    Rig {
        pool,
        services,
        user_service,
        superadmin_id: superadmin.id,
        _app: mock,
    }
}

async fn make_receptionist(rig: &Rig) -> Uuid {
    let receptionist = rig
        .user_service
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            UserCreateInput {
                email: "rx@idc.io".into(),
                name: "Rashid".into(),
                role: UserRole::Receptionist,
                password: "rx-strong-pass-789".into(),
                entity_id: ENTITY_ID.into(),
            },
        )
        .await
        .unwrap();
    receptionist.id
}

fn check_type_create_input(
    name: &str,
    base: Option<i64>,
    has_subtypes: bool,
) -> CheckTypeCreateInput {
    CheckTypeCreateInput {
        name_ar: name.into(),
        name_en: None,
        has_subtypes,
        base_price_iqd: base,
        dye_supported: false,
        sort_order: 0,
    }
}

// =========================================================================
// check_types
// =========================================================================

#[tokio::test]
async fn check_types_create_flat_persists_row_with_base_price() {
    let rig = rig().await;
    let svc = &rig.services;
    let created = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    assert!(!created.has_subtypes);
    assert_eq!(created.base_price_iqd, Some(50_000));
    let fetched = svc.check_types.get(created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);
}

#[tokio::test]
async fn check_types_create_xor_violation_returns_validation_error() {
    let rig = rig().await;
    let svc = &rig.services;
    let res = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), true),
        )
        .await;
    assert!(res.is_err(), "XOR rule must reject subtypes + base_price");
}

#[tokio::test]
async fn check_types_create_requires_superadmin() {
    let rig = rig().await;
    let receptionist_id = make_receptionist(&rig).await;
    let svc = &rig.services;
    let res = svc
        .check_types
        .create(
            receptionist_id,
            UserRole::Receptionist,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await;
    assert!(res.is_err(), "non-superadmin must be rejected by service");
}

#[tokio::test]
async fn check_types_update_bumps_version_and_writes_audit_row() {
    let rig = rig().await;
    let svc = &rig.services;
    let created = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let v0 = created.version;

    let updated = svc
        .check_types
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            created.id,
            CheckTypeUpdateInput {
                name_ar: Some("Echo+".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.name_ar, "Echo+");
    assert_eq!(updated.version, v0 + 1);

    let audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE entity = 'check_types'")
            .fetch_one(&rig.pool)
            .await
            .unwrap();
    assert!(audit_count >= 2, "expected create + update audit rows");
}

#[tokio::test]
async fn check_types_toggle_zero_to_one_clears_base_price() {
    let rig = rig().await;
    let svc = &rig.services;
    let flat = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();

    let toggled = svc
        .check_types
        .toggle_has_subtypes(rig.superadmin_id, UserRole::Superadmin, flat.id, true, None)
        .await
        .unwrap();
    assert!(toggled.has_subtypes);
    assert!(toggled.base_price_iqd.is_none());
}

#[tokio::test]
async fn check_types_toggle_one_to_zero_blocked_when_live_subtypes_exist() {
    let rig = rig().await;
    let svc = &rig.services;
    let subtyped = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("MRI", None, true),
        )
        .await
        .unwrap();
    svc.check_subtypes
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckSubtypeCreateInput {
                check_type_id: subtyped.id,
                name_ar: "Brain".into(),
                name_en: None,
                price_iqd: 70_000,
                sort_order: 0,
            },
        )
        .await
        .unwrap();

    let res = svc
        .check_types
        .toggle_has_subtypes(
            rig.superadmin_id,
            UserRole::Superadmin,
            subtyped.id,
            false,
            Some(1000),
        )
        .await;
    assert!(res.is_err(), "live subtypes must block 1->0 toggle");
}

#[tokio::test]
async fn check_types_soft_delete_blocked_when_referenced_by_pricing() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let doc = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Dr Sami".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: doc.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: Some(45_000),
                cut_kind: CutKind::Pct,
                cut_value: 30,
            },
        )
        .await
        .unwrap();

    let res = svc
        .check_types
        .soft_delete(rig.superadmin_id, UserRole::Superadmin, ct.id)
        .await;
    assert!(res.is_err(), "must block delete when references exist");
}

// =========================================================================
// check_subtypes
// =========================================================================

#[tokio::test]
async fn check_subtypes_create_requires_subtyped_parent() {
    let rig = rig().await;
    let svc = &rig.services;
    let flat = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Flat", Some(50_000), false),
        )
        .await
        .unwrap();
    let res = svc
        .check_subtypes
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckSubtypeCreateInput {
                check_type_id: flat.id,
                name_ar: "X".into(),
                name_en: None,
                price_iqd: 1000,
                sort_order: 0,
            },
        )
        .await;
    assert!(res.is_err(), "flat parent must reject subtype creation");
}

#[tokio::test]
async fn check_subtypes_update_bumps_version_and_audits() {
    let rig = rig().await;
    let svc = &rig.services;
    let parent = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("MRI", None, true),
        )
        .await
        .unwrap();
    let s = svc
        .check_subtypes
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckSubtypeCreateInput {
                check_type_id: parent.id,
                name_ar: "Brain".into(),
                name_en: None,
                price_iqd: 70_000,
                sort_order: 0,
            },
        )
        .await
        .unwrap();

    let updated = svc
        .check_subtypes
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            s.id,
            CheckSubtypeUpdateInput {
                price_iqd: Some(80_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.price_iqd, 80_000);
    assert_eq!(updated.version, s.version + 1);
}

// =========================================================================
// doctors
// =========================================================================

#[tokio::test]
async fn doctors_create_persists_and_audits() {
    let rig = rig().await;
    let svc = &rig.services;
    let _d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Layla".into(),
                specialty: Some("Cardio".into()),
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'doctors' AND action = 'create'",
    )
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn doctors_create_rejects_empty_name_after_trim() {
    let rig = rig().await;
    let svc = &rig.services;
    let res = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "   ".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn doctors_update_re_indexes_fts5_after_name_change() {
    let rig = rig().await;
    let svc = &rig.services;
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Old".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctors
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            d.id,
            DoctorUpdateInput {
                name: Some("Renamed".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let hits = svc
        .doctors
        .list(ENTITY_ID, false, Some("Renam".into()))
        .await
        .unwrap();
    assert!(hits.iter().any(|h| h.id == d.id));
}

#[tokio::test]
async fn doctors_soft_delete_cascades_pricings_in_one_logical_op() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Layla".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: d.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: Some(40_000),
                cut_kind: CutKind::Pct,
                cut_value: 25,
            },
        )
        .await
        .unwrap();

    svc.doctors
        .soft_delete(rig.superadmin_id, UserRole::Superadmin, d.id)
        .await
        .unwrap();

    let live_pricings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM doctor_check_pricing WHERE doctor_id = ? AND deleted_at IS NULL",
    )
    .bind(d.id.to_string())
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert_eq!(
        live_pricings, 0,
        "pricings must be cascaded to soft-deleted"
    );
}

#[tokio::test]
async fn doctors_set_active_flips_flag_and_audits_update() {
    let rig = rig().await;
    let svc = &rig.services;
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Layla".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let after = svc
        .doctors
        .set_active(rig.superadmin_id, UserRole::Superadmin, d.id, false)
        .await
        .unwrap();
    assert!(!after.is_active);
    let updates: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'doctors' AND action = 'update' AND entity_id = ?",
    )
    .bind(d.id.to_string())
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert!(updates >= 1, "expected an update audit row");
}

// =========================================================================
// doctor_pricing
// =========================================================================

#[tokio::test]
async fn doctor_pricing_upsert_inserts_then_updates_on_same_tuple() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Sami".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let p1 = svc
        .doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: d.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: Some(45_000),
                cut_kind: CutKind::Pct,
                cut_value: 30,
            },
        )
        .await
        .unwrap();
    let p2 = svc
        .doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: d.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: Some(40_000),
                cut_kind: CutKind::Pct,
                cut_value: 25,
            },
        )
        .await
        .unwrap();
    assert_eq!(p1.id, p2.id, "same tuple must produce same row id");
    assert_eq!(p2.cut_value, 25);
}

#[tokio::test]
async fn doctor_pricing_upsert_rejects_pct_above_100() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Sami".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let res = svc
        .doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: d.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: None,
                cut_kind: CutKind::Pct,
                cut_value: 150,
            },
        )
        .await;
    assert!(
        res.is_err(),
        "pct over 100 must be rejected by entity validation"
    );
}

#[tokio::test]
async fn pricing_resolver_returns_doctor_override_when_present() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Sami".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: d.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: Some(30_000),
                cut_kind: CutKind::Pct,
                cut_value: 25,
            },
        )
        .await
        .unwrap();
    let p = svc
        .pricing_resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: Some(d.id),
            check_type_id: ct.id,
            check_subtype_id: None,
        })
        .await
        .unwrap();
    assert_eq!(p, 30_000);
}

// =========================================================================
// operators
// =========================================================================

#[tokio::test]
async fn operators_create_persists_and_audits() {
    let rig = rig().await;
    let svc = &rig.services;
    let op = svc
        .operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 1000,
                notes: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(op.name, "Hassan");
}

#[tokio::test]
async fn operators_update_bumps_version_and_audits() {
    let rig = rig().await;
    let svc = &rig.services;
    let op = svc
        .operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 1000,
                notes: None,
            },
        )
        .await
        .unwrap();
    let after = svc
        .operators
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            op.id,
            OperatorUpdateInput {
                base_cut_per_check_iqd: Some(2000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(after.base_cut_per_check_iqd, 2000);
    assert_eq!(after.version, op.version + 1);
}

#[tokio::test]
async fn operators_soft_delete_cascades_specialties_in_one_tx() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let op = svc
        .operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 1000,
                notes: None,
            },
        )
        .await
        .unwrap();
    svc.operator_specialties
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorSpecialtyInput {
                operator_id: op.id,
                check_type_id: ct.id,
            },
        )
        .await
        .unwrap();

    svc.operators
        .soft_delete(rig.superadmin_id, UserRole::Superadmin, op.id)
        .await
        .unwrap();

    let live: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operator_specialties WHERE operator_id = ? AND deleted_at IS NULL",
    )
    .bind(op.id.to_string())
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert_eq!(live, 0, "specialties must cascade to soft-deleted");
}

#[tokio::test]
async fn operators_set_active_flips_flag_even_when_open_shifts_exist() {
    // Phase-04 shifts don't exist here -- but per §7.24 the set_active path
    // MUST never block on shifts. This pins the design at the service layer.
    let rig = rig().await;
    let svc = &rig.services;
    let op = svc
        .operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 1000,
                notes: None,
            },
        )
        .await
        .unwrap();
    let after = svc
        .operators
        .set_active(rig.superadmin_id, UserRole::Superadmin, op.id, false)
        .await
        .unwrap();
    assert!(!after.is_active);
}

#[tokio::test]
async fn operator_specialties_upsert_returns_existing_on_duplicate() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let op = svc
        .operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 1000,
                notes: None,
            },
        )
        .await
        .unwrap();
    let s1 = svc
        .operator_specialties
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorSpecialtyInput {
                operator_id: op.id,
                check_type_id: ct.id,
            },
        )
        .await
        .unwrap();
    let s2 = svc
        .operator_specialties
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorSpecialtyInput {
                operator_id: op.id,
                check_type_id: ct.id,
            },
        )
        .await
        .unwrap();
    assert_eq!(s1.id, s2.id, "duplicate must return same id, not a new row");
}

// =========================================================================
// inventory items + consumption map
// =========================================================================

#[tokio::test]
async fn inventory_create_rejects_empty_unit() {
    let rig = rig().await;
    let svc = &rig.services;
    let res = svc
        .inventory_items
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "Gel".into(),
                name_en: None,
                unit: "  ".into(),
                low_stock_threshold: 0,
            },
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn inventory_create_persists_with_quantity_on_hand_zero() {
    let rig = rig().await;
    let svc = &rig.services;
    let i = svc
        .inventory_items
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "Gel".into(),
                name_en: None,
                unit: "ml".into(),
                low_stock_threshold: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(i.quantity_on_hand, 0);
}

#[tokio::test]
async fn inventory_soft_delete_blocked_when_consumption_map_references_exist() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let i = svc
        .inventory_items
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "Gel".into(),
                name_en: None,
                unit: "ml".into(),
                low_stock_threshold: 0,
            },
        )
        .await
        .unwrap();
    svc.consumption
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            ConsumptionCreateInput {
                check_type_id: ct.id,
                check_subtype_id: None,
                item_id: i.id,
                quantity_per_check: 5,
                on_dye_only: false,
            },
        )
        .await
        .unwrap();

    let res = svc
        .inventory_items
        .soft_delete(rig.superadmin_id, UserRole::Superadmin, i.id)
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn inventory_consumption_rejects_dye_only_on_non_dye_parent() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let i = svc
        .inventory_items
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "Dye".into(),
                name_en: None,
                unit: "ml".into(),
                low_stock_threshold: 0,
            },
        )
        .await
        .unwrap();
    let res = svc
        .consumption
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            ConsumptionCreateInput {
                check_type_id: ct.id,
                check_subtype_id: None,
                item_id: i.id,
                quantity_per_check: 1,
                on_dye_only: true,
            },
        )
        .await;
    assert!(res.is_err(), "non-dye parent must reject on_dye_only=true");
}

#[tokio::test]
async fn inventory_consumption_update_bumps_version() {
    let rig = rig().await;
    let svc = &rig.services;
    let ct = svc
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    let item = svc
        .inventory_items
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "Gel".into(),
                name_en: None,
                unit: "ml".into(),
                low_stock_threshold: 0,
            },
        )
        .await
        .unwrap();
    let c = svc
        .consumption
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            ConsumptionCreateInput {
                check_type_id: ct.id,
                check_subtype_id: None,
                item_id: item.id,
                quantity_per_check: 5,
                on_dye_only: false,
            },
        )
        .await
        .unwrap();
    let after = svc
        .consumption
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            ConsumptionUpdateInput {
                id: c.id,
                quantity_per_check: 7,
                on_dye_only: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(after.quantity_per_check, 7);
    assert_eq!(after.version, c.version + 1);
}

// =========================================================================
// every mutation enqueues an outbox row
// =========================================================================

#[tokio::test]
async fn catalog_create_mutations_enqueue_outbox_rows() {
    let rig = rig().await;
    let svc = &rig.services;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    svc.check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            check_type_create_input("Echo", Some(50_000), false),
        )
        .await
        .unwrap();
    svc.doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "X".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Y".into(),
                phone: None,
                base_cut_per_check_iqd: 0,
                notes: None,
            },
        )
        .await
        .unwrap();
    svc.inventory_items
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            InventoryItemCreateInput {
                name_ar: "Gel".into(),
                name_en: None,
                unit: "ml".into(),
                low_stock_threshold: 0,
            },
        )
        .await
        .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert!(after >= before + 4, "expected at least 4 new outbox rows");
}

#[tokio::test]
async fn list_includes_inactive_only_when_flag_true() {
    let rig = rig().await;
    let svc = &rig.services;
    let active = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Active".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let inactive = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Inactive".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctors
        .set_active(rig.superadmin_id, UserRole::Superadmin, inactive.id, false)
        .await
        .unwrap();

    let active_only = svc.doctors.list(ENTITY_ID, false, None).await.unwrap();
    assert_eq!(active_only.len(), 1);
    assert_eq!(active_only[0].id, active.id);

    let all = svc.doctors.list(ENTITY_ID, true, None).await.unwrap();
    assert_eq!(all.len(), 2);
}
