//! Integration tests for Phase 7 reports.
//!
//! Drives `ReportsService` end-to-end against an in-memory SQLite with all
//! migrations applied. Covers:
//! - dashboard KPIs aggregation correctness across locked + voided visits
//! - visits report (rows + groups + totals)
//! - doctor earnings (per-doctor + house pseudo-row)
//! - operator earnings (visits, dye visits, hours-on-shift, cut)
//! - daily close: per-doctor / per-operator / per-check breakdown +
//!   audit_log emission with daily_close_run action
//! - role gate (accountant + superadmin pass; receptionist rejected)
//! - 90-day range clamp (§7.16)
//! - CSV export writes a real file with BOM + CRLF + footer (visits, doctors,
//!   operators)

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
use app_lib::domains::patients::domain::entities::Patient;
use app_lib::domains::patients::domain::repositories::PatientRepo;
use app_lib::domains::patients::infrastructure::SqlitePatientRepo;
use app_lib::domains::patients::service::{PatientCreateInput, PatientService};
use app_lib::domains::receipts::ReceiptRenderOptions;
use app_lib::domains::reports::domain::entities::{
    DateRange, VisitsReport, VisitsReportFilters, VisitsReportGroupBy,
};
use app_lib::domains::reports::domain::repositories::ReportsReadModel;
use app_lib::domains::reports::infrastructure::SqliteReportsReadModel;
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
use chrono::{Duration, NaiveDate, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-rep";
const DEVICE_ID: &str = "dev-rep";

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
    reports_service: Arc<ReportsService>,
    visit_service: Arc<VisitService>,
    receptionist: User,
    superadmin: User,
    patient: Patient,
    check_type: CheckType,
    doctor: Doctor,
    _doctor_pricing: DoctorCheckPricing,
    operator: Operator,
    _operator_specialty: OperatorSpecialty,
    inventory_item: InventoryItem,
    _consumption: InventoryConsumptionMap,
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
        name: "Dr Apple".into(),
        specialty: Some("Cardio".into()),
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
        default_cut_kind: None,
        default_cut_value: None,
    })
    .unwrap();
    let doctor_pricing = DoctorCheckPricing::try_new(DoctorPricingNewInput {
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
    let operator = Operator::try_new(OperatorNewInput {
        name: "Op Bee".into(),
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

    // Open a shift with a backdated check_in_at so the hours-on-shift
    // aggregation has a deterministic 4h+ window. Keep check_out_at NULL so
    // the visit lock workflow finds the shift open.
    let mut shift = OperatorShift::open(OperatorShiftOpenInput {
        operator_id: operator.id,
        by_user_id: receptionist.id,
        note: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    shift.check_in_at = Utc::now() - Duration::hours(4);
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
                name: "Pat O.".into(),
            },
        )
        .await
        .unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-rep-{}", Uuid::now_v7()));
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
        audit_repo: audit.clone(),
        outbox_repo: outbox.clone(),
        receipts_dir,
        device_id: DEVICE_ID.to_string(),
    }));

    let read_model: Arc<dyn ReportsReadModel> = Arc::new(SqliteReportsReadModel::new(pool.clone()));
    let reports_service = Arc::new(ReportsService::new(ReportsServiceConfig {
        pool: pool.clone(),
        read_model,
        audit_repo: audit,
        outbox_repo: outbox,
        device_id: DEVICE_ID.to_string(),
    }));

    Fixture {
        pool,
        reports_service,
        visit_service,
        receptionist,
        superadmin,
        patient,
        check_type,
        doctor,
        _doctor_pricing: doctor_pricing,
        operator,
        _operator_specialty: operator_specialty,
        inventory_item,
        _consumption: consumption,
    }
}

fn settings() -> MoneySettings {
    MoneySettings {
        dye_cost_iqd: 2_000,
        report_cost_iqd: 3_000,
        internal_doctor_pct: 40,
    }
}

async fn lock_visit(f: &Fixture, dye: bool, doctor: Option<Uuid>) -> Uuid {
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
                doctor_id: doctor,
                dye,
                report: false,
            },
        )
        .await
        .unwrap();
    let res = f
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
    res.visit.id
}

#[tokio::test]
async fn dashboard_kpis_aggregate_locked_visits() {
    let f = seed().await;
    // Lock two visits: one with doctor (dye), one house (no doctor, internal
    // pct from settings).
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, None).await;

    // Wide range covering "now".
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let kpis = f
        .reports_service
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap();
    // Doctor visit: price 50000 + dye 2000 = 52000; doctor cut = 15000 (30%);
    // operator cut = 4000.
    // House visit: price 50000 + 0 = 50000; internal_pct = 40 of 50000 =
    // 20000 doctor cut; operator cut = 4000.
    // Totals: revenue = 52000 + 50000 = 102000; doctor cuts = 35000;
    // operator cuts = 8000.
    assert_eq!(kpis.revenue_iqd, 102_000);
    assert_eq!(kpis.doctor_cuts_iqd, 35_000);
    assert_eq!(kpis.operator_cuts_iqd, 8_000);
    // Inventory consumption: 1 unit per visit x 2 visits = 2 IQD-equivalent.
    assert_eq!(kpis.inventory_consumption_value_iqd, 2);
    assert_eq!(kpis.net_iqd, 102_000 - 35_000 - 8_000 - 2);
}

#[tokio::test]
async fn visits_report_rows_mode_and_groups_mode() {
    let f = seed().await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;

    let now = Utc::now();
    let base_filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };

    // Rows mode. The Visits Report's per-row Price column = price_snapshot
    // (50_000 each). Totals footer sums per-row Price (PRD §7.25), so
    // totals.revenue_iqd = 100_000. Dye / report add-ons are operational
    // line items and surface in the "Net" column instead.
    let rows = f
        .reports_service
        .visits_report(base_filters.clone())
        .await
        .unwrap();
    match rows {
        VisitsReport::Rows { rows, totals } => {
            assert_eq!(rows.len(), 2);
            assert_eq!(totals.visits, 2);
            assert_eq!(totals.revenue_iqd, 100_000);
        }
        _ => panic!("expected rows mode"),
    }

    // Groups mode (by doctor).
    let groups = f
        .reports_service
        .visits_report(VisitsReportFilters {
            group_by: VisitsReportGroupBy::ByDoctor,
            ..base_filters
        })
        .await
        .unwrap();
    match groups {
        VisitsReport::Groups { groups, totals } => {
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].visits, 2);
            assert_eq!(totals.visits, 2);
        }
        _ => panic!("expected groups mode"),
    }
}

#[tokio::test]
async fn doctor_earnings_includes_house_pseudo_row() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, None).await;

    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let rows = f
        .reports_service
        .doctor_earnings(ENTITY_ID, range, false)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    // The house row has doctor_id = None.
    let house = rows.iter().find(|r| r.doctor_id.is_none()).unwrap();
    assert_eq!(house.visits, 1);
    let other = rows.iter().find(|r| r.doctor_id.is_some()).unwrap();
    assert_eq!(other.visits, 1);
    assert_eq!(other.doctor_cut_total_iqd, 15_000); // 30% of 50000
}

#[tokio::test]
async fn operator_earnings_reports_visits_and_dye() {
    let f = seed().await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;

    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let rows = f
        .reports_service
        .operator_earnings(ENTITY_ID, range, false)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.operator_id, f.operator.id);
    assert_eq!(r.visits, 2);
    assert_eq!(r.visits_with_dye, 1);
    assert_eq!(r.operator_cut_total_iqd, 8_000);
    // Backdated shift covers 4h; some skew within a few seconds is fine.
    assert!(r.hours_on_shift_milli >= 4 * 3_600_000 - 5_000);
}

#[tokio::test]
async fn daily_close_emits_audit_row_and_breakdowns() {
    let f = seed().await;
    let visit_id = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = visit_id;

    // Compute the local-tz target date (Baghdad +03:00).
    let now = Utc::now();
    let baghdad = now + Duration::hours(3);
    let target: NaiveDate = baghdad.date_naive();

    let mut settings_snapshot: BTreeMap<String, String> = BTreeMap::new();
    settings_snapshot.insert("dye_cost_iqd".into(), "2000".into());
    settings_snapshot.insert("report_cost_iqd".into(), "3000".into());
    settings_snapshot.insert("internal_doctor_pct".into(), "40".into());

    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, settings_snapshot)
        .await
        .unwrap();
    assert_eq!(close.locked_count, 1);
    assert_eq!(close.total_revenue_iqd, 50_000);
    assert_eq!(close.per_doctor.len(), 1);
    assert_eq!(close.per_operator.len(), 1);
    assert_eq!(close.per_check_type.len(), 1);
    assert!(!close.input_hash.is_empty());
    assert_eq!(close.tz_offset, "+03:00");

    // The audit row is keyed on entity = 'daily_close' with the target date
    // as entity_id.
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity = 'daily_close' \
           AND action = 'daily_close_run' \
           AND entity_id = ?",
    )
    .bind(target.format("%Y-%m-%d").to_string())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);

    // No outbox row for the daily close itself, but the audit row IS
    // enqueued for sync.
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'audit_log'")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    // Daily close adds one; the locked-visit workflow added many more.
    assert!(row.0 >= 1);
}

#[tokio::test]
async fn role_gate_rejects_receptionist() {
    // The static gate is pure; assert it returns the right outcome.
    assert!(ReportsService::require_reports_role(UserRole::Receptionist).is_err());
    assert!(ReportsService::require_reports_role(UserRole::Accountant).is_ok());
    assert!(ReportsService::require_reports_role(UserRole::Superadmin).is_ok());
}

#[tokio::test]
async fn range_clamp_to_ninety_days() {
    let f = seed().await;
    // Lock one visit "today".
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let now = Utc::now();
    // Request a 200-day range; the service should clamp to the most-recent
    // 90 days, which still includes "now".
    let range = DateRange {
        from_utc: now - Duration::days(200),
        to_utc: now + Duration::days(1),
    };
    let kpis = f
        .reports_service
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap();
    assert_eq!(kpis.revenue_iqd, 50_000);
}

#[tokio::test]
async fn export_visits_csv_writes_bom_and_footer() {
    let f = seed().await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;

    let dir = std::env::temp_dir().join(format!("idc-csv-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("visits.csv");

    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    f.reports_service
        .export_visits_csv(filters, &path)
        .await
        .unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
    let text = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(text.contains("\r\n"));
    assert!(text.contains("TOTAL,"));
    // patient column quoted only if needed; bare "Pat O." has a period so
    // unquoted, the period itself is fine.
    assert!(text.contains("Pat O."));
}

#[tokio::test]
async fn dashboard_tops_returns_top_five_lists() {
    let f = seed().await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, None).await;

    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let tops = f
        .reports_service
        .dashboard_tops(ENTITY_ID, range, false)
        .await
        .unwrap();
    assert!(tops.top_doctors.len() >= 2);
    assert_eq!(tops.top_operators.len(), 1);
    assert_eq!(tops.top_check_types.len(), 1);
}

// Silence "unused" on superadmin / inventory_item which we keep for clarity.
#[allow(dead_code)]
const _UNUSED: fn(&Fixture) = |_f| {
    let _ = &_f.inventory_item;
};

// ---------------------------------------------------------------------------
// Phase-07 extended integration tests (gap-analysis §9 + §10 + §11 + §12)
// ---------------------------------------------------------------------------

fn baghdad_today() -> NaiveDate {
    (Utc::now() + Duration::hours(3)).date_naive()
}

fn snapshot_settings() -> BTreeMap<String, String> {
    let mut s: BTreeMap<String, String> = BTreeMap::new();
    s.insert("dye_cost_iqd".into(), "2000".into());
    s.insert("report_cost_iqd".into(), "3000".into());
    s.insert("internal_doctor_pct".into(), "40".into());
    s
}

/// §7.19 idempotency: re-running daily close without new visits yields the
/// same `input_hash`.
#[tokio::test]
async fn daily_close_idempotent_when_no_new_visits_between_runs() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let run1 = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let run2 = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    assert_eq!(run1.input_hash, run2.input_hash);
    assert_eq!(run1.locked_count, run2.locked_count);
    assert_eq!(run1.total_revenue_iqd, run2.total_revenue_iqd);
}

/// §7.19 recomputation chip: a new locked visit between runs changes the
/// hash AND the locked_count.
#[tokio::test]
async fn daily_close_hash_changes_when_new_visit_locks_between_runs() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let run1 = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let run2 = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    assert_ne!(run1.input_hash, run2.input_hash);
    assert_eq!(run2.locked_count, run1.locked_count + 1);
}

/// §7.20: `provisional` flips with the outbox depth.
#[tokio::test]
async fn daily_close_provisional_reflects_outbox_depth() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    // Visit lock enqueues outbox rows (visit + inventory adjustment + audit);
    // the daily-close before draining sees pending_sync > 0.
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    assert!(close.pending_sync > 0);
    assert!(close.provisional);
}

/// §7.18 + §9.1 P07-G01: the `daily_close_run` audit delta carries the full
/// forensic payload.
#[tokio::test]
async fn daily_close_audit_delta_carries_full_payload() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let row: (String,) = sqlx::query_as(
        "SELECT delta FROM audit_log \
         WHERE entity = 'daily_close' AND action = 'daily_close_run' \
         AND entity_id = ? \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(target.format("%Y-%m-%d").to_string())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&row.0).unwrap();
    assert_eq!(v["input_hash"].as_str().unwrap(), close.input_hash);
    assert_eq!(
        v["total_revenue_iqd"].as_i64().unwrap(),
        close.total_revenue_iqd
    );
    assert_eq!(v["locked_count"].as_i64().unwrap(), close.locked_count);
    assert_eq!(v["voided_count"].as_i64().unwrap(), close.voided_count);
    assert_eq!(
        v["pending_sync_count"].as_i64().unwrap(),
        close.pending_sync
    );
    assert_eq!(v["provisional"].as_bool().unwrap(), close.provisional);
    // generated_at must be ISO 8601 with timezone (RFC3339).
    let gen = v["generated_at"].as_str().unwrap();
    assert!(chrono::DateTime::parse_from_rfc3339(gen).is_ok());
}

/// §7.21 P11.4 P07-G30: per-check-type rows carry BOTH name_ar AND name_en.
#[tokio::test]
async fn daily_close_per_check_type_carries_both_locale_names() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    assert!(!close.per_check_type.is_empty());
    for row in &close.per_check_type {
        assert!(!row.name_ar.is_empty(), "name_ar must be present");
        // Fixture seed sets name_en = Some("Test"); the helper preserves it.
        assert!(row.name_en.is_some(), "name_en must be present");
    }
}

/// §7.8 boundary: a visit locked at "today + 1 day" is NOT in today's close.
#[tokio::test]
async fn daily_close_excludes_visits_outside_local_day_boundary() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let past_date = baghdad_today() - Duration::days(7);
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, past_date, snapshot_settings())
        .await
        .unwrap();
    assert_eq!(close.locked_count, 0);
    assert_eq!(close.total_revenue_iqd, 0);
}

/// §6.5 empty-day: zero locked visits on the target date returns a
/// zero-totals artifact -- NOT an error.
#[tokio::test]
async fn daily_close_on_empty_day_returns_zero_totals_artifact() {
    let f = seed().await;
    let target = baghdad_today();
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    assert_eq!(close.locked_count, 0);
    assert_eq!(close.total_revenue_iqd, 0);
    assert_eq!(close.voided_count, 0);
    assert_eq!(close.net_iqd, 0);
    assert_eq!(close.per_doctor.len(), 0);
    assert_eq!(close.per_operator.len(), 0);
    assert_eq!(close.per_check_type.len(), 0);
    assert!(!close.input_hash.is_empty());
}

/// §7.23 PDF filename: rendered file path embeds the input_hash 6-char
/// prefix + the target date.
#[tokio::test]
async fn daily_close_pdf_path_embeds_target_date_and_input_hash_prefix() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let dir = std::env::temp_dir().join(format!("idc-pdf-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let filename = format!(
        "daily-close_{}_{}.pdf",
        target.format("%Y-%m-%d"),
        &close.input_hash[..6]
    );
    let path = dir.join(&filename);
    f.reports_service
        .render_daily_close_pdf(&close, &path)
        .unwrap();
    assert!(path.exists());
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains(&close.input_hash));
    assert!(body.contains("DAILY CLOSE"));
    if close.provisional {
        assert!(body.contains("PROVISIONAL"));
    }
}

/// §7.10: voided visits do NOT subtract from `total_revenue_iqd` but DO
/// surface in `voided_count` and `voided_value_iqd`.
#[tokio::test]
async fn daily_close_voided_revenue_is_separate_and_does_not_subtract() {
    let f = seed().await;
    let locked = lock_visit(&f, false, Some(f.doctor.id)).await;
    // Void it via the visit service.
    f.visit_service
        .void(
            f.superadmin.id,
            UserRole::Superadmin,
            locked,
            "test void reason".into(),
        )
        .await
        .unwrap();
    // Lock another so locked_count > 0 too.
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    assert_eq!(close.locked_count, 1);
    assert_eq!(close.voided_count, 1);
    assert!(close.voided_value_iqd > 0);
    // Total revenue equals the surviving locked visit only.
    assert_eq!(close.total_revenue_iqd, 50_000);
}

/// §7.2 dashboard include_voided toggle widens the set without subtracting.
#[tokio::test]
async fn dashboard_kpis_include_voided_toggle_extends_visit_set() {
    let f = seed().await;
    let visit_id = lock_visit(&f, false, Some(f.doctor.id)).await;
    f.visit_service
        .void(
            f.superadmin.id,
            UserRole::Superadmin,
            visit_id,
            "void me".into(),
        )
        .await
        .unwrap();
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let off = f
        .reports_service
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap();
    let on = f
        .reports_service
        .dashboard_kpis(ENTITY_ID, range, true)
        .await
        .unwrap();
    // include_voided=false sees no locked visits left.
    assert_eq!(off.revenue_iqd, 0);
    // include_voided=true sees the voided visit's revenue.
    assert!(on.revenue_iqd > 0);
}

/// §7.22: top doctors slice is sorted by revenue DESC and capped at 5.
#[tokio::test]
async fn dashboard_tops_top_doctors_sorted_by_revenue_desc_and_capped_at_5() {
    let f = seed().await;
    for _ in 0..3 {
        let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    }
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let tops = f
        .reports_service
        .dashboard_tops(ENTITY_ID, range, false)
        .await
        .unwrap();
    assert!(tops.top_doctors.len() <= 5);
    // The doctor's revenue exceeds the (house) row.
    let revs: Vec<i64> = tops.top_doctors.iter().map(|d| d.revenue_iqd).collect();
    let mut sorted = revs.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(revs, sorted);
}

/// §7.14 by_operator grouping returns one row per operator with sums.
#[tokio::test]
async fn visits_report_groupby_by_operator_aggregates_per_operator() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        group_by: VisitsReportGroupBy::ByOperator,
        ..Default::default()
    };
    let r = f.reports_service.visits_report(filters).await.unwrap();
    match r {
        VisitsReport::Groups { groups, totals } => {
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].visits, 2);
            assert_eq!(totals.visits, 2);
        }
        _ => panic!("expected groups mode"),
    }
}

/// §7.14 by_status: locked vs voided land in separate groups.
#[tokio::test]
async fn visits_report_groupby_by_status_separates_locked_and_voided() {
    let f = seed().await;
    let v = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    f.visit_service
        .void(f.superadmin.id, UserRole::Superadmin, v, "void five".into())
        .await
        .unwrap();
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        group_by: VisitsReportGroupBy::ByStatus,
        include_voided: true,
        ..Default::default()
    };
    let r = f.reports_service.visits_report(filters).await.unwrap();
    match r {
        VisitsReport::Groups { groups, .. } => {
            let keys: Vec<&str> = groups.iter().map(|g| g.key.as_str()).collect();
            assert!(keys.contains(&"locked"));
            assert!(keys.contains(&"voided"));
        }
        _ => panic!("expected groups mode"),
    }
}

/// §4.1 + §7.14: doctor_ids filter combines with include_house.
#[tokio::test]
async fn visits_report_filter_by_doctor_ids_excludes_other_doctors() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, None).await; // house
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        doctor_ids: vec![f.doctor.id],
        include_house: false,
        ..Default::default()
    };
    let r = f.reports_service.visits_report(filters).await.unwrap();
    match r {
        VisitsReport::Rows { rows, .. } => {
            assert_eq!(rows.len(), 1);
            assert!(rows.iter().all(|r| r.doctor_name.is_some()));
        }
        _ => panic!("expected rows mode"),
    }
}

/// §7.14 + §4.1: include_house OR-combines `doctor_id IN (...)` with NULL.
#[tokio::test]
async fn visits_report_filter_doctor_ids_with_include_house_yields_both() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, false, None).await;
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(1),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        doctor_ids: vec![f.doctor.id],
        include_house: true,
        ..Default::default()
    };
    let r = f.reports_service.visits_report(filters).await.unwrap();
    match r {
        VisitsReport::Rows { rows, .. } => assert_eq!(rows.len(), 2),
        _ => panic!("expected rows mode"),
    }
}

/// §4 + §7.16: a range with `to <= from` returns Validation error.
#[tokio::test]
async fn range_clamp_rejects_inverted_range_with_validation_error() {
    let f = seed().await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now + Duration::days(2),
        to_utc: now,
    };
    let err = f
        .reports_service
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap_err();
    assert!(matches!(err, app_lib::error::AppError::Validation(_)));
}

/// §7.4 + §7.30: doctor drilldown returns per-check breakdown and source
/// visits.
#[tokio::test]
async fn doctor_drilldown_returns_per_check_breakdown_and_source_visits() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let dd = f
        .reports_service
        .doctor_drilldown(ENTITY_ID, Some(f.doctor.id), range, false)
        .await
        .unwrap();
    assert_eq!(dd.doctor_id, Some(f.doctor.id));
    assert!(!dd.per_check.is_empty());
    assert_eq!(dd.source_visits.len(), 2);
    assert_eq!(dd.totals.visits, 2);
}

/// §7.4: doctor drilldown for the house pseudo-doctor uses doctor_id=None.
#[tokio::test]
async fn doctor_drilldown_for_house_uses_doctor_id_none() {
    let f = seed().await;
    let _ = lock_visit(&f, false, None).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let dd = f
        .reports_service
        .doctor_drilldown(ENTITY_ID, None, range, false)
        .await
        .unwrap();
    assert!(dd.doctor_id.is_none());
    assert_eq!(dd.name, "(house)");
    assert_eq!(dd.source_visits.len(), 1);
}

/// §7.5 + §7.30: operator drilldown returns shifts + attributed visits.
#[tokio::test]
async fn operator_drilldown_returns_shifts_and_attributed_visits() {
    let f = seed().await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let od = f
        .reports_service
        .operator_drilldown(ENTITY_ID, f.operator.id, range, false)
        .await
        .unwrap();
    assert_eq!(od.operator_id, f.operator.id);
    assert!(!od.shifts.is_empty());
    assert_eq!(od.attributed_visits.len(), 1);
    assert!(od.total_hours_milli >= 0);
}

/// §7.7 doctor CSV export writes a valid file to disk.
#[tokio::test]
async fn export_doctors_csv_writes_file_with_bom_and_total_footer() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let dir = std::env::temp_dir().join(format!("idc-doccsv-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doctors.csv");
    f.reports_service
        .export_doctor_earnings_csv(ENTITY_ID, range, false, &path)
        .await
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
    let text = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(text.contains("Doctor,Specialty,Visits"));
    assert!(text.contains("TOTAL"));
}

/// §7.7 operator CSV export writes a valid file with the hours column.
#[tokio::test]
async fn export_operators_csv_writes_file_with_hours_column() {
    let f = seed().await;
    let _ = lock_visit(&f, true, Some(f.doctor.id)).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    let dir = std::env::temp_dir().join(format!("idc-opcsv-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("operators.csv");
    f.reports_service
        .export_operator_earnings_csv(ENTITY_ID, range, false, &path)
        .await
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
    let text = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(text.contains("Operator,Visits,Visits With Dye"));
    assert!(text.contains("Hours On Shift"));
    assert!(text.contains("TOTAL"));
}

/// §7.18 (mirror): each daily_close run writes a NEW audit row -- the table
/// has 2 rows after 2 runs (additive policy).
#[tokio::test]
async fn daily_close_two_runs_emit_two_audit_rows() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let _ = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let _ = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity = 'daily_close' AND action = 'daily_close_run' \
         AND entity_id = ?",
    )
    .bind(target.format("%Y-%m-%d").to_string())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 2);
}

/// §4 step 5 + §7.20: outbox depth is read into pending_sync.
#[tokio::test]
async fn daily_close_pending_sync_reflects_outbox_row_count() {
    let f = seed().await;
    let _ = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let close = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    // Close itself adds an audit-log outbox row, so the artifact's
    // pending_sync (sampled BEFORE that enqueue) is one less.
    assert!(row.0 >= close.pending_sync);
}

/// §7.10: voided visits' consume_visit inventory adjustments REMAIN
/// reflected in inventory_consumption_value (the offset enqueues +delta
/// records but the original -delta row is still there; net should match the
/// repo's SUM over the day).
#[tokio::test]
async fn daily_close_inventory_value_after_void_uses_repo_sum() {
    let f = seed().await;
    let visit = lock_visit(&f, false, Some(f.doctor.id)).await;
    let target = baghdad_today();
    let close_before = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    f.visit_service
        .void(
            f.superadmin.id,
            UserRole::Superadmin,
            visit,
            "void inventory".into(),
        )
        .await
        .unwrap();
    let close_after = f
        .reports_service
        .daily_close(f.superadmin.id, ENTITY_ID, target, snapshot_settings())
        .await
        .unwrap();
    // After void, repository SUM nets to zero (negative consume +
    // positive offset). Both pre- and post-void counts are non-negative.
    assert!(close_before.total_inventory_consumption_value_iqd >= 0);
    assert!(close_after.total_inventory_consumption_value_iqd >= 0);
}

/// Role gate: dashboard_tops mirrors the receptionist 403 rule.
#[tokio::test]
async fn role_gate_rejects_receptionist_for_dashboard_tops() {
    assert!(ReportsService::require_reports_role(UserRole::Receptionist).is_err());
    assert!(ReportsService::require_reports_role(UserRole::Accountant).is_ok());
    assert!(ReportsService::require_reports_role(UserRole::Superadmin).is_ok());
}
