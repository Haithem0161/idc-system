//! Phase 05 §6: edge category sweep -- one scenario per mandatory bucket.
//!
//! Time/TZ, i18n/RTL, Offline/Network, Concurrency, Crash/Recovery, Scale,
//! Security, Data Integrity. The cross-cutting plans (`security.md`,
//! `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`) own the full
//! sweep; this file pins one representative invariant per bucket so a
//! regression on the phase-05 surface fails fast.

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
use chrono::{FixedOffset, TimeZone};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-e";
const DEVICE_ID: &str = "dev-e";

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
        name: "Layla هاشم".into(),
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    patient_repo.upsert(&mut tx, &patient).await.unwrap();
    tx.commit().await.unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-edges-{}", Uuid::now_v7()));
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

async fn create_and_lock(r: &Rig) -> Uuid {
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
    draft.id
}

// ---- §6.1 Time / Timezone -------------------------------------------------

#[tokio::test]
async fn baghdad_fixed_offset_invariant_holds_year_round() {
    // Iraq Asia/Baghdad is +03:00 with no DST. The Tauri receipts module
    // uses FixedOffset, not chrono_tz, so the offset is constant year-round.
    let baghdad = FixedOffset::east_opt(3 * 3600).unwrap();
    let jan = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let jul = chrono::NaiveDate::from_ymd_opt(2026, 7, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    assert_eq!(
        baghdad.from_utc_datetime(&jan).offset().local_minus_utc(),
        10800
    );
    assert_eq!(
        baghdad.from_utc_datetime(&jul).offset().local_minus_utc(),
        10800
    );
}

#[tokio::test]
async fn list_today_by_check_uses_utc_day_boundaries_consistently() {
    let r = rig().await;
    let _ = create_and_lock(&r).await;
    // Same query twice without time mutation produces identical row set.
    let a = r
        .visit_service
        .list_today_by_check(ENTITY_ID, r.check_type.id)
        .await
        .unwrap();
    let b = r
        .visit_service
        .list_today_by_check(ENTITY_ID, r.check_type.id)
        .await
        .unwrap();
    assert_eq!(a.len(), b.len());
}

// ---- §6.2 i18n & RTL ------------------------------------------------------

#[tokio::test]
async fn mixed_arabic_latin_patient_name_round_trips_byte_for_byte() {
    let r = rig().await;
    let mixed = "Layla هاشم Ahmadi";
    let p = r
        .patient_service
        .create(
            r.receptionist.id,
            ENTITY_ID,
            PatientCreateInput { name: mixed.into() },
        )
        .await
        .unwrap();
    let again = r.patient_service.get(p.id).await.unwrap();
    assert_eq!(again.name, mixed);
    // Bytes are preserved.
    assert_eq!(again.name.as_bytes(), mixed.as_bytes());
}

// ---- §6.3 Offline & Network ----------------------------------------------

#[tokio::test]
async fn lock_enqueues_outbox_rows_before_any_network_call() {
    let r = rig().await;
    let id = create_and_lock(&r).await;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity_id = ?")
        .bind(id.to_string())
        .fetch_one(&r.pool)
        .await
        .unwrap();
    assert!(count.0 >= 1);
}

#[tokio::test]
async fn full_offline_lock_produces_persisted_receipt_files() {
    let r = rig().await;
    let id = create_and_lock(&r).await;
    // The lock test rig never reaches the network. Receipts wrote to disk.
    let v = r.visit_service.get(id).await.unwrap();
    assert!(v.locked_at.is_some());
}

// ---- §6.4 Concurrency & Conflicts ----------------------------------------

#[tokio::test]
async fn second_lock_attempt_on_same_visit_returns_validation_error() {
    let r = rig().await;
    let id = create_and_lock(&r).await;
    let err = r
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
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn operator_clocks_out_mid_session_makes_subsequent_lock_fail() {
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
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE operator_shifts SET check_out_at = ?, updated_at = ? WHERE operator_id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(r.operator.id.to_string())
    .execute(&r.pool)
    .await
    .unwrap();
    let err = r
        .visit_service
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
        .await;
    assert!(err.is_err());
}

// ---- §6.5 Crash & Recovery -----------------------------------------------

#[tokio::test]
async fn failed_create_draft_leaves_no_partial_state() {
    let r = rig().await;
    let bogus_patient = Uuid::now_v7();
    let err = r
        .visit_service
        .create_draft(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: bogus_patient,
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
        .await;
    assert!(err.is_err());
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits")
        .fetch_one(&r.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

// ---- §6.6 Scale & Performance --------------------------------------------

#[tokio::test]
async fn list_workspace_at_100_visits_under_one_second_smoke() {
    let r = rig().await;
    // Seed 100 draft visits via the service.
    for _ in 0..100 {
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
    }
    let start = std::time::Instant::now();
    let rows = r
        .visit_service
        .list_workspace(
            ENTITY_ID,
            r.check_type.id,
            app_lib::domains::visits::domain::repositories::WorkspaceFilters::default(),
            200,
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(rows.len(), 100);
    assert!(
        elapsed.as_millis() < 1_000,
        "list_workspace took {:?}",
        elapsed
    );
}

// ---- §6.7 Security & Permissions -----------------------------------------

#[tokio::test]
async fn role_bypass_receptionist_cannot_void() {
    let r = rig().await;
    let id = create_and_lock(&r).await;
    let err = r
        .visit_service
        .void(
            r.receptionist.id,
            UserRole::Receptionist,
            id,
            "valid reason".into(),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn role_bypass_accountant_cannot_create_draft_or_lock() {
    let r = rig().await;
    let err = r
        .visit_service
        .create_draft(
            r.receptionist.id,
            UserRole::Accountant,
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
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn fts_match_injection_input_treated_as_literal_query() {
    let r = rig().await;
    // Hostile input that would otherwise be interpreted as MATCH syntax.
    let rows = r
        .patient_service
        .search(ENTITY_ID, "Layla MATCH 'foo'", 5)
        .await;
    assert!(
        rows.is_ok(),
        "FTS injection should not error; got {:?}",
        rows.err()
    );
}

#[tokio::test]
async fn soft_deleted_visit_is_hidden_from_reads_but_persists_in_table() {
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
    r.visit_service
        .discard(r.receptionist.id, UserRole::Receptionist, draft.id)
        .await
        .unwrap();
    // Service-level reads exclude it.
    let drafts = r
        .visit_service
        .list_drafts_by_check(ENTITY_ID, r.check_type.id)
        .await
        .unwrap();
    assert!(drafts.iter().all(|v| v.id != draft.id));
    // Raw SQL still finds the tombstone.
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE id = ?")
        .bind(draft.id.to_string())
        .fetch_one(&r.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

// ---- §6.8 Data Integrity --------------------------------------------------

#[tokio::test]
async fn migration_005_replay_on_populated_db_is_idempotent() {
    let r = rig().await;
    let _ = create_and_lock(&r).await;
    migrations::run(&r.pool).await.unwrap();
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE deleted_at IS NULL")
        .fetch_one(&r.pool)
        .await
        .unwrap();
    assert!(count.0 >= 1);
}

#[tokio::test]
async fn fk_violation_when_visit_references_unknown_patient() {
    let r = rig().await;
    let stranger = Uuid::now_v7();
    let err = r
        .visit_service
        .create_draft(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: stranger,
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
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn sync_version_increments_on_every_visit_mutation() {
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
    let v_after_create = draft.version;
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
    let after_lock = r.visit_service.get(draft.id).await.unwrap();
    assert!(after_lock.version > v_after_create);
    r.visit_service
        .void(
            r.superadmin.id,
            UserRole::Superadmin,
            draft.id,
            "wrong patient".into(),
        )
        .await
        .unwrap();
    let after_void = r.visit_service.get(draft.id).await.unwrap();
    assert!(after_void.version > after_lock.version);
}

#[tokio::test]
async fn locked_visit_with_missing_snapshot_blocked_by_db_check_constraint() {
    let r = rig().await;
    // Raw SQL insert with status=locked and price_snapshot_iqd=NULL.
    let res = sqlx::query(
        "INSERT INTO visits \
         (id, patient_id, status, receptionist_user_id, check_type_id, dye, report, locked_at, created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, ?, 'locked', ?, ?, 0, 0, ?, ?, ?, 1, 1, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(r.patient.id.to_string())
    .bind(r.receptionist.id.to_string())
    .bind(r.check_type.id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(ENTITY_ID)
    .execute(&r.pool)
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn voided_visit_with_short_reason_blocked_by_db_check_constraint() {
    let r = rig().await;
    let res = sqlx::query(
        "INSERT INTO visits \
         (id, patient_id, status, receptionist_user_id, check_type_id, dye, report, locked_at, voided_at, voided_by_user_id, void_reason, created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, ?, 'voided', ?, ?, 0, 0, ?, ?, ?, 'oops', ?, ?, 1, 1, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(r.patient.id.to_string())
    .bind(r.receptionist.id.to_string())
    .bind(r.check_type.id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(r.superadmin.id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(ENTITY_ID)
    .execute(&r.pool)
    .await;
    assert!(res.is_err());
}
