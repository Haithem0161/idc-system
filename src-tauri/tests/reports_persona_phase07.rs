//! Phase-07 canonical persona script: **P1 Asma the Accountant**.
//!
//! Walks the full accountant day end-to-end through the reports service in
//! 10 sequenced steps. See `docs/idc-system/testing/personas.md` for the
//! narrative.
//!
//! Each step asserts the observable outcome (artifact value, file existence,
//! audit row, role gate). Failure of any step fails the persona run.

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
use app_lib::domains::reports::domain::entities::{
    DateRange, VisitsReport, VisitsReportFilters, VisitsReportGroupBy,
};
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
use chrono::{Duration, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-persona07";
const DEVICE_ID: &str = "dev-persona07";

fn money() -> MoneySettings {
    MoneySettings {
        dye_cost_iqd: 2_000,
        report_pct: 20,
        reporting_doctor_name: String::new(),
        internal_doctor_pct: 40,
    }
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

#[allow(dead_code)]
struct AccountantRig {
    pool: SqlitePool,
    reports: Arc<ReportsService>,
    visits: Arc<VisitService>,
    receptionist: User,
    asma: User,
    patient: Patient,
    check_type: CheckType,
    doctor: Doctor,
    operator: Operator,
    inventory_item: InventoryItem,
    _consumption: InventoryConsumptionMap,
    _operator_specialty: OperatorSpecialty,
}

async fn rig() -> AccountantRig {
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
        "mehdi@idc.iq",
        "Mehdi",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let asma = User::try_new(
        "asma@idc.iq",
        "Asma",
        UserRole::Accountant,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &receptionist).await.unwrap();
    user_repo.upsert(&mut tx, &asma).await.unwrap();
    tx.commit().await.unwrap();

    let check_type = CheckType::try_new(CheckTypeNewInput {
        name_ar: "موجات صوتية".into(),
        name_en: Some("US".into()),
        has_subtypes: false,
        base_price_iqd: Some(60_000),
        dye_supported: true,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let doctor = Doctor::try_new(DoctorNewInput {
        name: "Dr Ali".into(),
        specialty: Some("Cardio".into()),
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
        name: "Kareem".into(),
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
        name_ar: "جل".into(),
        name_en: Some("Gel".into()),
        unit: "ml".into(),
        low_stock_threshold: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let consumption = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
        check_type_id: check_type.id,
        check_subtype_id: None,
        item_id: inventory_item.id,
        quantity_per_check: 5,
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
    shift.check_in_at = Utc::now() - Duration::hours(6);
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
                name: "Sara".into(),
            },
        )
        .await
        .unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-persona07-{}", Uuid::now_v7()));
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

    AccountantRig {
        pool,
        reports,
        visits,
        receptionist,
        asma,
        patient,
        check_type,
        doctor,
        operator,
        inventory_item,
        _consumption: consumption,
        _operator_specialty: operator_specialty,
    }
}

async fn lock_visit(r: &AccountantRig, dye: bool, doctor: Option<Uuid>) -> Uuid {
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
            money(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    res.visit.id
}

/// 10-step day for P1 Asma the Accountant. Each step verifies a piece of
/// what `phase-07-test.md` §5 + §8 promises Asma can do end-to-end.
#[tokio::test]
async fn p1_asma_accountant_day_full_walk() {
    let r = rig().await;

    // ------------------------------------------------------------------
    // STEP 1 -- Reception locks 4 visits across the day (the day Asma will
    // close): 3 with the doctor, 1 house. One of the doctor visits is dye.
    // ------------------------------------------------------------------
    let v1 = lock_visit(&r, false, Some(r.doctor.id)).await;
    let _v2 = lock_visit(&r, true, Some(r.doctor.id)).await;
    let _v3 = lock_visit(&r, false, Some(r.doctor.id)).await;
    let _v4 = lock_visit(&r, false, None).await;

    let now = Utc::now();
    let day_range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };

    // ------------------------------------------------------------------
    // STEP 2 -- Asma opens the dashboard. Role gate accepts accountant.
    // ------------------------------------------------------------------
    ReportsService::require_reports_role(UserRole::Accountant).unwrap();
    let kpis = r
        .reports
        .dashboard_kpis(ENTITY_ID, day_range, false)
        .await
        .unwrap();
    assert!(kpis.revenue_iqd > 0);
    assert!(kpis.doctor_cuts_iqd > 0);
    assert!(kpis.operator_cuts_iqd > 0);

    // ------------------------------------------------------------------
    // STEP 3 -- Top-5 cards render: doctors / operators / check types.
    // ------------------------------------------------------------------
    let tops = r
        .reports
        .dashboard_tops(ENTITY_ID, day_range, false)
        .await
        .unwrap();
    assert!(!tops.top_doctors.is_empty());
    assert_eq!(tops.top_operators.len(), 1);
    assert_eq!(tops.top_check_types.len(), 1);
    assert!(tops.top_doctors.len() <= 5);

    // ------------------------------------------------------------------
    // STEP 4 -- Asma opens the Visits Report grouped by doctor.
    // ------------------------------------------------------------------
    let filters_by_doc = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        group_by: VisitsReportGroupBy::ByDoctor,
        ..Default::default()
    };
    let by_doc = r.reports.visits_report(filters_by_doc).await.unwrap();
    match by_doc {
        VisitsReport::Groups { groups, totals } => {
            // One doctor group + the house pseudo-group.
            assert!(!groups.is_empty());
            assert_eq!(totals.visits, 4);
        }
        _ => panic!("expected groups mode"),
    }

    // ------------------------------------------------------------------
    // STEP 5 -- Asma drills into Dr Ali to see the per-check breakdown.
    // ------------------------------------------------------------------
    let dd = r
        .reports
        .doctor_drilldown(ENTITY_ID, Some(r.doctor.id), day_range, false)
        .await
        .unwrap();
    assert_eq!(dd.doctor_id, Some(r.doctor.id));
    assert!(!dd.per_check.is_empty());
    assert_eq!(dd.source_visits.len(), 3);

    // ------------------------------------------------------------------
    // STEP 6 -- Asma drills into Kareem the operator to see his shifts +
    // attributed visits.
    // ------------------------------------------------------------------
    let od = r
        .reports
        .operator_drilldown(ENTITY_ID, r.operator.id, day_range, false)
        .await
        .unwrap();
    assert_eq!(od.operator_id, r.operator.id);
    assert!(!od.shifts.is_empty());
    assert_eq!(od.attributed_visits.len(), 4);

    // ------------------------------------------------------------------
    // STEP 7 -- Asma exports the visits CSV to disk.
    // ------------------------------------------------------------------
    let dir = std::env::temp_dir().join(format!("idc-asma-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv_path = dir.join("visits.csv");
    let csv_filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    r.reports
        .export_visits_csv(csv_filters, &csv_path)
        .await
        .unwrap();
    let bytes = std::fs::read(&csv_path).unwrap();
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
    assert!(std::str::from_utf8(&bytes[3..]).unwrap().contains("TOTAL"));

    // ------------------------------------------------------------------
    // STEP 8 -- A superadmin voids one of the visits (a dye/lab dispute).
    // The accountant's earlier KPIs are unchanged in cached state -- we
    // re-render and observe the void surfaces.
    // ------------------------------------------------------------------
    let sa = User::try_new(
        "boss@idc.iq",
        "Boss",
        UserRole::Superadmin,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(r.pool.clone()));
    let mut tx = r.pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &sa).await.unwrap();
    tx.commit().await.unwrap();
    r.visits
        .void(sa.id, UserRole::Superadmin, v1, "lab dispute".into())
        .await
        .unwrap();
    let kpis_after_void = r
        .reports
        .dashboard_kpis(ENTITY_ID, day_range, false)
        .await
        .unwrap();
    assert!(kpis_after_void.revenue_iqd < kpis.revenue_iqd);

    // ------------------------------------------------------------------
    // STEP 9 -- Asma runs the Daily Close. The artifact carries the
    // `input_hash`, breakdowns, and -- since outbox is non-empty --
    // surfaces as provisional.
    // ------------------------------------------------------------------
    let target = (Utc::now() + Duration::hours(3)).date_naive();
    let mut settings: BTreeMap<String, String> = BTreeMap::new();
    settings.insert("dye_cost_iqd".into(), "2000".into());
    settings.insert("report_cost_iqd".into(), "3000".into());
    settings.insert("internal_doctor_pct".into(), "40".into());
    let close = r
        .reports
        .daily_close(r.asma.id, ENTITY_ID, target, settings.clone())
        .await
        .unwrap();
    assert!(close.locked_count >= 3);
    assert_eq!(close.voided_count, 1);
    assert!(close.voided_value_iqd > 0);
    assert!(!close.input_hash.is_empty());
    assert_eq!(close.tz_offset, "+03:00");
    assert!(close.provisional);
    assert!(!close.per_doctor.is_empty());
    assert!(!close.per_operator.is_empty());
    assert!(!close.per_check_type.is_empty());

    // ------------------------------------------------------------------
    // STEP 10 -- Asma exports the Daily Close PDF and a daily_close_run
    // audit row is on disk for the auditor.
    // ------------------------------------------------------------------
    let pdf_path = dir.join("daily-close.pdf");
    r.reports
        .render_daily_close_pdf(&close, None, &pdf_path)
        .unwrap();
    assert!(pdf_path.exists());
    // Real PDF: assert the binary magic rather than scanning for text.
    let bytes = std::fs::read(&pdf_path).unwrap();
    assert!(bytes.starts_with(b"%PDF-"), "export is not a real PDF");
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity = 'daily_close' AND action = 'daily_close_run' AND entity_id = ?",
    )
    .bind(target.format("%Y-%m-%d").to_string())
    .fetch_one(&r.pool)
    .await
    .unwrap();
    assert!(row.0 >= 1);
}

#[allow(dead_code)]
const _UNUSED: fn(&AccountantRig) = |_r| {
    let _ = &_r.inventory_item;
};
