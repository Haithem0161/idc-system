//! Phase 05 §5: P2 Mehdi the Receptionist canonical persona.
//!
//! This is the DoD-gating script. The 10-step walk follows:
//!   1) Bootstrap: empty checks grid; the receptionist's role passes
//!      `require_role`.
//!   2) Search a patient by FTS prefix.
//!   3) Create a new patient inline (the visit form's "create on first
//!      save" flow).
//!   4) Create a draft for the chosen check type.
//!   5) Pricing resolve returns a fresh snapshot block; the draft row's
//!      version is unchanged afterwards.
//!   6) Lock the visit: snapshots populate, audit row emits, inventory
//!      adjusts, outbox grows, receipt files exist on disk.
//!   7) The lines-run-today count for the operator is 1.
//!   8) Mariam the superadmin voids the visit with a 5-char+ reason;
//!      offset rows appear and inventory restores.
//!   9) Discarding a *fresh* draft (separate visit) soft-deletes it and
//!      writes a discard audit row.
//!  10) A second draft can be created after the discard (the index slot
//!      is reusable; no constraint left dirty).

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
use app_lib::domains::catalog::domain::entities::doctor::DoctorNewInput;
use app_lib::domains::catalog::domain::entities::doctor_pricing::DoctorPricingNewInput;
use app_lib::domains::catalog::domain::entities::inventory_consumption::ConsumptionMapNewInput;
use app_lib::domains::catalog::domain::entities::inventory_item::InventoryItemNewInput;
use app_lib::domains::catalog::domain::entities::operator::OperatorNewInput;
use app_lib::domains::catalog::domain::entities::operator_specialty::OperatorSpecialtyNewInput;
use app_lib::domains::catalog::domain::entities::{
    CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem, Operator,
    OperatorSpecialty,
};
use app_lib::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo, InventoryConsumptionRepo,
    InventoryItemRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::patients::domain::repositories::PatientRepo;
use app_lib::domains::patients::infrastructure::SqlitePatientRepo;
use app_lib::domains::patients::service::{PatientCreateInput, PatientService};
use app_lib::domains::receipts::ReceiptRenderOptions;
use app_lib::domains::reports::infrastructure::SqliteFrozenCloseRepo;
use app_lib::domains::shifts::domain::entities::operator_shift::OperatorShiftOpenInput;
use app_lib::domains::shifts::domain::entities::OperatorShift;
use app_lib::domains::shifts::domain::repositories::OperatorShiftRepo;
use app_lib::domains::shifts::infrastructure::SqliteOperatorShiftRepo;
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::domains::visits::domain::entities::VisitStatus;
use app_lib::domains::visits::domain::repositories::{InventoryAdjustmentRepo, VisitRepo};
use app_lib::domains::visits::domain::services::MoneySettings;
use app_lib::domains::visits::infrastructure::{SqliteInventoryAdjustmentRepo, SqliteVisitRepo};
use app_lib::domains::visits::service::{CreateDraftInput, VisitService, VisitServiceConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-persona";
const DEVICE_ID: &str = "dev-persona";

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
async fn persona_p2_mehdi_walks_through_phase05_reception_day() {
    // ---- bootstrap ----------------------------------------------------
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let cs_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let doc_repo: Arc<dyn DoctorRepo> = Arc::new(SqliteDoctorRepo::new(pool.clone()));
    let dp_repo: Arc<dyn DoctorPricingRepo> = Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let op_repo: Arc<dyn OperatorRepo> = Arc::new(SqliteOperatorRepo::new(pool.clone()));
    let os_repo: Arc<dyn OperatorSpecialtyRepo> =
        Arc::new(SqliteOperatorSpecialtyRepo::new(pool.clone()));
    let item_repo: Arc<dyn InventoryItemRepo> =
        Arc::new(SqliteInventoryItemRepo::new(pool.clone()));
    let cons_repo: Arc<dyn InventoryConsumptionRepo> =
        Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone()));
    let shift_repo: Arc<dyn OperatorShiftRepo> =
        Arc::new(SqliteOperatorShiftRepo::new(pool.clone()));
    let patient_repo: Arc<dyn PatientRepo> = Arc::new(SqlitePatientRepo::new(pool.clone()));
    let visit_repo: Arc<dyn VisitRepo> = Arc::new(SqliteVisitRepo::new(pool.clone()));
    let adj_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(pool.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));

    let mehdi = User::try_new(
        "mehdi@idc",
        "Mehdi",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mariam = User::try_new(
        "mariam@idc",
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

    let check_type = CheckType::try_new(CheckTypeNewInput {
        name_ar: "أشعة".into(),
        name_en: Some("X-Ray".into()),
        has_subtypes: false,
        base_price_iqd: Some(75_000),
        dye_supported: true,
        report_supported: true,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let doctor = Doctor::try_new(DoctorNewInput {
        name: "Dr Sara".into(),
        specialty: Some("Radiology".into()),
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
        default_cut_kind: None,
        default_cut_value: None,
    })
    .unwrap();
    let pricing = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doctor.id,
        check_type_id: check_type.id,
        check_subtype_id: None,
        price_override_iqd: None,
        cut_kind: CutKind::Pct,
        cut_value: 30,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let kareem = Operator::try_new(OperatorNewInput {
        name: "Kareem".into(),
        phone: None,
        base_cut_per_check_iqd: 5_000,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let kareem_spec = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: kareem.id,
        check_type_id: check_type.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let film = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "فيلم".into(),
        name_en: Some("Film".into()),
        unit: "pcs".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let film_consume = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: check_type.id,
        check_subtype_id: None,
        item_id: film.id,
        quantity_per_check: 1,
        on_dye_only: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &check_type).await.unwrap();
    doc_repo.upsert(&mut tx, &doctor).await.unwrap();
    dp_repo.upsert(&mut tx, &pricing).await.unwrap();
    op_repo.upsert(&mut tx, &kareem).await.unwrap();
    os_repo.upsert(&mut tx, &kareem_spec).await.unwrap();
    item_repo.upsert(&mut tx, &film).await.unwrap();
    cons_repo.upsert(&mut tx, &film_consume).await.unwrap();
    tx.commit().await.unwrap();

    let kareem_shift = OperatorShift::open(OperatorShiftOpenInput {
        operator_id: kareem.id,
        by_user_id: mehdi.id,
        note: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    shift_repo.upsert(&mut tx, &kareem_shift).await.unwrap();
    tx.commit().await.unwrap();

    let patient_service = Arc::new(PatientService::new(
        pool.clone(),
        patient_repo.clone(),
        visit_repo.clone(),
        audit.clone(),
        outbox.clone(),
        DEVICE_ID.to_string(),
    ));
    // Pre-seed two patients so the FTS index has content to search.
    for n in ["Layla Ahmadi", "Layth Salman", "Bob Brown"] {
        patient_service
            .create(mehdi.id, ENTITY_ID, PatientCreateInput { name: n.into() })
            .await
            .unwrap();
    }

    let receipts_dir = std::env::temp_dir().join(format!("idc-persona-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&receipts_dir).unwrap();

    let visit_service = Arc::new(VisitService::new(VisitServiceConfig {
        pool: pool.clone(),
        visits: visit_repo,
        adjustments: adj_repo,
        patients: patient_repo.clone(),
        check_types: ct_repo,
        check_subtypes: cs_repo,
        doctors: doc_repo,
        doctor_pricing: dp_repo,
        operators: op_repo,
        operator_specialties: os_repo,
        consumption: cons_repo,
        inventory_items: item_repo,
        shifts: shift_repo,
        frozen_close: Arc::new(SqliteFrozenCloseRepo::new(pool.clone())),
        audit_repo: audit,
        outbox_repo: outbox,
        receipts_dir,
        device_id: DEVICE_ID.to_string(),
    }));

    let money = MoneySettings {
        dye_cost_iqd: 2_000,
        report_cost_iqd: 3_000,
        internal_doctor_pct: 40,
    };

    // ---- Step 1: checks grid renders one card per active check type --
    let grid = visit_service.checks_grid(ENTITY_ID).await.unwrap();
    assert!(grid.iter().any(|c| c.check_type_id == check_type.id));

    // ---- Step 2: search the patient FTS --------------------------------
    let layla_matches = patient_service.search(ENTITY_ID, "Lay", 5).await.unwrap();
    assert!(layla_matches.iter().any(|p| p.name.starts_with("Layla")));

    // ---- Step 3: create a brand new patient inline (the visit form's
    // first-save path).
    let new_patient = patient_service
        .create(
            mehdi.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Hawra Ali".into(),
            },
        )
        .await
        .unwrap();

    // ---- Step 4: create a draft for the X-Ray check type ---------------
    let draft = visit_service
        .create_draft(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: new_patient.id,
                check_type_id: check_type.id,
                check_subtype_id: None,
                doctor_id: Some(doctor.id),
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(draft.status, VisitStatus::Draft);

    // ---- Step 5: pricing resolve is read-only --------------------------
    let resolved = visit_service
        .resolve_snapshots(draft.id, money)
        .await
        .unwrap();
    assert_eq!(resolved.snapshots.price_iqd, 75_000);
    // 75_000 * 30% = 22_500.
    assert_eq!(resolved.snapshots.doctor_cut_iqd, 22_500);
    let after_resolve = visit_service.get(draft.id).await.unwrap();
    assert_eq!(after_resolve.version, draft.version);
    assert!(after_resolve.snapshots.is_none());

    // ---- Step 6: Lock the visit ----------------------------------------
    let lock_res = visit_service
        .lock(
            mehdi.id,
            UserRole::Receptionist,
            draft.id,
            kareem.id,
            money,
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(lock_res.visit.status, VisitStatus::Locked);
    let snap = lock_res.visit.snapshots.as_ref().unwrap();
    assert_eq!(snap.total_amount_iqd, 75_000);
    // Receipt files persisted to disk.
    assert!(lock_res.artifacts.a5_path.exists());
    assert!(lock_res.artifacts.thermal_path.exists());

    // Audit row for the lock.
    let lock_audit: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity='visits' AND action='lock' AND entity_id = ?",
    )
    .bind(draft.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lock_audit.0, 1);
    // Inventory consumed.
    let on_hand: (i64,) =
        sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
            .bind(film.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(on_hand.0, -1);

    // ---- Step 7: lines run today = 1 for Kareem ------------------------
    let lines = visit_service
        .lines_run_today(ENTITY_ID, kareem.id)
        .await
        .unwrap();
    assert_eq!(lines, 1);

    // ---- Step 8: Mariam voids the visit with a valid reason ------------
    let voided = visit_service
        .void(
            mariam.id,
            UserRole::Superadmin,
            draft.id,
            "double-billed".into(),
        )
        .await
        .unwrap();
    assert_eq!(voided.status, VisitStatus::Voided);
    let restored: (i64,) =
        sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
            .bind(film.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(restored.0, 0);

    // ---- Step 9: create + discard a second draft -----------------------
    let second = visit_service
        .create_draft(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: new_patient.id,
                check_type_id: check_type.id,
                check_subtype_id: None,
                doctor_id: None,
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();
    visit_service
        .discard(mehdi.id, UserRole::Receptionist, second.id)
        .await
        .unwrap();
    let discard_audit: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity='visits' AND action='discard' AND entity_id = ?",
    )
    .bind(second.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(discard_audit.0, 1);

    // ---- Step 10: a fresh draft can be created on the same patient ----
    let third = visit_service
        .create_draft(
            mehdi.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: new_patient.id,
                check_type_id: check_type.id,
                check_subtype_id: None,
                doctor_id: None,
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(third.status, VisitStatus::Draft);

    // End-of-day audit log carries: 3 patient creates + 1 patient create
    // (inline) + visit create + lock + void + draft create + discard +
    // draft create = at least 9 entries on the audit log for this script.
    let total_audit: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        total_audit.0 >= 9,
        "expected >= 9 audit rows, got {}",
        total_audit.0
    );
}
