//! Phase-02 integration: `SettingsService` update path with role gate, key
//! validation, and audit-first ordering.

use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::settings::domain::repositories::SettingRepo;
use app_lib::domains::settings::domain::value_objects::SettingValue;
use app_lib::domains::settings::infrastructure::SqliteSettingRepo;
use app_lib::domains::settings::service::SettingsService;
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

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

fn make_service(pool: &SqlitePool, device_id: &str) -> (SettingsService, Arc<SqliteSettingRepo>) {
    let repo = Arc::new(SqliteSettingRepo::new(pool.clone()));
    let svc = SettingsService::new(
        pool.clone(),
        repo.clone(),
        Arc::new(SqliteAuditRepo::new(pool.clone())),
        Arc::new(SqliteOutboxRepo::new(pool.clone())),
        device_id.to_string(),
    );
    (svc, repo)
}

#[tokio::test]
async fn update_inserts_new_setting_when_key_missing() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    let s = svc
        .update(
            actor,
            UserRole::Superadmin,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(10_000),
        )
        .await
        .unwrap();
    assert_eq!(s.key, "dye_cost_iqd");
    assert_eq!(s.value, SettingValue::Int(10_000));
    assert_eq!(s.entity_id, "tenant-1");
    assert_eq!(s.version, 1);

    let stored = repo
        .get_by_key("dye_cost_iqd", "tenant-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.value, SettingValue::Int(10_000));
}

#[tokio::test]
async fn update_bumps_version_when_key_already_exists() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    let v1 = svc
        .update(
            actor,
            UserRole::Superadmin,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(10_000),
        )
        .await
        .unwrap();
    let v2 = svc
        .update(
            actor,
            UserRole::Superadmin,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(12_000),
        )
        .await
        .unwrap();
    assert_eq!(v1.version, 1);
    assert_eq!(v2.version, 2);
    assert_eq!(v2.value, SettingValue::Int(12_000));
}

#[tokio::test]
async fn update_writes_audit_row_with_action_update_and_entity_settings() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "currency_symbol",
        SettingValue::Text("د.ع".into()),
    )
    .await
    .unwrap();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "currency_symbol",
        SettingValue::Text("IQD".into()),
    )
    .await
    .unwrap();

    let (rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'settings' AND action = 'update'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn update_enqueues_settings_outbox_row_per_write() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "thermal_width",
        SettingValue::Int(32),
    )
    .await
    .unwrap();

    let (rows,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'settings'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // One outbox row for settings, plus its audit_log outbox.
    assert!(rows >= 1, "settings outbox row should be enqueued");
}

#[tokio::test]
async fn update_rejects_receptionist_caller_via_role_gate() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Receptionist,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(10_000),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_rejects_accountant_caller_via_role_gate() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Accountant,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(10_000),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_rejects_thermal_width_value_not_in_32_or_48() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Superadmin,
            "tenant-1",
            "thermal_width",
            SettingValue::Int(64),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_rejects_internal_doctor_pct_above_100() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Superadmin,
            "tenant-1",
            "internal_doctor_pct",
            SettingValue::Int(150),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_rejects_idle_lock_minutes_zero_and_negative() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    for invalid in [0_i64, -5] {
        let err = svc
            .update(
                Uuid::now_v7(),
                UserRole::Superadmin,
                "tenant-1",
                "idle_lock_minutes",
                SettingValue::Int(invalid),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(_)),
            "{invalid} should be rejected"
        );
    }
}

#[tokio::test]
async fn update_rejects_dye_cost_negative_int_and_wrong_type() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Superadmin,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Int(-1),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Superadmin,
            "tenant-1",
            "dye_cost_iqd",
            SettingValue::Text("10000".into()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_rejects_arabic_numerals_non_bool_value() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let err = svc
        .update(
            Uuid::now_v7(),
            UserRole::Superadmin,
            "tenant-1",
            "arabic_numerals",
            SettingValue::Int(1),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn update_isolates_settings_by_tenant() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-A",
        "dye_cost_iqd",
        SettingValue::Int(10_000),
    )
    .await
    .unwrap();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-B",
        "dye_cost_iqd",
        SettingValue::Int(99_000),
    )
    .await
    .unwrap();

    let a = repo
        .get_by_key("dye_cost_iqd", "tenant-A")
        .await
        .unwrap()
        .unwrap();
    let b = repo
        .get_by_key("dye_cost_iqd", "tenant-B")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.value, SettingValue::Int(10_000));
    assert_eq!(b.value, SettingValue::Int(99_000));
}

#[tokio::test]
async fn list_returns_only_rows_for_the_requested_tenant() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-A",
        "arabic_numerals",
        SettingValue::Bool(true),
    )
    .await
    .unwrap();
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-B",
        "arabic_numerals",
        SettingValue::Bool(false),
    )
    .await
    .unwrap();
    let listed = repo.list("tenant-A").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entity_id, "tenant-A");
}

#[tokio::test]
async fn get_returns_none_for_unknown_key() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let out = svc.get("ghost_key", "tenant-1").await.unwrap();
    assert!(out.is_none());
}

#[tokio::test]
async fn list_returns_empty_vec_for_fresh_tenant() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let out = svc.list("tenant-fresh").await.unwrap();
    assert!(out.is_empty());
}

#[tokio::test]
async fn list_live_by_entity_returns_only_live_rows_for_scope() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    // Seed migration already inserted the 'unscoped' rows; add a tenant row.
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "dye_cost_iqd",
        SettingValue::Int(60_000),
    )
    .await
    .unwrap();

    let unscoped = repo.list_live_by_entity("unscoped").await.unwrap();
    assert!(
        unscoped
            .iter()
            .all(|s| s.entity_id == "unscoped" && s.deleted_at.is_none()),
        "only live unscoped rows"
    );
    assert!(
        unscoped.iter().any(|s| s.key == "dye_cost_iqd"),
        "the unscoped dye seed is present"
    );

    let tenant = repo.list_live_by_entity("tenant-1").await.unwrap();
    assert_eq!(tenant.len(), 1);
    assert_eq!(tenant[0].key, "dye_cost_iqd");
    assert_eq!(tenant[0].value, SettingValue::Int(60_000));
}

#[tokio::test]
async fn has_live_key_true_only_for_live_scoped_row() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    assert!(repo.has_live_key("dye_cost_iqd", "unscoped").await.unwrap());
    assert!(!repo.has_live_key("dye_cost_iqd", "tenant-1").await.unwrap());

    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "dye_cost_iqd",
        SettingValue::Int(60_000),
    )
    .await
    .unwrap();
    assert!(repo.has_live_key("dye_cost_iqd", "tenant-1").await.unwrap());
}

#[tokio::test]
async fn update_row_by_id_applies_tombstone_and_repoint_without_conflict() {
    let pool = fresh_pool().await;
    let (_svc, repo) = make_service(&pool, "dev-A");

    // Take a live 'unscoped' seed row and tombstone it via update_row_by_id.
    let row = repo
        .get_by_key("dye_cost_iqd", "unscoped")
        .await
        .unwrap()
        .unwrap();
    let tomb = row.clone().tombstoned();
    let mut tx = pool.begin().await.unwrap();
    repo.update_row_by_id(&mut tx, &tomb).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        repo.get_by_key("dye_cost_iqd", "unscoped")
            .await
            .unwrap()
            .is_none(),
        "tombstone hides the row"
    );

    // Re-point a different live 'unscoped' row to a tenant; no conflict, value kept.
    let cfg = repo
        .get_by_key("arabic_numerals", "unscoped")
        .await
        .unwrap()
        .unwrap();
    let repointed = cfg.clone().repointed_to("tenant-1");
    let mut tx = pool.begin().await.unwrap();
    repo.update_row_by_id(&mut tx, &repointed).await.unwrap();
    tx.commit().await.unwrap();
    assert!(repo
        .get_by_key("arabic_numerals", "unscoped")
        .await
        .unwrap()
        .is_none());
    let moved = repo
        .get_by_key("arabic_numerals", "tenant-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.value, cfg.value);
}

#[tokio::test]
async fn reconcile_scope_tombstones_duplicates_and_repoints_singletons() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();
    let tenant = "3627804e-3594-4d6f-9e8c-b157e460e7f4";

    // Tenant already edited dye + report_pct + internal (the accountant's values).
    for (k, v) in [
        ("dye_cost_iqd", 60_000),
        ("report_pct", 25),
        ("internal_doctor_pct", 25),
    ] {
        svc.update(actor, UserRole::Superadmin, tenant, k, SettingValue::Int(v))
            .await
            .unwrap();
    }

    let out = svc.reconcile_scope(tenant).await.unwrap();
    assert!(
        out.tombstoned >= 3,
        "3 money keys had tenant dupes to tombstone"
    );
    assert!(out.repointed >= 1, "config-only keys got re-pointed");

    // No live 'unscoped' rows remain.
    let unscoped = repo.list_live_by_entity("unscoped").await.unwrap();
    assert!(
        unscoped.is_empty(),
        "unscoped fully folded, got {unscoped:?}"
    );

    // Tenant keeps the edited money values (tombstone won, not re-point).
    let dye = repo
        .get_by_key("dye_cost_iqd", tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dye.value, SettingValue::Int(60_000));
    let rp = repo
        .get_by_key("report_pct", tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rp.value, SettingValue::Int(25));

    // A config-only key (arabic_numerals) is now under the tenant with its value.
    let an = repo
        .get_by_key("arabic_numerals", tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(an.value, SettingValue::Bool(false));
    assert!(repo
        .get_by_key("arabic_numerals", "unscoped")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn reconcile_scope_is_idempotent() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let tenant = "tenant-1";

    let first = svc.reconcile_scope(tenant).await.unwrap();
    assert!(
        first.repointed + first.tombstoned > 0,
        "first run does work"
    );

    let second = svc.reconcile_scope(tenant).await.unwrap();
    assert_eq!(second.repointed, 0);
    assert_eq!(second.tombstoned, 0);

    assert!(repo
        .list_live_by_entity("unscoped")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn reconcile_scope_noop_for_unscoped_tenant() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");

    let out = svc.reconcile_scope("unscoped").await.unwrap();
    assert_eq!(out.repointed, 0);
    assert_eq!(out.tombstoned, 0);
    // Unscoped rows are untouched (still live) when there is no real tenant.
    assert!(!repo
        .list_live_by_entity("unscoped")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn reconcile_scope_enqueues_one_outbox_op_per_changed_row() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let tenant = "tenant-1";

    let out = svc.reconcile_scope(tenant).await.unwrap();
    let changed = out.repointed + out.tombstoned;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'settings'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0 as usize, changed, "one settings op per changed row");
}

/// Guard test for Task 4 (`AppState::reconcile_and_warm_settings`): confirms
/// the service-level contract the cache re-warm loop depends on -- after
/// `reconcile_scope`, `list(tenant)` carries the tenant money values (not
/// seed defaults) and the unscoped scope is left empty.
#[tokio::test]
async fn reconcile_then_list_yields_tenant_scoped_money_values() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();
    let tenant = "tenant-1";

    for (k, v) in [
        ("dye_cost_iqd", 60_000),
        ("report_pct", 25),
        ("internal_doctor_pct", 25),
    ] {
        svc.update(actor, UserRole::Superadmin, tenant, k, SettingValue::Int(v))
            .await
            .unwrap();
    }

    svc.reconcile_scope(tenant).await.unwrap();

    // What the cache-warm loop reads after login: list(tenant) must carry the
    // tenant money values AND the re-pointed config keys, with no unscoped rows.
    let rows = svc.list(tenant).await.unwrap();
    let get = |k: &str| rows.iter().find(|s| s.key == k).map(|s| s.value.clone());
    assert_eq!(get("dye_cost_iqd"), Some(SettingValue::Int(60_000)));
    assert_eq!(get("report_pct"), Some(SettingValue::Int(25)));
    assert_eq!(get("internal_doctor_pct"), Some(SettingValue::Int(25)));
    assert_eq!(get("arabic_numerals"), Some(SettingValue::Bool(false)));
    assert!(svc.list("unscoped").await.unwrap().is_empty());
}
