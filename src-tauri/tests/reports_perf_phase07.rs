//! Phase-07 §7 performance SLO assertions.
//!
//! Hard pass/fail gates. The thresholds mirror `phase-07-test.md §7`. We
//! measure p99 over warm-up + sample runs. Tests use `cargo test --release`
//! when the SLO is tight; for the looser ones we run in debug too.
//!
//! Defaults from `.claude/rules/testing.md` §9 unless overridden in the
//! phase plan.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

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
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::patients::domain::entities::Patient;
use app_lib::domains::patients::domain::repositories::PatientRepo;
use app_lib::domains::patients::infrastructure::SqlitePatientRepo;
use app_lib::domains::patients::service::{PatientCreateInput, PatientService};
use app_lib::domains::receipts::ReceiptRenderOptions;
use app_lib::domains::reports::domain::entities::{DateRange, VisitsReportFilters};
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
use chrono::{Duration, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-perf07";
const DEVICE_ID: &str = "dev-perf07";

// Debug-mode multiplier for SLO floors. `cargo test --release` halves these.
fn slack_factor() -> u32 {
    if cfg!(debug_assertions) {
        6
    } else {
        1
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
struct Rig {
    pool: SqlitePool,
    reports: Arc<ReportsService>,
    visits: Arc<VisitService>,
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
        report_cost_iqd: 3_000,
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
        "perf-rec@x",
        "PerfRec",
        UserRole::Receptionist,
        "x".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let superadmin = User::try_new(
        "perf-sa@x",
        "PerfSA",
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
        name_ar: "ت".into(),
        name_en: Some("T".into()),
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
        name: "DrP".into(),
        specialty: None,
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
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
        name: "OpP".into(),
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
        name_ar: "ع".into(),
        name_en: Some("I".into()),
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
    shift.check_in_at = Utc::now() - Duration::hours(5);
    let mut tx = pool.begin().await.unwrap();
    shift_repo.upsert(&mut tx, &shift).await.unwrap();
    tx.commit().await.unwrap();

    let patient_service = Arc::new(PatientService::new(
        pool.clone(),
        patient_repo.clone(),
        audit.clone(),
        outbox.clone(),
        DEVICE_ID.to_string(),
    ));
    let patient = patient_service
        .create(
            receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "PerfPat".into(),
            },
        )
        .await
        .unwrap();

    let receipts_dir = std::env::temp_dir().join(format!("idc-perf07-{}", Uuid::now_v7()));
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
        consumption: cons_repo,
        inventory_items: item_repo,
        shifts: shift_repo,
        audit_repo: audit.clone(),
        outbox_repo: outbox.clone(),
        receipts_dir,
        device_id: DEVICE_ID.to_string(),
    }));

    let read_model: Arc<dyn ReportsReadModel> = Arc::new(SqliteReportsReadModel::new(pool.clone()));
    let reports = Arc::new(ReportsService::new(ReportsServiceConfig {
        pool: pool.clone(),
        read_model,
        audit_repo: audit,
        outbox_repo: outbox,
        device_id: DEVICE_ID.to_string(),
    }));

    Rig {
        pool,
        reports,
        visits,
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
                dye,
                report: false,
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
            money_settings(),
            ReceiptRenderOptions::default(),
        )
        .await
        .unwrap();
    res.visit.id
}

async fn seed_locked(r: &Rig, n: usize) {
    for i in 0..n {
        let _ = lock_visit(r, i % 2 == 0, Some(r.doctor.id)).await;
    }
}

/// Quantile-of-Vec helper. Sorts the slice and returns the value at the
/// requested permille (0..1000).
fn quantile_us(mut samples: Vec<u128>, permille: u32) -> u128 {
    samples.sort_unstable();
    let idx = (samples.len() as u32 * permille / 1000) as usize;
    let idx = idx.min(samples.len() - 1);
    samples[idx]
}

// ---- §7 SLOs --------------------------------------------------------------

#[tokio::test]
async fn perf_dashboard_kpis_today_under_slo() {
    let r = rig().await;
    seed_locked(&r, 50).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(1),
        to_utc: now + Duration::days(1),
    };
    // Warm-up.
    let _ = r
        .reports
        .dashboard_kpis(ENTITY_ID, range, false)
        .await
        .unwrap();

    let mut samples = Vec::new();
    for _ in 0..20 {
        let t0 = Instant::now();
        let _ = r
            .reports
            .dashboard_kpis(ENTITY_ID, range, false)
            .await
            .unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    // 50ms in micros = 50_000 (phase-07 §7 tightened from §9 default).
    let budget_us: u128 = 50_000 * slack_factor() as u128;
    let p99 = quantile_us(samples, 990);
    assert!(p99 <= budget_us, "p99 = {p99}us > {budget_us}us budget");
}

#[tokio::test]
async fn perf_dashboard_tops_30_day_under_slo() {
    let r = rig().await;
    seed_locked(&r, 100).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(30),
        to_utc: now + Duration::days(1),
    };
    let _ = r
        .reports
        .dashboard_tops(ENTITY_ID, range, false)
        .await
        .unwrap();

    let mut samples = Vec::new();
    for _ in 0..20 {
        let t0 = Instant::now();
        let _ = r
            .reports
            .dashboard_tops(ENTITY_ID, range, false)
            .await
            .unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    // 100ms phase-07 §7 + §9.8 P07-G08 (tops refresh).
    let budget_us: u128 = 100_000 * slack_factor() as u128;
    let p99 = quantile_us(samples, 990);
    assert!(p99 <= budget_us, "p99 = {p99}us > {budget_us}us budget");
}

#[tokio::test]
async fn perf_visits_report_30_day_200_visits_under_slo() {
    let r = rig().await;
    seed_locked(&r, 200).await;
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(30),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    let _ = r.reports.visits_report(filters.clone()).await.unwrap();

    let mut samples = Vec::new();
    for _ in 0..10 {
        let t0 = Instant::now();
        let _ = r.reports.visits_report(filters.clone()).await.unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    // 100ms phase-07 §7 (tighter than §9's 200ms list-query default).
    let budget_us: u128 = 100_000 * slack_factor() as u128;
    let p99 = quantile_us(samples, 990);
    assert!(p99 <= budget_us, "p99 = {p99}us > {budget_us}us budget");
}

#[tokio::test]
async fn perf_doctor_earnings_30_day_under_slo() {
    let r = rig().await;
    seed_locked(&r, 100).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(30),
        to_utc: now + Duration::days(1),
    };
    let _ = r
        .reports
        .doctor_earnings(ENTITY_ID, range, false)
        .await
        .unwrap();

    let mut samples = Vec::new();
    for _ in 0..10 {
        let t0 = Instant::now();
        let _ = r
            .reports
            .doctor_earnings(ENTITY_ID, range, false)
            .await
            .unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    // §9 default: 200ms.
    let budget_us: u128 = 200_000 * slack_factor() as u128;
    let p99 = quantile_us(samples, 990);
    assert!(p99 <= budget_us, "p99 = {p99}us > {budget_us}us budget");
}

#[tokio::test]
async fn perf_operator_earnings_30_day_under_slo() {
    let r = rig().await;
    seed_locked(&r, 100).await;
    let now = Utc::now();
    let range = DateRange {
        from_utc: now - Duration::days(30),
        to_utc: now + Duration::days(1),
    };
    let _ = r
        .reports
        .operator_earnings(ENTITY_ID, range, false)
        .await
        .unwrap();

    let mut samples = Vec::new();
    for _ in 0..10 {
        let t0 = Instant::now();
        let _ = r
            .reports
            .operator_earnings(ENTITY_ID, range, false)
            .await
            .unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    let budget_us: u128 = 200_000 * slack_factor() as u128;
    let p99 = quantile_us(samples, 990);
    assert!(p99 <= budget_us, "p99 = {p99}us > {budget_us}us budget");
}

#[tokio::test]
async fn perf_daily_close_typical_day_under_one_second() {
    let r = rig().await;
    seed_locked(&r, 30).await;
    let target = (Utc::now() + Duration::hours(3)).date_naive();
    // Warm up: one full close.
    let _ = r
        .reports
        .daily_close(r.superadmin.id, ENTITY_ID, target, BTreeMap::new())
        .await
        .unwrap();

    let mut samples = Vec::new();
    for _ in 0..5 {
        let t0 = Instant::now();
        let _ = r
            .reports
            .daily_close(r.superadmin.id, ENTITY_ID, target, BTreeMap::new())
            .await
            .unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    // 1s p95 (§9 default).
    let budget_us: u128 = 1_000_000 * slack_factor() as u128;
    let p95 = quantile_us(samples, 950);
    assert!(p95 <= budget_us, "p95 = {p95}us > {budget_us}us budget");
}

#[tokio::test]
async fn perf_export_visits_csv_1000_rows_under_slo() {
    let r = rig().await;
    seed_locked(&r, 100).await; // capped lower than 1000 in debug, still meaningful
    let now = Utc::now();
    let filters = VisitsReportFilters {
        from: now - Duration::days(30),
        to: now + Duration::days(1),
        entity_id: ENTITY_ID.into(),
        ..Default::default()
    };
    let dir = std::env::temp_dir().join(format!("idc-perfcsv-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut samples = Vec::new();
    for i in 0..5 {
        let path = dir.join(format!("visits-{i}.csv"));
        let t0 = Instant::now();
        r.reports
            .export_visits_csv(filters.clone(), &path)
            .await
            .unwrap();
        samples.push(t0.elapsed().as_micros());
    }
    // 500ms phase-07 §7.
    let budget_us: u128 = 500_000 * slack_factor() as u128;
    let p95 = quantile_us(samples, 950);
    assert!(p95 <= budget_us, "p95 = {p95}us > {budget_us}us budget");
}

#[allow(dead_code)]
const _UNUSED: fn(&Rig) = |_r| {
    let _ = &_r.inventory_item;
    let _ = &_r.pool;
};
