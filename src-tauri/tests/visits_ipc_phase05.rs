//! Phase-05 §2.2 IPC handler / wire-shape coverage.
//!
//! Drives `VisitService` + `PatientService` along the same path each Tauri
//! command takes, then asserts the serialized JSON shape (which is the
//! frontend contract). Pure JSON-shape assertions live here; behaviour is
//! covered by `visits_phase05.rs` / `patients_phase05.rs`.

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
use app_lib::domains::patients::service::{PatientCreateInput, PatientService, PatientUpdateInput};
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
use app_lib::error::AppError;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-x";
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
        dye_supported: true,
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
        item_id: item.id,
        quantity_per_check: 2,
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

    let receipts_dir = std::env::temp_dir().join(format!("idc-ipc-{}", Uuid::now_v7()));
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
        dye_cost_iqd: 2_000,
        report_pct: 20,
        reporting_doctor_name: String::new(),
        internal_doctor_pct: 40,
    }
}

// ---- patients wire-shape --------------------------------------------------

#[tokio::test]
async fn patients_create_response_carries_canonical_keys() {
    let r = rig().await;
    let p = r
        .patient_service
        .create(
            r.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Mariam".into(),
            },
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&p).unwrap();
    let obj = json.as_object().unwrap();
    for key in [
        "id",
        "name",
        "created_at",
        "updated_at",
        "deleted_at",
        "version",
        "dirty",
        "entity_id",
    ] {
        assert!(obj.contains_key(key), "missing key {key}");
    }
}

#[tokio::test]
async fn patients_search_returns_array() {
    let r = rig().await;
    let rows = r.patient_service.search(ENTITY_ID, "Lay", 5).await.unwrap();
    let json = serde_json::to_value(&rows).unwrap();
    assert!(json.is_array());
}

#[tokio::test]
async fn patients_update_response_carries_updated_name_and_bumped_version() {
    let r = rig().await;
    let updated = r
        .patient_service
        .update(
            r.receptionist.id,
            r.patient.id,
            PatientUpdateInput {
                name: "Layla H.".into(),
            },
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&updated).unwrap();
    assert_eq!(json["name"], json!("Layla H."));
    assert!(json["version"].as_i64().unwrap() > 1);
}

// ---- visits wire-shape ----------------------------------------------------

async fn create_draft(r: &Rig) -> Uuid {
    r.visit_service
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
            },
        )
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn visits_create_draft_returns_status_draft_and_snapshot_null() {
    let r = rig().await;
    let id = create_draft(&r).await;
    let v = r.visit_service.get(id).await.unwrap();
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["status"], json!("draft"));
    assert!(json["snapshots"].is_null());
    assert!(json["locked_at"].is_null());
}

#[tokio::test]
async fn visits_lock_returns_visit_and_artifacts_block() {
    let r = rig().await;
    let id = create_draft(&r).await;
    let res = r
        .visit_service
        .lock(
            r.receptionist.id,
            UserRole::Receptionist,
            id,
            r.operator.id,
            None,
            None,
            money(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&res).unwrap();
    assert!(json.get("visit").is_some());
    assert!(json.get("artifacts").is_some());
    let visit = &json["visit"];
    assert_eq!(visit["status"], json!("locked"));
    assert!(!visit["snapshots"].is_null());
    let snaps = &visit["snapshots"];
    for k in [
        "price_iqd",
        "dye_cost_iqd",
        "report_amount_iqd",
        "report_pct",
        "reporting_doctor_name",
        "doctor_cut_iqd",
        "operator_cut_iqd",
        "total_amount_iqd",
        "patient_name",
        "operator_name",
        "check_type_name_ar",
    ] {
        assert!(snaps.get(k).is_some(), "missing snapshot key {k}");
    }
    let artifacts = &json["artifacts"];
    assert!(artifacts.get("a5_path").is_some());
    assert!(artifacts.get("thermal_path").is_some());
}

#[tokio::test]
async fn visits_void_returns_status_voided_with_void_reason_trimmed() {
    let r = rig().await;
    let id = create_draft(&r).await;
    r.visit_service
        .lock(
            r.receptionist.id,
            UserRole::Receptionist,
            id,
            r.operator.id,
            None,
            None,
            money(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    let voided = r
        .visit_service
        .void(
            r.superadmin.id,
            UserRole::Superadmin,
            id,
            "   typo on lock   ".into(),
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&voided).unwrap();
    assert_eq!(json["status"], json!("voided"));
    assert_eq!(json["void_reason"], json!("typo on lock"));
    assert!(!json["voided_at"].is_null());
}

#[tokio::test]
async fn visits_get_for_unknown_id_returns_not_found_envelope() {
    let r = rig().await;
    let err = r.visit_service.get(Uuid::now_v7()).await.unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], json!("NOT_FOUND"));
}

#[tokio::test]
async fn visits_discard_on_locked_returns_validation_envelope() {
    let r = rig().await;
    let id = create_draft(&r).await;
    r.visit_service
        .lock(
            r.receptionist.id,
            UserRole::Receptionist,
            id,
            r.operator.id,
            None,
            None,
            money(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    let err = r
        .visit_service
        .discard(r.receptionist.id, UserRole::Receptionist, id)
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], json!("VALIDATION_ERROR"));
}

#[tokio::test]
async fn visits_qualified_operators_returns_array_of_dto_with_id_name_active() {
    let r = rig().await;
    let ops = r
        .visit_service
        .qualified_operators(ENTITY_ID, r.check_type.id)
        .await
        .unwrap();
    let json = serde_json::to_value(&ops).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 1);
    let row = &json[0];
    assert!(row.get("id").is_some());
    assert!(row.get("name").is_some());
    assert!(row.get("is_active").is_some());
}

#[tokio::test]
async fn visits_pricing_resolve_returns_snapshots_block_only() {
    let r = rig().await;
    let id = create_draft(&r).await;
    let resolved = r
        .visit_service
        .resolve_snapshots(id, money())
        .await
        .unwrap();
    let json = serde_json::to_value(&resolved).unwrap();
    assert!(json.get("snapshots").is_some());
    assert_eq!(json["snapshots"]["price_iqd"], json!(50_000));
}

#[tokio::test]
async fn visits_lock_with_wrong_role_returns_validation_envelope() {
    let r = rig().await;
    let id = create_draft(&r).await;
    let err = r
        .visit_service
        .lock(
            r.receptionist.id,
            UserRole::Accountant,
            id,
            r.operator.id,
            None,
            None,
            money(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], json!("VALIDATION_ERROR"));
}

#[tokio::test]
async fn shifts_lines_run_today_returns_i64_zero_for_no_visits() {
    let r = rig().await;
    let count = r
        .visit_service
        .lines_run_today(ENTITY_ID, r.operator.id)
        .await
        .unwrap();
    let json = serde_json::to_value(count).unwrap();
    assert_eq!(json, json!(0));
}

#[tokio::test]
async fn visits_checks_grid_returns_array_of_cards_with_today_count() {
    let r = rig().await;
    let grid = r.visit_service.checks_grid(ENTITY_ID).await.unwrap();
    let json = serde_json::to_value(&grid).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());
    let row = &arr[0];
    for k in [
        "check_type_id",
        "name_ar",
        "has_subtypes",
        "dye_supported",
        "todays_visits",
    ] {
        assert!(row.get(k).is_some(), "missing key {k}");
    }
}

#[tokio::test]
async fn app_error_envelope_serializes_for_every_phase05_kind() {
    for err in [
        AppError::Validation("v".into()),
        AppError::NotFound("v".into()),
        AppError::Conflict("v".into()),
        AppError::NotAuthenticated,
        AppError::Configuration("v".into()),
    ] {
        let json = serde_json::to_value(&err).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("code"));
        assert!(obj.contains_key("message"));
    }
}

#[tokio::test]
async fn visits_list_workspace_returns_array_with_correct_status_filter() {
    let r = rig().await;
    let id = create_draft(&r).await;
    r.visit_service
        .lock(
            r.receptionist.id,
            UserRole::Receptionist,
            id,
            r.operator.id,
            None,
            None,
            money(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    let rows = r
        .visit_service
        .list_workspace(
            ENTITY_ID,
            r.check_type.id,
            app_lib::domains::visits::domain::repositories::WorkspaceFilters {
                statuses: vec![VisitStatus::Locked],
                doctor_ids: vec![],
                subtype_ids: vec![],
                from: None,
                to: None,
            },
            50,
        )
        .await
        .unwrap();
    let json = serde_json::to_value(&rows).unwrap();
    assert!(json.is_array());
    assert!(rows.iter().any(|v| v.id == id));
    assert!(rows.iter().all(|v| v.status == VisitStatus::Locked));
}

#[tokio::test]
async fn visits_list_today_by_check_returns_empty_when_check_type_unknown() {
    let r = rig().await;
    let rows = r
        .visit_service
        .list_today_by_check(ENTITY_ID, Uuid::now_v7())
        .await
        .unwrap();
    let json = serde_json::to_value(&rows).unwrap();
    assert_eq!(json, json!([]));
}
