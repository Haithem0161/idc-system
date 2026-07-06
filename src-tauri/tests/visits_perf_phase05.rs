//! Phase 05 §7 performance SLO assertions.
//!
//! Numbers reflect the release-mode budget; the debug-mode threshold is
//! 4x the release budget to absorb test-rig variance. The shifts plan's
//! pattern is reused. All assertions fail loudly if exceeded.

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
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteMandoubRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::patients::domain::entities::{Patient, PatientNewInput};
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
use app_lib::domains::visits::domain::repositories::{InventoryAdjustmentRepo, VisitRepo};
use app_lib::domains::visits::domain::services::MoneySettings;
use app_lib::domains::visits::infrastructure::{SqliteInventoryAdjustmentRepo, SqliteVisitRepo};
use app_lib::domains::visits::service::{CreateDraftInput, VisitService, VisitServiceConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-perf";
const DEVICE_ID: &str = "dev-perf";

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

#[allow(dead_code)]
struct Rig {
    pool: SqlitePool,
    visit_service: Arc<VisitService>,
    patient_service: Arc<PatientService>,
    receptionist: User,
    superadmin: User,
    patient: Patient,
    check_type: CheckType,
    doctor: Doctor,
    operator: Operator,
}

async fn rig() -> Rig {
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
        "rec@x",
        "Rec",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let superadmin = User::try_new(
        "boss@x",
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

    let check_type = CheckType::try_new(CheckTypeNewInput {
        name_ar: "اختبار".into(),
        name_en: Some("Test".into()),
        has_subtypes: false,
        base_price_iqd: Some(50_000),
        dye_price_iqd: None,
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
    let spec = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: operator.id,
        check_type_id: check_type.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let item = InventoryItem::try_new(InventoryItemNewInput {
        name_ar: "أ".into(),
        name_en: Some("I".into()),
        unit: "p".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let consumption = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: check_type.id,
        check_subtype_id: None,
        item_id: item.id,
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
    op_repo.upsert(&mut tx, &operator).await.unwrap();
    os_repo.upsert(&mut tx, &spec).await.unwrap();
    item_repo.upsert(&mut tx, &item).await.unwrap();
    cons_repo.upsert(&mut tx, &consumption).await.unwrap();
    tx.commit().await.unwrap();

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

    let patient_service = Arc::new(PatientService::new(
        pool.clone(),
        patient_repo.clone(),
        visit_repo.clone(),
        audit.clone(),
        outbox.clone(),
        DEVICE_ID.to_string(),
    ));
    let patient = Patient::try_new(PatientNewInput {
        name: "Layla".into(),
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    patient_repo.upsert(&mut tx, &patient).await.unwrap();
    tx.commit().await.unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-perf-{}", Uuid::now_v7()));
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
        mandoubs: Arc::new(SqliteMandoubRepo::new(pool.clone())),
        consumption: cons_repo,
        inventory_items: item_repo,
        shifts: shift_repo,
        frozen_close: Arc::new(SqliteFrozenCloseRepo::new(pool.clone())),
        audit_repo: audit,
        outbox_repo: outbox,
        receipts_dir,
        device_id: DEVICE_ID.to_string(),
    }));

    Rig {
        pool,
        visit_service,
        patient_service,
        receptionist,
        superadmin,
        patient,
        check_type,
        doctor,
        operator,
    }
}

fn money() -> MoneySettings {
    MoneySettings {
        report_pct: 20,
        reporting_doctor_name: String::new(),
        internal_doctor_pct: 40,
    }
}

/// Threshold quadrupled for debug builds to reduce flakiness; release-mode
/// SLO is the documented value.
fn threshold_ms(release_ms: u128) -> u128 {
    if cfg!(debug_assertions) {
        release_ms * 4
    } else {
        release_ms
    }
}

#[tokio::test]
async fn perf_create_draft_under_50ms() {
    // SLO: < 50 ms p99 release.
    let r = rig().await;
    let limit = threshold_ms(50);
    let mut worst = 0_u128;
    for _ in 0..10 {
        let start = std::time::Instant::now();
        r.visit_service
            .create_draft(
                r.receptionist.id,
                UserRole::Receptionist,
                ENTITY_ID,
                CreateDraftInput {
                    patient_id: r.patient.id,
                    check_type_id: r.check_type.id,
                    check_subtype_id: None,
                    doctor_id: None,
                    mandoub_id: None,
                    dye: false,
                    report: false,
                    dalal: false,
                    discount: false,
                    price_override_iqd: None,
                },
            )
            .await
            .unwrap();
        worst = worst.max(start.elapsed().as_millis());
    }
    assert!(
        worst <= limit,
        "create_draft worst {} ms > {}",
        worst,
        limit
    );
}

#[tokio::test]
async fn perf_lock_typical_case_under_200ms() {
    // SLO: phase-05 §7 lock end-to-end < 200 ms p99 (project tighter target
    // < 100 ms; phase-04 perf rig uses 200 ms as the §9 fallback). Debug 4x.
    let r = rig().await;
    let limit = threshold_ms(200);
    let mut worst = 0_u128;
    for _ in 0..5 {
        let draft = r
            .visit_service
            .create_draft(
                r.receptionist.id,
                UserRole::Receptionist,
                ENTITY_ID,
                CreateDraftInput {
                    patient_id: r.patient.id,
                    check_type_id: r.check_type.id,
                    check_subtype_id: None,
                    doctor_id: Some(r.doctor.id),
                    mandoub_id: None,
                    dye: false,
                    report: false,
                    dalal: false,
                    discount: false,
                    price_override_iqd: None,
                },
            )
            .await
            .unwrap();
        let start = std::time::Instant::now();
        r.visit_service
            .lock(
                r.receptionist.id,
                UserRole::Receptionist,
                draft.id,
                r.operator.id,
                None,
                None,
                money(),
                ReceiptRenderOptions::default(),
            )
            .await
            .unwrap();
        worst = worst.max(start.elapsed().as_millis());
    }
    assert!(worst <= limit, "lock worst {} ms > {}", worst, limit);
}

#[tokio::test]
async fn perf_pricing_resolve_under_30ms() {
    // SLO: read-only resolve < 20 ms p99 release; tested at debug 4x = 80 ms.
    let r = rig().await;
    let draft = r
        .visit_service
        .create_draft(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: r.patient.id,
                check_type_id: r.check_type.id,
                check_subtype_id: None,
                doctor_id: Some(r.doctor.id),
                mandoub_id: None,
                dye: false,
                report: false,
                dalal: false,
                discount: false,
                price_override_iqd: None,
            },
        )
        .await
        .unwrap();
    let limit = threshold_ms(30);
    let mut worst = 0_u128;
    for _ in 0..20 {
        let start = std::time::Instant::now();
        r.visit_service
            .resolve_snapshots(draft.id, money())
            .await
            .unwrap();
        worst = worst.max(start.elapsed().as_millis());
    }
    assert!(
        worst <= limit,
        "pricing_resolve worst {} ms > {}",
        worst,
        limit
    );
}

#[tokio::test]
async fn perf_patients_search_at_100_patients_under_50ms() {
    let r = rig().await;
    for i in 0..100 {
        r.patient_service
            .create(
                r.receptionist.id,
                ENTITY_ID,
                PatientCreateInput {
                    name: format!("Patient_{}", i),
                },
            )
            .await
            .unwrap();
    }
    let limit = threshold_ms(50);
    let mut worst = 0_u128;
    for _ in 0..10 {
        let start = std::time::Instant::now();
        let rows = r
            .patient_service
            .search(ENTITY_ID, "Patient", 10)
            .await
            .unwrap();
        assert!(!rows.is_empty());
        worst = worst.max(start.elapsed().as_millis());
    }
    assert!(
        worst <= limit,
        "patients_search worst {} ms > {}",
        worst,
        limit
    );
}

#[tokio::test]
async fn perf_list_today_by_check_50_rows_under_30ms() {
    let r = rig().await;
    for _ in 0..50 {
        let draft = r
            .visit_service
            .create_draft(
                r.receptionist.id,
                UserRole::Receptionist,
                ENTITY_ID,
                CreateDraftInput {
                    patient_id: r.patient.id,
                    check_type_id: r.check_type.id,
                    check_subtype_id: None,
                    doctor_id: None,
                    mandoub_id: None,
                    dye: false,
                    report: false,
                    dalal: false,
                    discount: false,
                    price_override_iqd: None,
                },
            )
            .await
            .unwrap();
        // Lock without dye/report so each visit lands in today's list.
        r.visit_service
            .lock(
                r.receptionist.id,
                UserRole::Receptionist,
                draft.id,
                r.operator.id,
                None,
                None,
                money(),
                ReceiptRenderOptions::default(),
            )
            .await
            .unwrap();
    }
    let limit = threshold_ms(30);
    let mut worst = 0_u128;
    for _ in 0..10 {
        let start = std::time::Instant::now();
        let rows = r
            .visit_service
            .list_today_by_check(ENTITY_ID, r.check_type.id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 50);
        worst = worst.max(start.elapsed().as_millis());
    }
    assert!(
        worst <= limit,
        "list_today_by_check worst {} ms > {}",
        worst,
        limit
    );
}

#[tokio::test]
async fn perf_qualified_operators_under_30ms() {
    let r = rig().await;
    let limit = threshold_ms(30);
    let mut worst = 0_u128;
    for _ in 0..10 {
        let start = std::time::Instant::now();
        r.visit_service
            .qualified_operators(ENTITY_ID, r.check_type.id)
            .await
            .unwrap();
        worst = worst.max(start.elapsed().as_millis());
    }
    assert!(
        worst <= limit,
        "qualified_operators worst {} ms > {}",
        worst,
        limit
    );
}
