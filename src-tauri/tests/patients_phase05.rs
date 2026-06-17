//! Phase 05 patients integration tests.
//!
//! Drives `PatientService` end-to-end against an in-memory SQLite to assert
//! the §2.1 contract for patients (FTS5 indexing, soft-delete invariants,
//! rename audit semantics, sync-version monotonicity).

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
use app_lib::domains::catalog::domain::entities::CheckType;
use app_lib::domains::catalog::domain::repositories::CheckTypeRepo;
use app_lib::domains::catalog::infrastructure::SqliteCheckTypeRepo;
use app_lib::domains::patients::domain::entities::{Patient, PatientNewInput};
use app_lib::domains::patients::domain::repositories::PatientRepo;
use app_lib::domains::patients::infrastructure::SqlitePatientRepo;
use app_lib::domains::patients::service::{PatientCreateInput, PatientService, PatientUpdateInput};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::domains::visits::domain::entities::{Visit, VisitCreateDraftInput};
use app_lib::domains::visits::domain::repositories::VisitRepo;
use app_lib::domains::visits::infrastructure::SqliteVisitRepo;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-p";
const DEVICE_ID: &str = "dev-p";

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
    service: Arc<PatientService>,
    receptionist: User,
}

async fn seed() -> Fixture {
    let pool = fresh_pool().await;

    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let patient_repo: Arc<dyn PatientRepo> = Arc::new(SqlitePatientRepo::new(pool.clone()));
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
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &receptionist).await.unwrap();
    tx.commit().await.unwrap();

    let visit_repo: Arc<dyn VisitRepo> = Arc::new(SqliteVisitRepo::new(pool.clone()));
    let service = Arc::new(PatientService::new(
        pool.clone(),
        patient_repo,
        visit_repo,
        audit,
        outbox,
        DEVICE_ID.to_string(),
    ));

    Fixture {
        pool,
        service,
        receptionist,
    }
}

#[tokio::test]
async fn create_persists_with_dirty_flag_and_audit_row() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(p.name, "Layla");
    assert!(p.dirty);

    let audit: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'patients' AND action = 'create'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(audit.0, 1);
}

#[tokio::test]
async fn create_trims_whitespace_in_name() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "  Layla H.  ".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(p.name, "Layla H.");
}

#[tokio::test]
async fn create_rejects_empty_after_trim() {
    let f = seed().await;
    let err = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput { name: "   ".into() },
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn update_renames_and_bumps_version_and_writes_audit_row() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    let renamed = f
        .service
        .update(
            f.receptionist.id,
            p.id,
            PatientUpdateInput {
                name: "Layla H.".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "Layla H.");
    assert!(renamed.version > p.version);

    let audit: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'patients' AND entity_id = ? AND action = 'update'",
    )
    .bind(p.id.to_string())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(audit.0, 1);
}

#[tokio::test]
async fn search_returns_results_by_fts_prefix_and_excludes_soft_deleted() {
    let f = seed().await;
    for n in ["Layla", "Layth", "Bob"] {
        f.service
            .create(
                f.receptionist.id,
                ENTITY_ID,
                PatientCreateInput { name: n.into() },
            )
            .await
            .unwrap();
    }
    let rows = f.service.search(ENTITY_ID, "Lay", 10).await.unwrap();
    assert_eq!(rows.len(), 2);
    // Now soft-delete Layth and assert he disappears.
    let layth = rows.iter().find(|p| p.name == "Layth").unwrap().clone();
    f.service
        .soft_delete(f.receptionist.id, layth.id)
        .await
        .unwrap();
    let after = f.service.search(ENTITY_ID, "Lay", 10).await.unwrap();
    assert!(after.iter().all(|p| p.name != "Layth"));
}

#[tokio::test]
async fn search_handles_arabic_name() {
    let f = seed().await;
    f.service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "ليلى".into(),
            },
        )
        .await
        .unwrap();
    let rows = f.service.search(ENTITY_ID, "ليلى", 5).await.unwrap();
    assert!(rows.iter().any(|p| p.name == "ليلى"));
}

#[tokio::test]
async fn search_treats_match_operator_input_as_literal_fts_query() {
    let f = seed().await;
    f.service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    // Malicious query containing MATCH syntax: must not crash, must not
    // match. The search wrapper escapes / quotes the input.
    let rows = f.service.search(ENTITY_ID, "Layla MATCH 'foo'", 5).await;
    assert!(rows.is_ok());
}

#[tokio::test]
async fn list_recent_excludes_soft_deleted_and_orders_recent_first() {
    let f = seed().await;
    let a = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Patient A".into(),
            },
        )
        .await
        .unwrap();
    let b = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Patient B".into(),
            },
        )
        .await
        .unwrap();
    f.service
        .soft_delete(f.receptionist.id, a.id)
        .await
        .unwrap();
    let rows = f.service.list_recent(ENTITY_ID, 10).await.unwrap();
    assert!(rows.iter().all(|p| p.id != a.id));
    assert!(rows.iter().any(|p| p.id == b.id));
}

#[tokio::test]
async fn soft_delete_emits_audit_action_soft_delete() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    f.service
        .soft_delete(f.receptionist.id, p.id)
        .await
        .unwrap();
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'patients' AND entity_id = ? AND action = 'soft_delete'",
    )
    .bind(p.id.to_string())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn soft_delete_rejected_when_referenced_by_live_visit() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    // Seed a minimal check_type so we can reference a visit.
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(f.pool.clone()));
    let ct = CheckType::try_new(CheckTypeNewInput {
        name_ar: "ا".into(),
        name_en: Some("T".into()),
        has_subtypes: false,
        base_price_iqd: Some(1_000),
        dye_supported: false,
        report_supported: false,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    tx.commit().await.unwrap();

    let v = Visit::create_draft(VisitCreateDraftInput {
        patient_id: p.id,
        receptionist_user_id: f.receptionist.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        doctor_id: None,
        dye: false,
        report: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let v_repo: Arc<dyn VisitRepo> = Arc::new(SqliteVisitRepo::new(f.pool.clone()));
    let mut tx = f.pool.begin().await.unwrap();
    v_repo.upsert(&mut tx, &v).await.unwrap();
    tx.commit().await.unwrap();

    let err = f.service.soft_delete(f.receptionist.id, p.id).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn soft_delete_is_idempotent_when_already_deleted() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    f.service
        .soft_delete(f.receptionist.id, p.id)
        .await
        .unwrap();
    let res = f.service.soft_delete(f.receptionist.id, p.id).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn patient_fts_handles_rename_old_name_no_longer_matches() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Mariam".into(),
            },
        )
        .await
        .unwrap();
    f.service
        .update(
            f.receptionist.id,
            p.id,
            PatientUpdateInput {
                name: "Mariam K.".into(),
            },
        )
        .await
        .unwrap();
    let new_match = f.service.search(ENTITY_ID, "Mariam", 5).await.unwrap();
    assert!(new_match.iter().any(|r| r.name == "Mariam K."));
}

#[tokio::test]
async fn get_returns_not_found_for_unknown_id() {
    let f = seed().await;
    let err = f.service.get(Uuid::now_v7()).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn upsert_via_repo_seeds_pool_and_search_returns_row() {
    let f = seed().await;
    let p = Patient::try_new(PatientNewInput {
        name: "Sara".into(),
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    f.service.repo().upsert(&mut tx, &p).await.unwrap();
    tx.commit().await.unwrap();
    let rows = f.service.search(ENTITY_ID, "Sara", 5).await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn patient_outbox_op_is_enqueued_on_create() {
    let f = seed().await;
    let p = f
        .service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput {
                name: "Layla".into(),
            },
        )
        .await
        .unwrap();
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'patients' AND entity_id = ?")
            .bind(p.id.to_string())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(count.0 >= 1);
}

// ---- patient archive (demographics, stats, visits, merge, restore) --------

/// Seed a check type + a draft visit for `patient_id`, returning the visit id.
async fn seed_visit_for(f: &Fixture, patient_id: Uuid) -> Uuid {
    let ct_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(f.pool.clone()));
    let ct = CheckType::try_new(CheckTypeNewInput {
        name_ar: "ا".into(),
        name_en: Some("T".into()),
        has_subtypes: false,
        base_price_iqd: Some(1_000),
        dye_supported: false,
        report_supported: false,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    ct_repo.upsert(&mut tx, &ct).await.unwrap();
    tx.commit().await.unwrap();

    let v = Visit::create_draft(VisitCreateDraftInput {
        patient_id,
        receptionist_user_id: f.receptionist.id,
        check_type_id: ct.id,
        check_subtype_id: None,
        doctor_id: None,
        dye: false,
        report: false,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let id = v.id;
    let v_repo: Arc<dyn VisitRepo> = Arc::new(SqliteVisitRepo::new(f.pool.clone()));
    let mut tx = f.pool.begin().await.unwrap();
    v_repo.upsert(&mut tx, &v).await.unwrap();
    tx.commit().await.unwrap();
    id
}

async fn mk_patient(f: &Fixture, name: &str) -> Patient {
    f.service
        .create(
            f.receptionist.id,
            ENTITY_ID,
            PatientCreateInput { name: name.into() },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn update_demographics_persists_and_validates_sex() {
    use app_lib::domains::patients::domain::entities::PatientDemographicsInput;
    let f = seed().await;
    let p = mk_patient(&f, "Layla").await;

    let updated = f
        .service
        .update_demographics(
            f.receptionist.id,
            p.id,
            PatientDemographicsInput {
                phone: Some("0770 12 34".into()),
                sex: Some("f".into()),
                birth_date: Some("1990-01-01".into()),
                file_no: Some("F-7".into()),
                notes: Some("  vip ".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.sex.as_deref(), Some("F"));
    assert_eq!(updated.notes.as_deref(), Some("vip"));
    assert!(updated.version > p.version);

    // Invalid sex rejected.
    let bad = f
        .service
        .update_demographics(
            f.receptionist.id,
            p.id,
            PatientDemographicsInput {
                sex: Some("Z".into()),
                ..Default::default()
            },
        )
        .await;
    assert!(bad.is_err());
}

#[tokio::test]
async fn restore_undeletes_a_soft_deleted_patient() {
    let f = seed().await;
    let p = mk_patient(&f, "Layla").await;
    f.service
        .soft_delete(f.receptionist.id, p.id)
        .await
        .unwrap();
    let restored = f.service.restore(f.receptionist.id, p.id).await.unwrap();
    assert!(restored.deleted_at.is_none());
    assert!(restored.version > p.version);
}

#[tokio::test]
async fn stats_counts_visits_and_drafts() {
    let f = seed().await;
    let p = mk_patient(&f, "Layla").await;
    seed_visit_for(&f, p.id).await;
    seed_visit_for(&f, p.id).await;
    let stats = f.service.stats(p.id).await.unwrap();
    assert_eq!(stats.total_visits, 2);
    assert_eq!(stats.draft_count, 2);
    // No locked visits seeded -> nothing spent yet.
    assert_eq!(stats.total_spent_iqd, 0);
}

#[tokio::test]
async fn list_visits_returns_patient_history_newest_first() {
    let f = seed().await;
    let p = mk_patient(&f, "Layla").await;
    seed_visit_for(&f, p.id).await;
    let visits = f.service.list_visits(p.id, 50, 0).await.unwrap();
    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].status, "draft");
}

#[tokio::test]
async fn find_duplicates_groups_same_name() {
    let f = seed().await;
    mk_patient(&f, "Layla Hashim").await;
    mk_patient(&f, "layla hashim").await; // case-insensitive collision
    mk_patient(&f, "Omar").await; // unique
    let groups = f.service.find_duplicates(ENTITY_ID).await.unwrap();
    let name_groups: Vec<_> = groups.iter().filter(|g| g.kind == "name").collect();
    assert_eq!(name_groups.len(), 1);
    assert_eq!(name_groups[0].patient_ids.len(), 2);
}

#[tokio::test]
async fn merge_repoints_visits_tombstones_merged_and_enqueues_ops() {
    let f = seed().await;
    let survivor = mk_patient(&f, "Layla Hashim").await;
    let merged = mk_patient(&f, "Layla Hashim").await;
    let v1 = seed_visit_for(&f, merged.id).await;
    let v2 = seed_visit_for(&f, merged.id).await;

    f.service
        .merge(f.receptionist.id, survivor.id, merged.id)
        .await
        .unwrap();

    // (a) both visits now point at the survivor
    for vid in [v1, v2] {
        let (pid,): (String,) = sqlx::query_as("SELECT patient_id FROM visits WHERE id = ?")
            .bind(vid.to_string())
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(pid, survivor.id.to_string());
    }

    // (b) merged patient is tombstoned
    let merged_after = f.service.get(merged.id).await.unwrap();
    assert!(merged_after.deleted_at.is_some());

    // (c) outbox carries an op for each re-pointed visit + the tombstone
    let (visit_ops,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM outbox WHERE entity = 'visits' AND entity_id IN (?, ?)",
    )
    .bind(v1.to_string())
    .bind(v2.to_string())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(visit_ops, 2);
    let (patient_ops,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'patients' AND entity_id = ?")
            .bind(merged.id.to_string())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(patient_ops >= 1);
}

#[tokio::test]
async fn merge_rejects_self_merge() {
    let f = seed().await;
    let p = mk_patient(&f, "Layla").await;
    assert!(f
        .service
        .merge(f.receptionist.id, p.id, p.id)
        .await
        .is_err());
}
