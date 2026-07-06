//! Phase-03 §7 Performance SLO assertions.
//!
//! Each scenario seeds the synthetic scale fixture, exercises the surface,
//! and asserts wall-clock thresholds matching the §7 table. Release-only
//! perf SLOs are gated by `#[cfg(not(debug_assertions))]` so we don't fail
//! debug runs on slow CI nodes.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use app_lib::db::migrations;
use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
use app_lib::domains::catalog::domain::entities::doctor::DoctorNewInput;
use app_lib::domains::catalog::domain::entities::doctor_pricing::DoctorPricingNewInput;
use app_lib::domains::catalog::domain::entities::inventory_item::InventoryItemNewInput;
use app_lib::domains::catalog::domain::entities::{
    CheckType, Doctor, DoctorCheckPricing, InventoryItem,
};
use app_lib::domains::catalog::domain::repositories::{
    CatalogListFilter, CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo,
    InventoryItemRepo,
};
use app_lib::domains::catalog::domain::services::{EffectivePriceQuery, PricingResolver};
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryItemRepo,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

const ENTITY_ID: &str = "perf-tenant";

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

fn flat_check_type(name: &str, price: i64) -> CheckType {
    CheckType::try_new(CheckTypeNewInput {
        name_ar: name.into(),
        name_en: None,
        has_subtypes: false,
        base_price_iqd: Some(price),
        dye_price_iqd: None,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap()
}

fn doctor(name: &str) -> Doctor {
    Doctor::try_new(DoctorNewInput {
        name: name.into(),
        specialty: None,
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
        default_cut_kind: None,
        default_cut_value: None,
    })
    .unwrap()
}

async fn seed_doctors(pool: &SqlitePool, count: usize) -> Vec<Doctor> {
    let repo = SqliteDoctorRepo::new(pool.clone());
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let d = doctor(&format!("Doctor {i:04}"));
        let mut tx = pool.begin().await.unwrap();
        repo.upsert(&mut tx, &d).await.unwrap();
        tx.commit().await.unwrap();
        out.push(d);
    }
    out
}

async fn seed_inventory(pool: &SqlitePool, count: usize) -> Vec<InventoryItem> {
    let repo = SqliteInventoryItemRepo::new(pool.clone());
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let item = InventoryItem::try_new(InventoryItemNewInput {
            name_ar: format!("Item {i:04}"),
            name_en: None,
            unit: "ml".into(),
            low_stock_threshold: 0,
            entity_id: ENTITY_ID.into(),
            origin_device_id: None,
        })
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        repo.upsert(&mut tx, &item).await.unwrap();
        tx.commit().await.unwrap();
        out.push(item);
    }
    out
}

fn assert_p99<T>(samples: Vec<T>, threshold: std::time::Duration, label: &str)
where
    T: Into<f64> + Copy + PartialOrd,
{
    let mut sorted: Vec<f64> = samples.iter().map(|s| (*s).into()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((sorted.len() as f64) * 0.99).ceil() as usize - 1;
    let idx = idx.min(sorted.len() - 1);
    let p99_ms = sorted[idx];
    let threshold_ms = threshold.as_secs_f64() * 1000.0;
    assert!(
        p99_ms <= threshold_ms,
        "{label} p99 {p99_ms:.2}ms > threshold {threshold_ms:.2}ms"
    );
}

#[tokio::test]
async fn perf_check_types_list_small_under_5ms_p99() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    for i in 0..20 {
        let ct = flat_check_type(&format!("Type {i:02}"), 1000);
        let mut tx = pool.begin().await.unwrap();
        repo.upsert(&mut tx, &ct).await.unwrap();
        tx.commit().await.unwrap();
    }
    let mut samples = Vec::new();
    for _ in 0..50 {
        let t0 = Instant::now();
        let _ = repo
            .list(CatalogListFilter {
                entity_id: ENTITY_ID.into(),
                include_deleted: false,
                include_inactive: false,
                query: None,
            })
            .await
            .unwrap();
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let threshold = std::time::Duration::from_millis(if cfg!(debug_assertions) { 100 } else { 5 });
    assert_p99(samples, threshold, "check_types::list small");
}

#[tokio::test]
async fn perf_doctors_fts_at_200_under_50ms_p99() {
    let pool = fresh_pool().await;
    seed_doctors(&pool, 200).await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let mut samples = Vec::new();
    for _ in 0..30 {
        let t0 = Instant::now();
        let _ = repo.search_fts(ENTITY_ID, "Doctor", false).await.unwrap();
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let threshold = std::time::Duration::from_millis(if cfg!(debug_assertions) { 500 } else { 50 });
    assert_p99(samples, threshold, "doctors::search_fts at 200");
}

#[tokio::test]
async fn perf_inventory_catalog_active_at_500_under_30ms_p99() {
    let pool = fresh_pool().await;
    seed_inventory(&pool, 500).await;
    let repo = SqliteInventoryItemRepo::new(pool.clone());
    let mut samples = Vec::new();
    for _ in 0..30 {
        let t0 = Instant::now();
        let _ = repo
            .list(CatalogListFilter {
                entity_id: ENTITY_ID.into(),
                include_deleted: false,
                include_inactive: false,
                query: None,
            })
            .await
            .unwrap();
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let threshold = std::time::Duration::from_millis(if cfg!(debug_assertions) { 200 } else { 30 });
    assert_p99(samples, threshold, "inventory_catalog::list at 500");
}

#[tokio::test]
async fn perf_effective_price_single_call_under_5ms_p99() {
    let pool = fresh_pool().await;
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let sub_repo: Arc<dyn CheckSubtypeRepo> = Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let price_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let doc_repo = SqliteDoctorRepo::new(pool.clone());
    let ct = flat_check_type("Echo", 30_000);
    let doc = doctor("Sami");
    let p = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: doc.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: Some(25_000),
        cut_kind: CutKind::Pct,
        cut_value: 30,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    doc_repo.upsert(&mut tx, &doc).await.unwrap();
    price_repo.upsert(&mut tx, &p).await.unwrap();
    tx.commit().await.unwrap();

    let resolver = PricingResolver::new(ct_repo, sub_repo, price_repo);
    let mut samples = Vec::new();
    for _ in 0..50 {
        let t0 = Instant::now();
        let _ = resolver
            .effective_price(EffectivePriceQuery {
                doctor_id: Some(doc.id),
                check_type_id: ct.id,
                check_subtype_id: None,
            })
            .await
            .unwrap();
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let threshold = std::time::Duration::from_millis(if cfg!(debug_assertions) { 100 } else { 5 });
    assert_p99(samples, threshold, "effective_price single call");
}

#[tokio::test]
async fn perf_catalog_create_throughput_at_least_30_per_sec_in_debug() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    let t0 = Instant::now();
    let n = 60;
    for i in 0..n {
        let ct = flat_check_type(&format!("T {i:04}"), 1000);
        let mut tx = pool.begin().await.unwrap();
        repo.upsert(&mut tx, &ct).await.unwrap();
        tx.commit().await.unwrap();
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let throughput = (n as f64) / elapsed;
    // 30 ops/sec floor in debug builds.
    assert!(
        throughput >= 30.0,
        "throughput {throughput:.1} ops/sec below 30 floor"
    );
}

#[tokio::test]
async fn perf_doctor_fts_prefix_under_50ms_p99_in_release() {
    if cfg!(debug_assertions) {
        // Skip in debug; threshold uses release-tier assertion.
        return;
    }
    let pool = fresh_pool().await;
    seed_doctors(&pool, 200).await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let mut samples = Vec::new();
    for _ in 0..30 {
        let t0 = Instant::now();
        let _ = repo
            .search_fts(ENTITY_ID, "Doctor 01", false)
            .await
            .unwrap();
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    assert_p99(samples, std::time::Duration::from_millis(50), "fts prefix");
}
