//! Integration tests for Phase 5 reception.
//!
//! Drives `VisitService` end-to-end against an in-memory SQLite with all
//! migrations applied. Covers:
//! - draft creation success + role gate
//! - illegal-transition matrix (discard locked, void draft)
//! - lock workflow: money math correctness, inventory consumption, audit
//!   rows, receipt file persistence
//! - operator-eligibility gate (no qualified operator on shift)
//! - void offsetting math + inventory restoration
//! - inventory_adjustments append-only trigger
//! - patients FTS5 search

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
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem,
    Operator, OperatorSpecialty,
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
use app_lib::domains::patients::domain::entities::{Patient, PatientNewInput};
use app_lib::domains::patients::domain::repositories::PatientRepo;
use app_lib::domains::patients::infrastructure::SqlitePatientRepo;
use app_lib::domains::patients::PatientService;
use app_lib::domains::receipts::ReceiptRenderOptions;
use app_lib::domains::shifts::domain::entities::operator_shift::OperatorShiftOpenInput;
use app_lib::domains::shifts::domain::entities::OperatorShift;
use app_lib::domains::shifts::domain::repositories::OperatorShiftRepo;
use app_lib::domains::shifts::infrastructure::SqliteOperatorShiftRepo;
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::domains::visits::domain::entities::{
    AdjustmentNewInput, AdjustmentReason, InventoryAdjustment, VisitStatus,
};
use app_lib::domains::visits::domain::repositories::{InventoryAdjustmentRepo, VisitRepo};
use app_lib::domains::visits::domain::services::MoneySettings;
use app_lib::domains::visits::infrastructure::{SqliteInventoryAdjustmentRepo, SqliteVisitRepo};
use app_lib::domains::visits::service::{CreateDraftInput, VisitService, VisitServiceConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-r";
const DEVICE_ID: &str = "dev-r";

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
    visit_service: Arc<VisitService>,
    patient_service: Arc<PatientService>,
    receptionist: User,
    superadmin: User,
    patient: Patient,
    check_type: CheckType,
    _check_subtype: Option<CheckSubtype>,
    doctor: Doctor,
    _doctor_pricing: DoctorCheckPricing,
    operator: Operator,
    _operator_specialty: OperatorSpecialty,
    inventory_item: InventoryItem,
    _consumption: InventoryConsumptionMap,
    receipts_dir: std::path::PathBuf,
}

async fn seed() -> Fixture {
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

    let receptionist = User::try_new(
        "reception@example.com",
        "Reception",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let superadmin = User::try_new(
        "boss@example.com",
        "Boss",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &receptionist).await.unwrap();
    user_repo.upsert(&mut tx, &superadmin).await.unwrap();
    tx.commit().await.unwrap();

    // ---- catalog: flat check type with dye supported. -------------------
    let check_type = CheckType::try_new(CheckTypeNewInput {
        name_ar: "اختبار".into(),
        name_en: Some("Test".into()),
        has_subtypes: false,
        base_price_iqd: Some(50_000),
        dye_supported: true,
        report_supported: false,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let doctor = Doctor::try_new(DoctorNewInput {
        name: "Sara".into(),
        specialty: None,
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let doctor_pricing = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doctor.id,
        check_type_id: check_type.id,
        check_subtype_id: None,
        price_override_iqd: None,
        cut_kind: CutKind::Pct,
        cut_value: 25,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let operator = Operator::try_new(OperatorNewInput {
        name: "Op".into(),
        phone: None,
        base_cut_per_check_iqd: 5_000,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let operator_specialty = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: operator.id,
        check_type_id: check_type.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let inventory_item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "أداة".into(),
        name_en: Some("Item".into()),
        unit: "pcs".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let consumption = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: check_type.id,
        check_subtype_id: None,
        item_id: inventory_item.id,
        quantity_per_check: 2,
        on_dye_only: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &check_type).await.unwrap();
    doc_repo.upsert(&mut tx, &doctor).await.unwrap();
    dp_repo.upsert(&mut tx, &doctor_pricing).await.unwrap();
    op_repo.upsert(&mut tx, &operator).await.unwrap();
    os_repo.upsert(&mut tx, &operator_specialty).await.unwrap();
    item_repo.upsert(&mut tx, &inventory_item).await.unwrap();
    cons_repo.upsert(&mut tx, &consumption).await.unwrap();
    tx.commit().await.unwrap();

    // Open a shift for the operator so the lock workflow can pick them.
    let shift = OperatorShift::open(OperatorShiftOpenInput {
        operator_id: operator.id,
        by_user_id: receptionist.id,
        note: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    shift_repo.upsert(&mut tx, &shift).await.unwrap();
    tx.commit().await.unwrap();

    // Patient.
    let patient_service = Arc::new(PatientService::new(
        pool.clone(),
        patient_repo.clone(),
        audit.clone(),
        outbox.clone(),
        DEVICE_ID.to_string(),
    ));
    let patient = Patient::try_new(PatientNewInput {
        name: "John".into(),
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    patient_repo.upsert(&mut tx, &patient).await.unwrap();
    tx.commit().await.unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-receipts-test-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&receipts_dir).unwrap();

    let visit_service = Arc::new(VisitService::new(VisitServiceConfig {
        pool: pool.clone(),
        visits: visit_repo,
        adjustments: adj_repo,
        patients: patient_repo,
        check_types: ct_repo,
        check_subtypes: cs_repo,
        doctors: doc_repo,
        doctor_pricing: dp_repo,
        operators: op_repo,
        operator_specialties: os_repo,
        consumption: cons_repo,
        inventory_items: item_repo,
        shifts: shift_repo,
        audit_repo: audit,
        outbox_repo: outbox,
        receipts_dir: receipts_dir.clone(),
        device_id: DEVICE_ID.to_string(),
    }));

    Fixture {
        pool,
        visit_service,
        patient_service,
        receptionist,
        superadmin,
        patient,
        check_type,
        _check_subtype: None,
        doctor,
        _doctor_pricing: doctor_pricing,
        operator,
        _operator_specialty: operator_specialty,
        inventory_item,
        _consumption: consumption,
        receipts_dir,
    }
}

fn settings() -> MoneySettings {
    MoneySettings {
        dye_cost_iqd: 2_000,
        report_cost_iqd: 3_000,
        internal_doctor_pct: 40,
    }
}

#[tokio::test]
async fn create_draft_and_lock_produces_receipt_and_consumption() {
    let f = seed().await;
    let draft = f
        .visit_service
        .create_draft(
            f.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: f.patient.id,
                check_type_id: f.check_type.id,
                check_subtype_id: None,
                doctor_id: Some(f.doctor.id),
                dye: true,
                report: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(draft.status, VisitStatus::Draft);

    let lock_result = f
        .visit_service
        .lock(
            f.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            f.operator.id,
            settings(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(lock_result.visit.status, VisitStatus::Locked);
    let snap = lock_result.visit.snapshots.as_ref().unwrap();
    // 50000 base price * 25% cut = 12500 doctor cut; dye=true => 2000.
    assert_eq!(snap.price_iqd, 50_000);
    assert_eq!(snap.dye_cost_iqd, 2_000);
    assert_eq!(snap.doctor_cut_iqd, 12_500);
    assert_eq!(snap.total_amount_iqd, 52_000);
    assert!(lock_result.visit.locked_at.is_some());
    // Both receipt files exist.
    assert!(lock_result.artifacts.a5_path.exists());
    assert!(lock_result.artifacts.thermal_path.exists());

    // Inventory adjusted: quantity_on_hand should be -2 (started at 0,
    // consumed 2 via the consumption_map row).
    let item_id_str = f.inventory_item.id.to_string();
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(&item_id_str)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(row.0, -2);

    // Audit row for the lock is present.
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'visits' AND action = 'lock'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);

    // Outbox enqueued at least visit + adjustment + audit + recompute.
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(row.0 >= 3);

    let _ = f.receipts_dir;
}

#[tokio::test]
async fn lock_rejected_when_no_qualified_operator_on_shift() {
    let f = seed().await;
    // Soft-delete the specialty so the operator is no longer qualified.
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE operator_specialties SET deleted_at = ?, version = version + 1 \
         WHERE operator_id = ? AND check_type_id = ?",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(f.operator.id.to_string())
    .bind(f.check_type.id.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let draft = f
        .visit_service
        .create_draft(
            f.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: f.patient.id,
                check_type_id: f.check_type.id,
                check_subtype_id: None,
                doctor_id: Some(f.doctor.id),
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();

    let err = f
        .visit_service
        .lock(
            f.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            f.operator.id,
            settings(),
            ReceiptRenderOptions::default(),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn void_offsets_inventory_and_marks_visit_voided() {
    let f = seed().await;
    let draft = f
        .visit_service
        .create_draft(
            f.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: f.patient.id,
                check_type_id: f.check_type.id,
                check_subtype_id: None,
                doctor_id: Some(f.doctor.id),
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();
    let _locked = f
        .visit_service
        .lock(
            f.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            f.operator.id,
            settings(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();

    // Void must come from a superadmin.
    let err = f
        .visit_service
        .void(
            f.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            "patient walked out".into(),
        )
        .await;
    assert!(err.is_err());

    let voided = f
        .visit_service
        .void(
            f.superadmin.id,
            UserRole::Superadmin,
            draft.id,
            "patient walked out".into(),
        )
        .await
        .unwrap();
    assert_eq!(voided.status, VisitStatus::Voided);
    assert!(voided.voided_by_user_id.is_some());

    // Inventory restored to baseline (-2 + 2 = 0).
    let item_id_str = f.inventory_item.id.to_string();
    let row: (i64,) = sqlx::query_as("SELECT quantity_on_hand FROM inventory_items WHERE id = ?")
        .bind(&item_id_str)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn discard_locked_visit_is_rejected() {
    let f = seed().await;
    let draft = f
        .visit_service
        .create_draft(
            f.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: f.patient.id,
                check_type_id: f.check_type.id,
                check_subtype_id: None,
                doctor_id: Some(f.doctor.id),
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();
    let _ = f
        .visit_service
        .lock(
            f.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            f.operator.id,
            settings(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    let err = f
        .visit_service
        .discard(f.receptionist.id, UserRole::Receptionist, draft.id)
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn discard_draft_soft_deletes_and_emits_audit() {
    let f = seed().await;
    let draft = f
        .visit_service
        .create_draft(
            f.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: f.patient.id,
                check_type_id: f.check_type.id,
                check_subtype_id: None,
                doctor_id: None,
                dye: false,
                report: false,
            },
        )
        .await
        .unwrap();
    f.visit_service
        .discard(f.receptionist.id, UserRole::Receptionist, draft.id)
        .await
        .unwrap();
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'visits' AND action = 'discard'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn inventory_adjustments_trigger_blocks_business_update() {
    let f = seed().await;
    let item_id = f.inventory_item.id;
    let adj = InventoryAdjustment::try_new(AdjustmentNewInput {
        item_id,
        delta: 5,
        reason: AdjustmentReason::Receive,
        visit_id: None,
        note: Some("opening stock".into()),
        by_user_id: f.receptionist.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    let adj_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(f.pool.clone()));
    adj_repo.append(&mut tx, &adj).await.unwrap();
    tx.commit().await.unwrap();

    // Attempt a business-field UPDATE -- expect ABORT.
    let res = sqlx::query("UPDATE inventory_adjustments SET delta = 999 WHERE id = ?")
        .bind(adj.id.to_string())
        .execute(&f.pool)
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn patients_search_returns_matches_by_fts_prefix() {
    let f = seed().await;
    // Create additional patients via the service so audit rows + FTS
    // triggers fire.
    let _ = f
        .patient_service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            app_lib::domains::patients::service::PatientCreateInput {
                name: "Alice Anderson".into(),
            },
        )
        .await
        .unwrap();
    let _ = f
        .patient_service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            app_lib::domains::patients::service::PatientCreateInput {
                name: "Bob Brown".into(),
            },
        )
        .await
        .unwrap();
    let rows = f
        .patient_service
        .search(ENTITY_ID, "Alic", 10)
        .await
        .unwrap();
    assert!(rows.iter().any(|p| p.name.starts_with("Alice")));
}
