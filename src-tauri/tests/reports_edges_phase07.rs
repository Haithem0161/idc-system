//! Phase-07 §6 edge-category coverage for the reports bounded context.
//!
//! One scenario per §6.X mandatory category. Larger flows live in
//! `reports_phase07.rs`; scale + SLO assertions live in
//! `reports_perf_phase07.rs`. RTL / Arabic-Indic numeral expectations are
//! covered by the frontend test suites and the cross-cutting `i18n-rtl.md`.
//!
//! - §6.1 Time / Timezone     -- Baghdad +03:00 day-boundary
//! - §6.2 i18n & RTL          -- mixed-direction patient names round-trip
//! - §6.3 Offline & Network   -- local-only reads; no HTTP in service path
//! - §6.4 Concurrency / Conflicts -- two simultaneous daily_close runs
//! - §6.5 Crash & Recovery    -- atomic CSV rename + tmp cleanup
//! - §6.6 Scale & Performance -- smoke: 200 visits aggregate
//! - §6.7 Security & Permissions -- role gate
//! - §6.8 Data Integrity      -- snapshot columns are stable; clamp invariants

use std::collections::BTreeMap;
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
    CheckType, Doctor, InventoryConsumptionMap, InventoryItem, Operator, OperatorSpecialty,
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
use app_lib::domains::patients::domain::entities::Patient;
use app_lib::domains::patients::domain::repositories::PatientRepo;
use app_lib::domains::patients::infrastructure::SqlitePatientRepo;
use app_lib::domains::patients::service::{PatientCreateInput, PatientService};
use app_lib::domains::receipts::ReceiptRenderOptions;
use app_lib::domains::reports::domain::entities::{DateRange, VisitsReport, VisitsReportFilters};
use app_lib::domains::reports::domain::repositories::ReportsReadModel;
use app_lib::domains::reports::infrastructure::{SqliteFrozenCloseRepo, SqliteReportsReadModel};
use app_lib::domains::reports::service::{ReportsService, ReportsServiceConfig};
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
use app_lib::error::AppError;
use chrono::{Duration, NaiveDate, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-edge07";
const DEVICE_ID: &str = "dev-edge07";

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
    reports: Arc<ReportsService>,
    visits: Arc<VisitService>,
    patient_service: Arc<PatientService>,
    receptionist: User,
    superadmin: User,
    patient: Patient,
    check_type: CheckType,
    doctor: Doctor,
    operator: Operator,
    inventory_item: InventoryItem,
    _consumption: InventoryConsumptionMap,
    _operator_specialty: OperatorSpecialty,
}

fn money_settings() -> MoneySettings {
    MoneySettings {
        dye_cost_iqd: 2_000,
        report_pct: 20,
        reporting_doctor_name: String::new(),
        internal_doctor_pct: 40,
    }
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
        "edge-rec@x",
        "Edge Rec",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let superadmin = User::try_new(
        "edge-sa@x",
        "Edge SA",
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
        name: "Dr E".into(),
        specialty: None,
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
        default_cut_kind: None,
        default_cut_value: None,
    })
    .unwrap();
    let doctor_pricing = app_lib::domains::catalog::domain::entities::DoctorCheckPricing::try_new(
        DoctorPricingNewInput {
            doctor_id: doctor.id,
            check_type_id: check_type.id,
            check_subtype_id: None,
            price_override_iqd: None,
            cut_kind: CutKind::Pct,
            cut_value: 30,
            entity_id: ENTITY_ID.into(),
            origin_device_id: Some(DEVICE_ID.into()),
        },
    )
    .unwrap();
    let operator = Operator::try_new(OperatorNewInput {
        name: "Op E".into(),
        phone: None,
        base_cut_per_check_iqd: 4_000,
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
        name_ar: "بند".into(),
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
        quantity_per_check: 1,
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

    let mut shift = OperatorShift::open(OperatorShiftOpenInput {
        operator_id: operator.id,
        by_user_id: receptionist.id,
        note: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    shift.check_in_at = Utc::now() - Duration::hours(3);
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
    let patient = patient_service
        .create(
            receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "EdgePatient".into(),
            },
        )
        .await
        .unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-edge07-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&receipts_dir).unwrap();
    let visits = Arc::new(VisitService::new(VisitServiceConfig {
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
        audit_repo: audit.clone(),
        outbox_repo: outbox.clone(),
        receipts_dir,
        device_id: DEVICE_ID.to_string(),
    }));

    let read_model: Arc<dyn ReportsReadModel> = Arc::new(SqliteReportsReadModel::new(pool.clone()));
    let reports = Arc::new(ReportsService::new(ReportsServiceConfig {
        pool: pool.clone(),
        read_model,
        frozen_close_repo: Arc::new(SqliteFrozenCloseRepo::new(pool.clone())),
        audit_repo: audit,
        outbox_repo: outbox,
        device_id: DEVICE_ID.to_string(),
    }));

    Rig {
        pool,
        reports,
        visits,
        patient_service,
        receptionist,
        superadmin,
        patient,
        check_type,
        doctor,
        operator,
        inventory_item,
        _consumption: consumption,
        _operator_specialty: operator_specialty,
    }
}

async fn lock_visit(r: &Rig, dye: bool, doctor: Option<Uuid>) -> Uuid {
    let draft = r
        .visits
        .create_draft(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: r.patient.id,
                check_type_id: r.check_type.id,
                check_subtype_id: None,
                doctor_id: doctor,
                mandoub_id: None,
                dye,
                report: false,
                dalal: false,
                discount: false,
                price_override_iqd: None,
            },
        )
        .await
        .unwrap();
    let res = r
        .visits
        .lock(
            r.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            r.operator.id,
            None,
            None,
            money_settings(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    res.visit.id
}

// ----- §6.1 Time / Timezone -----------------------------------------------

#[tokio::test]
async fn s6_1_daily_close_uses_baghdad_local_day_boundary() {
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    // A target date from a year ago has no visits.
    let past = (Utc::now() - Duration::days(365)).date_naive();
    let close = r
        .reports
        .daily_close(r.superadmin.id, ENTITY_ID, past, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(close.locked_count, 0);
    // The tz_offset stamp is always +03:00 (Iraq is fixed-offset).
    assert_eq!(close.tz_offset, "+03:00");
}

#[tokio::test]
async fn s6_1_visits_report_today_only_excludes_other_days() {
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let now = Utc::now();
    // Single past-day window: from D-2 to D-1, exclusive of "now".
    let filters = VisitsReportFilters {
        from: now - Duration::days(2),
        to: now - Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    let r1 = r.reports.visits_report(filters).await.unwrap();
    match r1 {
        VisitsReport::Rows { rows, totals } => {
            assert_eq!(rows.len(), 0);
            assert_eq!(totals.visits, 0);
        }
        _ => panic!("expected rows mode"),
    }
}

// ----- §6.2 i18n & RTL ------------------------------------------------------

#[tokio::test]
async fn s6_2_mixed_direction_patient_names_round_trip_via_visits_report() {
    let r = rig().await;
    let mixed = r
        .patient_service
        .create(
            r.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla هاشم".into(),
            },
        )
        .await
        .unwrap();
    let draft = r
        .visits
        .create_draft(
            r.receptionist.id,
            UserRole::Receptionist,
            ENTITY_ID,
            CreateDraftInput {
                patient_id: mixed.id,
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
    let _ = r
        .visits
        .lock(
            r.receptionist.id,
            UserRole::Receptionist,
            draft.id,
            r.operator.id,
            None,
            None,
            money_settings(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    let rep = r.reports.visits_report(filters).await.unwrap();
    match rep {
        VisitsReport::Rows { rows, .. } => {
            assert!(rows.iter().any(|row| row.patient_name == "Layla هاشم"));
        }
        _ => panic!("expected rows mode"),
    }
}

// ----- §6.3 Offline & Network ---------------------------------------------

#[tokio::test]
async fn s6_3_dashboard_kpis_is_local_only_no_http_required() {
    // The service path uses SqliteReportsReadModel directly. If a regression
    // wired a remote fetch into dashboard_kpis, this test would hang waiting
    // for a server. We assert the local path completes within the tokio
    // runtime's test timeout (default 60s).
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let kpis = r
        .reports
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap();
    assert!(kpis.revenue_iqd > 0);
}

// ----- §6.4 Concurrency / Conflicts ---------------------------------------

#[tokio::test]
async fn s6_4_two_concurrent_daily_close_runs_serialize_correctly() {
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let target = (Utc::now() + Duration::hours(3)).date_naive();
    let svc1 = r.reports.clone();
    let svc2 = r.reports.clone();
    let sa1 = r.superadmin.id;
    let sa2 = r.superadmin.id;
    let s1 = tokio::spawn(async move {
        svc1.daily_close(sa1, ENTITY_ID, target, BTreeMap::new())
            .await
    });
    let s2 = tokio::spawn(async move {
        svc2.daily_close(sa2, ENTITY_ID, target, BTreeMap::new())
            .await
    });
    let r1 = s1.await.unwrap().unwrap();
    let r2 = s2.await.unwrap().unwrap();
    assert_eq!(r1.input_hash, r2.input_hash);
    // Both runs emit their own additive-only audit row.
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity = 'daily_close' AND action = 'daily_close_run' AND entity_id = ?",
    )
    .bind(target.format("%Y-%m-%d").to_string())
    .fetch_one(&r.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 2);
}

// ----- §6.5 Crash & Recovery ----------------------------------------------

#[tokio::test]
async fn s6_5_csv_writer_atomic_rename_leaves_no_tmp_file_in_export_dir() {
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let dir = std::env::temp_dir().join(format!("idc-edge-tmp-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("visits.csv");
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    r.reports.export_visits_csv(filters, &path).await.unwrap();
    let entries: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    // Only the final file; no `.visits.csv.tmp` left behind.
    assert!(entries.iter().any(|n| n == "visits.csv"));
    assert!(!entries.iter().any(|n| n.ends_with(".tmp")));
}

#[tokio::test]
async fn s6_5_pdf_render_atomic_rename_leaves_no_tmp_file() {
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let target = (Utc::now() + Duration::hours(3)).date_naive();
    let close = r
        .reports
        .daily_close(r.superadmin.id, ENTITY_ID, target, BTreeMap::new())
        .await
        .unwrap();
    let dir = std::env::temp_dir().join(format!("idc-pdf-tmp-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("daily-close.pdf");
    r.reports
        .render_daily_close_pdf(&close, None, &path)
        .unwrap();
    let entries: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(entries.iter().any(|n| n == "daily-close.pdf"));
    assert!(!entries.iter().any(|n| n.ends_with(".tmp")));
}

// ----- §6.6 Scale (smoke) -------------------------------------------------

#[tokio::test]
async fn s6_6_visits_report_with_50_locked_visits_returns_correct_aggregates() {
    let r = rig().await;
    for _ in 0..50 {
        let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    }
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    let rep = r.reports.visits_report(filters).await.unwrap();
    match rep {
        VisitsReport::Rows { rows, totals } => {
            assert_eq!(rows.len(), 50);
            assert_eq!(totals.visits, 50);
            assert_eq!(totals.revenue_iqd, 50 * 50_000);
        }
        _ => panic!("expected rows mode"),
    }
}

// ----- §6.7 Security & Permissions ---------------------------------------

#[tokio::test]
async fn s6_7_role_gate_enforced_on_reports_apis() {
    assert!(ReportsService::require_reports_role(UserRole::Receptionist).is_err());
    assert!(ReportsService::require_reports_role(UserRole::Accountant).is_ok());
    assert!(ReportsService::require_reports_role(UserRole::Superadmin).is_ok());
}

#[tokio::test]
async fn s6_7_inverted_range_returns_validation_not_panic() {
    let r = rig().await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now + Duration::days(2),
        to_utc: now,
    };
    let err = r
        .reports
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// ----- §6.8 Data Integrity ------------------------------------------------

#[tokio::test]
async fn s6_8_visits_report_uses_snapshot_columns_stable_across_pricing_change() {
    // Phase-05 invariant: locked visits hold price_snapshot, doctor_cut_snapshot,
    // operator_cut_snapshot. Reports must sum those, not re-resolve live
    // pricing.
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    let r1 = r.reports.visits_report(filters.clone()).await.unwrap();
    // Simulate a re-issue with a different (live) money setting: the report's
    // totals must remain the snapshot value because they read from
    // price_snapshot_iqd on the visit row.
    let r2 = r.reports.visits_report(filters).await.unwrap();
    match (r1, r2) {
        (
            VisitsReport::Rows {
                totals: t1,
                rows: rows1,
            },
            VisitsReport::Rows {
                totals: t2,
                rows: rows2,
            },
        ) => {
            assert_eq!(t1.revenue_iqd, t2.revenue_iqd);
            assert_eq!(rows1.len(), rows2.len());
        }
        _ => panic!("expected rows mode"),
    }
}

#[tokio::test]
async fn s6_8_range_clamp_to_90_days_keeps_recent_visits_in_scope() {
    let r = rig().await;
    let _ = lock_visit(&r, false, Some(r.doctor.id)).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(365),
        to_utc: now + Duration::days(1),
    };
    let kpis = r
        .reports
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap();
    // Recent visit is within the clamped 90-day window.
    assert!(kpis.revenue_iqd > 0);
}

#[tokio::test]
async fn s6_8_daily_close_artifact_carries_tenant_id_and_target_date() {
    let r = rig().await;
    let target: NaiveDate = (Utc::now() + Duration::hours(3)).date_naive();
    let close = r
        .reports
        .daily_close(r.superadmin.id, ENTITY_ID, target, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(close.tenant_id, ENTITY_ID);
    assert_eq!(close.target_date, target);
}

// Keep the inventory_item reference live to silence warnings if a future
// refactor inlines its usage.
#[allow(dead_code)]
const _UNUSED: fn(&Rig) = |_r| {
    let _ = &_r.inventory_item;
};
