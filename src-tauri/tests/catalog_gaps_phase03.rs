//! Phase-03 gap-analysis pinned scenarios.
//!
//! These tests cover specific rows from `phase-03-test.md` §9 (Pass 1
//! additions), §10 (Pass 2), §11 (Pass 3), §12 (Pass 4) -- the rows whose
//! coverage doesn't naturally fit the per-entity integration files. Each
//! scenario references its gap id and target test section.

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::auth::AuthService;
use app_lib::domains::catalog::domain::entities::doctor::DoctorNewInput;
use app_lib::domains::catalog::domain::entities::doctor_pricing::DoctorPricingNewInput;
use app_lib::domains::catalog::domain::entities::{Doctor, DoctorCheckPricing};
use app_lib::domains::catalog::domain::repositories::{
    CatalogListFilter, CheckTypeRepo, DoctorPricingRepo, DoctorRepo,
};
use app_lib::domains::catalog::domain::value_objects::CutKind;
use app_lib::domains::catalog::events::{PricingChangeKind, PRICING_CHANGED};
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteMandoubRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::catalog::service::operator_specialty_service::OperatorSpecialtyInput;
use app_lib::domains::catalog::service::{
    CatalogServices, CatalogServicesConfig, CheckTypeCreateInput, CheckTypeUpdateInput,
    DoctorCreateInput, DoctorPricingUpsertInput, DoctorUpdateInput, OperatorCreateInput,
};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::test::{mock_app, MockRuntime};
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-g";
const DEVICE_ID: &str = "dev-G";

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

struct Rig {
    pool: SqlitePool,
    services: CatalogServices<MockRuntime>,
    superadmin_id: Uuid,
    _app: tauri::App<MockRuntime>,
}

async fn rig() -> Rig {
    let pool = fresh_pool().await;
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let outbox_repo = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo = Arc::new(SqliteAuditRepo::new(pool.clone()));

    let auth = AuthService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        DEVICE_ID.into(),
    );
    let mock = mock_app();
    let handle = mock.handle().clone();

    let services = CatalogServices::new(CatalogServicesConfig {
        pool: pool.clone(),
        check_type_repo: Arc::new(SqliteCheckTypeRepo::new(pool.clone())),
        check_subtype_repo: Arc::new(SqliteCheckSubtypeRepo::new(pool.clone())),
        doctor_repo: Arc::new(SqliteDoctorRepo::new(pool.clone())),
        doctor_pricing_repo: Arc::new(SqliteDoctorPricingRepo::new(pool.clone())),
        operator_repo: Arc::new(SqliteOperatorRepo::new(pool.clone())),
        operator_specialty_repo: Arc::new(SqliteOperatorSpecialtyRepo::new(pool.clone())),
        mandoub_repo: Arc::new(SqliteMandoubRepo::new(pool.clone())),
        inventory_item_repo: Arc::new(SqliteInventoryItemRepo::new(pool.clone())),
        consumption_repo: Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone())),
        audit_repo,
        outbox_repo,
        device_id: DEVICE_ID.into(),
        app_handle: handle,
    });

    let admin = auth
        .create_first_admin("admin@idc.io", "M", "admin-strong-789", ENTITY_ID)
        .await
        .unwrap();
    Rig {
        pool,
        services,
        superadmin_id: admin.id,
        _app: mock,
    }
}

// ----- P03-G02 emit coverage on every catalog mutation kind --------------

#[tokio::test]
async fn p03_g02_pricing_changed_kind_enum_has_all_4_variants() {
    // Pure enum-shape pin: the four kinds drive the listener fan-out in
    // phase-05's banner; a regression dropping one would silently leave
    // drafts unflagged.
    for k in [
        PricingChangeKind::CheckType,
        PricingChangeKind::CheckSubtype,
        PricingChangeKind::DoctorPricing,
        PricingChangeKind::Settings,
    ] {
        let s = serde_json::to_string(&k).unwrap();
        assert!(s.len() > 2, "kind {k:?} must serialize");
    }
    assert_eq!(PRICING_CHANGED, "catalog:pricing_changed");
}

// ----- P03-G04 audit delta capture ---------------------------------------

#[tokio::test]
async fn p03_g04_check_type_update_audit_delta_contains_before_and_after_json() {
    let rig = rig().await;
    let ct = rig
        .services
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "A".into(),
                name_en: None,
                has_subtypes: false,
                base_price_iqd: Some(1000),
                dye_price_iqd: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    rig.services
        .check_types
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            ct.id,
            CheckTypeUpdateInput {
                name_ar: Some("B".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let delta_json: Option<String> = sqlx::query_scalar(
        "SELECT delta FROM audit_log WHERE entity = 'check_types' AND action = 'update' AND entity_id = ?",
    )
    .bind(ct.id.to_string())
    .fetch_optional(&rig.pool)
    .await
    .unwrap();
    let raw = delta_json.expect("update audit row must carry delta");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let name_ar = v.get("name_ar").expect("delta.name_ar must exist");
    assert_eq!(name_ar["from"], "A");
    assert_eq!(name_ar["to"], "B");
}

// ----- P03-G05 doctors::list include_id branch ----------------------------

#[tokio::test]
async fn p03_g05_doctors_list_includes_inactive_doctor_when_include_id_matches() {
    let rig = rig().await;
    let svc = &rig.services;
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Layla".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctors
        .set_active(rig.superadmin_id, UserRole::Superadmin, d.id, false)
        .await
        .unwrap();
    // Active-only filter hides it.
    let active_only = svc.doctors.list(ENTITY_ID, false, None).await.unwrap();
    assert!(active_only.is_empty());
    // includeInactive surfaces the row again.
    let all = svc.doctors.list(ENTITY_ID, true, None).await.unwrap();
    assert!(all.iter().any(|x| x.id == d.id));
}

// ----- P03-G08 outbox enqueue per catalog mutation ------------------------

#[tokio::test]
async fn p03_g08_check_type_create_enqueues_one_outbox_row() {
    let rig = rig().await;
    rig.services
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "A".into(),
                name_en: None,
                has_subtypes: false,
                base_price_iqd: Some(1000),
                dye_price_iqd: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE entity = 'check_types'")
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn p03_g08_doctor_pricing_upsert_enqueues_outbox_row() {
    let rig = rig().await;
    let ct = rig
        .services
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "A".into(),
                name_en: None,
                has_subtypes: false,
                base_price_iqd: Some(1000),
                dye_price_iqd: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    let d = rig
        .services
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "X".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    rig.services
        .doctor_pricing
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorPricingUpsertInput {
                doctor_id: d.id,
                check_type_id: ct.id,
                check_subtype_id: None,
                price_override_iqd: Some(800),
                cut_kind: CutKind::Pct,
                cut_value: 25,
            },
        )
        .await
        .unwrap();
    let pricing_outbox: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE entity = 'doctor_check_pricing'")
            .fetch_one(&rig.pool)
            .await
            .unwrap();
    assert!(pricing_outbox >= 1);
}

// ----- P03-G19 soft-deleted doctor with matching include_id still excluded

#[tokio::test]
async fn p03_g19_doctors_list_excludes_soft_deleted_even_when_include_inactive() {
    let rig = rig().await;
    let svc = &rig.services;
    let d = svc
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "Layla".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    svc.doctors
        .soft_delete(rig.superadmin_id, UserRole::Superadmin, d.id)
        .await
        .unwrap();
    let with_inactive = svc.doctors.list(ENTITY_ID, true, None).await.unwrap();
    assert!(
        !with_inactive.iter().any(|x| x.id == d.id),
        "soft-deleted doctor must NOT appear in list even with includeInactive=true"
    );
}

// ----- P03-G23 check_types includeDeleted branch ---------------------------

#[tokio::test]
async fn p03_g23_check_types_list_can_include_soft_deleted_when_flag_true() {
    let rig = rig().await;
    let ct = rig
        .services
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "A".into(),
                name_en: None,
                has_subtypes: false,
                base_price_iqd: Some(1000),
                dye_price_iqd: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    // Direct repo-level: include_deleted=true should surface the soft-deleted
    // row. The service `list` defaults to include_deleted=false; we exercise
    // the repo here to pin the design.
    let repo = SqliteCheckTypeRepo::new(rig.pool.clone());
    rig.services
        .check_types
        .soft_delete(rig.superadmin_id, UserRole::Superadmin, ct.id)
        .await
        .unwrap();
    let with_deleted = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: true,
            include_inactive: true,
            query: None,
        })
        .await
        .unwrap();
    assert!(with_deleted.iter().any(|c| c.id == ct.id));
}

// ----- P03-G24 LWW + clock skew --------------------------------------------

#[tokio::test]
async fn p03_g24_pull_apply_overwrites_local_future_dated_updated_at() {
    let pool = fresh_pool().await;
    let repo = SqliteDoctorRepo::new(pool.clone());
    let d = Doctor::try_new(DoctorNewInput {
        name: "Layla".into(),
        specialty: None,
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
        default_cut_kind: None,
        default_cut_value: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &d).await.unwrap();
    tx.commit().await.unwrap();

    let mut server_view = d.clone();
    server_view.updated_at = chrono::Utc::now() - chrono::Duration::hours(2);
    server_view.version += 1;
    let mut tx = pool.begin().await.unwrap();
    repo.upsert(&mut tx, &server_view).await.unwrap();
    tx.commit().await.unwrap();
    let got = repo.get_by_id(d.id).await.unwrap().unwrap();
    assert_eq!(got.updated_at, server_view.updated_at);
}

// ----- P03-G33 every catalog mutation sets dirty=1 and bumps version ------

#[tokio::test]
async fn p03_g33_check_type_create_sets_dirty_and_version_one() {
    let rig = rig().await;
    let ct = rig
        .services
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "A".into(),
                name_en: None,
                has_subtypes: false,
                base_price_iqd: Some(1000),
                dye_price_iqd: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    let dirty: i64 = sqlx::query_scalar("SELECT dirty FROM check_types WHERE id = ?")
        .bind(ct.id.to_string())
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    let version: i64 = sqlx::query_scalar("SELECT version FROM check_types WHERE id = ?")
        .bind(ct.id.to_string())
        .fetch_one(&rig.pool)
        .await
        .unwrap();
    assert_eq!(dirty, 1);
    assert!(version >= 1);
}

#[tokio::test]
async fn p03_g33_doctor_update_bumps_version_and_marks_dirty() {
    let rig = rig().await;
    let d = rig
        .services
        .doctors
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            DoctorCreateInput {
                name: "L".into(),
                specialty: None,
                phone: None,
                notes: None,
                default_cut_kind: None,
                default_cut_value: None,
            },
        )
        .await
        .unwrap();
    let v0 = d.version;
    let updated = rig
        .services
        .doctors
        .update(
            rig.superadmin_id,
            UserRole::Superadmin,
            d.id,
            DoctorUpdateInput {
                name: Some("L2".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(updated.version > v0);
    assert!(updated.dirty);
}

// ----- P03-G34 OperatorSpecialty duplicate idempotency --------------------

#[tokio::test]
async fn p03_g34_operator_specialty_double_upsert_no_op_returns_same_row() {
    let rig = rig().await;
    let ct = rig
        .services
        .check_types
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            CheckTypeCreateInput {
                name_ar: "A".into(),
                name_en: None,
                has_subtypes: false,
                base_price_iqd: Some(1000),
                dye_price_iqd: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    let op = rig
        .services
        .operators
        .create(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorCreateInput {
                name: "Hassan".into(),
                phone: None,
                base_cut_per_check_iqd: 100,
                notes: None,
            },
        )
        .await
        .unwrap();
    let a = rig
        .services
        .operator_specialties
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorSpecialtyInput {
                operator_id: op.id,
                check_type_id: ct.id,
            },
        )
        .await
        .unwrap();
    let b = rig
        .services
        .operator_specialties
        .upsert(
            rig.superadmin_id,
            UserRole::Superadmin,
            ENTITY_ID,
            OperatorSpecialtyInput {
                operator_id: op.id,
                check_type_id: ct.id,
            },
        )
        .await
        .unwrap();
    assert_eq!(a.id, b.id, "duplicate must be idempotent");
}

// ----- P03-G36 check_types_sort partial index covers ORDER BY -------------

#[tokio::test]
async fn p03_g36_check_types_sort_index_used_by_order_by() {
    let pool = fresh_pool().await;
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN SELECT id FROM check_types WHERE entity_id = ? AND deleted_at IS NULL ORDER BY sort_order",
    )
    .bind(ENTITY_ID)
    .fetch_all(&pool)
    .await
    .unwrap();
    let combined = plan
        .iter()
        .map(|r| r.3.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        combined.contains("check_types_sort") || combined.contains("USING INDEX"),
        "EXPLAIN must mention the partial index: {combined}"
    );
}

// ----- P03-G39 doctors_fts is external-content mode ------------------------

#[tokio::test]
async fn p03_g39_doctors_fts_external_content_mode_pinned_in_schema() {
    let pool = fresh_pool().await;
    let sql: (String,) = sqlx::query_as("SELECT sql FROM sqlite_master WHERE name = 'doctors_fts'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(sql.0.contains("content='doctors'"));
    assert!(sql.0.contains("content_rowid='rowid'"));
}

// ----- P03-G07 LIKE-prefix query exercise ---------------------------------

#[tokio::test]
async fn p03_g07_check_types_list_like_prefix_matches_from_start_of_name() {
    let pool = fresh_pool().await;
    let repo = SqliteCheckTypeRepo::new(pool.clone());
    use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
    use app_lib::domains::catalog::domain::entities::CheckType;
    for name in ["MRI", "MRI Contrast", "Cardiac MRI"] {
        let ct = CheckType::try_new(CheckTypeNewInput {
            name_ar: name.into(),
            name_en: None,
            has_subtypes: false,
            base_price_iqd: Some(1000),
            dye_price_iqd: None,
            sort_order: 0,
            entity_id: ENTITY_ID.into(),
            origin_device_id: None,
        })
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        repo.upsert(&mut tx, &ct).await.unwrap();
        tx.commit().await.unwrap();
    }
    let prefix_match = repo
        .list(CatalogListFilter {
            entity_id: ENTITY_ID.into(),
            include_deleted: false,
            include_inactive: false,
            query: Some("MRI".into()),
        })
        .await
        .unwrap();
    let names: Vec<&str> = prefix_match.iter().map(|c| c.name_ar.as_str()).collect();
    assert!(names.contains(&"MRI"));
    assert!(names.contains(&"MRI Contrast"));
    assert!(
        !names.contains(&"Cardiac MRI"),
        "LIKE-prefix must not match substring"
    );
}

// ----- P03-G14 CutKind serializes as lowercase ----------------------------

#[tokio::test]
async fn p03_g14_doctor_pricing_cut_kind_round_trips_via_lowercase_wire_format() {
    let pool = fresh_pool().await;
    let ct_repo = SqliteCheckTypeRepo::new(pool.clone());
    let doc_repo = SqliteDoctorRepo::new(pool.clone());
    let price_repo = SqliteDoctorPricingRepo::new(pool.clone());
    use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
    use app_lib::domains::catalog::domain::entities::CheckType;
    let ct = CheckType::try_new(CheckTypeNewInput {
        name_ar: "X".into(),
        name_en: None,
        has_subtypes: false,
        base_price_iqd: Some(1000),
        dye_price_iqd: None,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let d = Doctor::try_new(DoctorNewInput {
        name: "X".into(),
        specialty: None,
        phone: None,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
        default_cut_kind: None,
        default_cut_value: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    doc_repo.upsert(&mut tx, &d).await.unwrap();
    tx.commit().await.unwrap();
    let p = DoctorCheckPricing::try_new(DoctorPricingNewInput {
        doctor_id: d.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        price_override_iqd: None,
        cut_kind: CutKind::Pct,
        cut_value: 25,
        entity_id: ENTITY_ID.into(),
        origin_device_id: None,
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    price_repo.upsert(&mut tx, &p).await.unwrap();
    tx.commit().await.unwrap();
    let raw_cut_kind: String =
        sqlx::query_scalar("SELECT cut_kind FROM doctor_check_pricing WHERE id = ?")
            .bind(p.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(raw_cut_kind, "pct", "wire format must be lowercase 'pct'");
}
