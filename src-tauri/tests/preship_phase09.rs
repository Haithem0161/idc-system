//! Phase-09 §2.1 -- pre-ship hardening regression tests for the 4 Rust surgical
//! edits (per `docs/idc-system/testing/phase-09-test.md` §1.1 / §2.1 / §6.8):
//!
//! 1. `domains/inventory/service/mod.rs` -- `unreachable!()` swapped for
//!    `Err(AppError::Internal(...))` at the ConsumeVisit construct switch.
//! 2. `domains/catalog/service/operator_service.rs` -- forward-reference to
//!    phase-04 removed; cascade is the documented policy.
//! 3. `lib.rs` -- `eprintln!` banner statements replaced with `tracing::info!`,
//!    gated behind `IDC_EMBEDDED_MODE`.
//!
//! The §2.1 plan also calls for static-analysis (grep) gates that pin the
//! cleanups so they cannot be re-introduced. Those grep tests run here as
//! `#[test]` functions reading source files from `CARGO_MANIFEST_DIR`.
//!
//! Crash protocol: phase-09 forbids running the full `cargo test` (it crashes
//! the IDE). Run this binary with `cargo test --test preship_phase09`.

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::auth::domain::entities::User;
use app_lib::domains::auth::domain::repositories::UserRepo;
use app_lib::domains::auth::domain::value_objects::UserRole;
use app_lib::domains::auth::infrastructure::SqliteUserRepo;
use app_lib::domains::catalog::domain::entities::check_type::CheckTypeNewInput;
use app_lib::domains::catalog::domain::entities::operator::OperatorNewInput;
use app_lib::domains::catalog::domain::entities::operator_specialty::OperatorSpecialtyNewInput;
use app_lib::domains::catalog::domain::entities::{CheckType, Operator, OperatorSpecialty};
use app_lib::domains::catalog::domain::repositories::{
    CheckTypeRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use app_lib::domains::catalog::infrastructure::{
    SqliteCheckTypeRepo, SqliteOperatorRepo, SqliteOperatorSpecialtyRepo,
};
use app_lib::domains::catalog::service::OperatorService;
use app_lib::domains::inventory::service::{
    AdjustmentInput, InventoryAdjustmentService, InventoryAdjustmentServiceConfig,
};
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use app_lib::domains::sync::infrastructure::{SqliteAuditRepo, SqliteOutboxRepo};
use app_lib::domains::visits::domain::entities::AdjustmentReason;
use app_lib::domains::visits::domain::repositories::InventoryAdjustmentRepo;
use app_lib::domains::visits::infrastructure::SqliteInventoryAdjustmentRepo;
use app_lib::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

const ENTITY_ID: &str = "tenant-preship-09";
const DEVICE_ID: &str = "dev-preship-09";

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

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_source(rel: &str) -> String {
    let path = manifest_dir().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

async fn seed_user(pool: &SqlitePool, role: UserRole) -> Uuid {
    let user_repo = SqliteUserRepo::new(pool.clone());
    let actor = User::try_new(
        "preship@example.com",
        "Preship",
        role,
        "x-hash".into(),
        ENTITY_ID.into(),
        Some(DEVICE_ID.into()),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &actor).await.unwrap();
    tx.commit().await.unwrap();
    actor.id
}

// =========================================================================
// §2.1 -- Inventory: ConsumeVisit early-return guard at the public service
// boundary (the construct-switch path itself is pinned by inline unit tests
// in `domains/inventory/service/mod.rs::tests`).
// =========================================================================

#[tokio::test]
async fn inventory_create_consume_visit_early_return_guard_blocks_at_service_entry() {
    // Phase-09 §2.1 regression: the L224-L228 early-return guard is preserved.
    // Calling `create` with `reason=ConsumeVisit` MUST be rejected at the
    // service boundary (NOT reach the construct switch). The phase-05 lock
    // workflow is the only legitimate caller of ConsumeVisit and it goes
    // through `Visit::lock` -> `InventoryAdjustment::try_consume_visit`, not
    // through the public `create` IPC path.
    let pool = fresh_pool().await;
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let items_repo = Arc::new(
        app_lib::domains::catalog::infrastructure::SqliteInventoryItemRepo::new(pool.clone()),
    );
    let consumption_repo = Arc::new(
        app_lib::domains::catalog::infrastructure::SqliteInventoryConsumptionRepo::new(
            pool.clone(),
        ),
    );
    let adjustments_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(pool.clone()));

    let svc = InventoryAdjustmentService::new(InventoryAdjustmentServiceConfig {
        pool: pool.clone(),
        items_repo,
        consumption_repo,
        adjustments_repo,
        audit_repo,
        outbox_repo,
        device_id: DEVICE_ID.into(),
    });

    let actor_id = seed_user(&pool, UserRole::Superadmin).await;
    let result = svc
        .create(
            actor_id,
            UserRole::Superadmin,
            ENTITY_ID,
            AdjustmentInput {
                item_id: Uuid::now_v7(),
                reason: AdjustmentReason::ConsumeVisit,
                delta: 1,
                note: None,
            },
        )
        .await;

    match result {
        Err(AppError::Validation(msg)) => {
            assert!(
                msg.contains("consume_visit") && msg.contains("visit lock"),
                "early-return guard message must mention the visit lock workflow, \
                 got {msg:?}",
            );
        }
        other => panic!("expected AppError::Validation from early-return guard, got {other:?}",),
    }
}

// =========================================================================
// §2.1 -- Operator soft_delete cascades specialties (documented policy,
// no forward-reference to phase-04)
// =========================================================================

async fn seed_operator_with_specialty(
    pool: &SqlitePool,
) -> (Operator, OperatorSpecialty, CheckType, OperatorService) {
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));

    let operator_repo: Arc<dyn OperatorRepo> = Arc::new(SqliteOperatorRepo::new(pool.clone()));
    let specialty_repo: Arc<dyn OperatorSpecialtyRepo> =
        Arc::new(SqliteOperatorSpecialtyRepo::new(pool.clone()));
    let check_type_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));

    let service = OperatorService::new(
        pool.clone(),
        operator_repo.clone(),
        specialty_repo.clone(),
        audit_repo,
        outbox_repo,
        DEVICE_ID.into(),
    );

    let check_type = CheckType::try_new(CheckTypeNewInput {
        name_ar: "EKG".into(),
        name_en: Some("EKG".into()),
        has_subtypes: false,
        base_price_iqd: Some(25_000),
        dye_supported: false,
        sort_order: 0,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    check_type_repo.upsert(&mut tx, &check_type).await.unwrap();
    tx.commit().await.unwrap();

    let operator = Operator::try_new(OperatorNewInput {
        name: "Kareem".into(),
        phone: None,
        base_cut_per_check_iqd: 1_000,
        notes: None,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    operator_repo.upsert(&mut tx, &operator).await.unwrap();
    tx.commit().await.unwrap();

    let specialty = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
        operator_id: operator.id,
        check_type_id: check_type.id,
        entity_id: ENTITY_ID.into(),
        origin_device_id: Some(DEVICE_ID.into()),
    })
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    specialty_repo.upsert(&mut tx, &specialty).await.unwrap();
    tx.commit().await.unwrap();

    (operator, specialty, check_type, service)
}

#[tokio::test]
async fn operator_soft_delete_cascades_specialties_per_documented_policy() {
    // Phase-09 §1.1 surgical edit #2 / #3: the forward-reference comment
    // ("phase-04 hardens this further...") was removed; cascade is the
    // documented behavior. This sentinel pins the runtime contract so the
    // documentation cleanup cannot drift from the behavior.
    let pool = fresh_pool().await;
    let actor_id = seed_user(&pool, UserRole::Superadmin).await;
    let (operator, specialty, _ct, service) = seed_operator_with_specialty(&pool).await;

    service
        .soft_delete(actor_id, UserRole::Superadmin, operator.id)
        .await
        .expect("documented cascade soft_delete must succeed");

    let live_op: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM operators WHERE id = ? AND deleted_at IS NULL")
            .bind(operator.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(live_op, 0, "operator must be soft-deleted");

    let live_specialty: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operator_specialties WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(specialty.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        live_specialty, 0,
        "specialty must cascade-soft-delete in the same transaction",
    );

    // Exactly one soft_delete audit row per operator (the cascade is an
    // implementation detail of the audit-first writer, not a separate
    // entity-level event).
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity = 'operators' \
         AND action = 'soft_delete' AND entity_id = ?",
    )
    .bind(operator.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audits, 1, "exactly one soft_delete audit row per operator");
}

// =========================================================================
// §6.8 / §2.1 -- Static-analysis (grep) gates for the cleanups
// =========================================================================

#[test]
fn grep_no_phase04_forward_reference_in_operator_service_rs() {
    // Phase-09 §1.1 surgical edit #2: stale phase-04 forward-reference removed.
    // CI gate so the comment cannot be reintroduced.
    let source = read_source("src/domains/catalog/service/operator_service.rs");
    for pattern in ["phase-04", "phase 04", "phase 4 hardens", "phase-4"] {
        assert!(
            !source.contains(pattern),
            "operator_service.rs must not reference {pattern} (phase-09 cleanup)",
        );
    }
}

#[test]
fn grep_no_eprintln_in_lib_rs() {
    // Phase-09 §1.1 surgical edit #4: `eprintln!` banner lines swapped for
    // `tracing::info!`. CI gate that `eprintln!` is not reintroduced.
    let source = read_source("src/lib.rs");
    assert!(
        !source.contains("eprintln!"),
        "lib.rs must not use eprintln! -- use tracing macros instead",
    );
}

#[test]
fn grep_inventory_construct_switch_uses_typed_error_not_unreachable() {
    // Phase-09 §1.1 surgical edit #1: the ConsumeVisit branch of the construct
    // switch returns `Err(AppError::Internal(...))`, not `unreachable!()`.
    let source = read_source("src/domains/inventory/service/mod.rs");
    let needle = "ConsumeVisit reached construction switch after early-return guard";
    assert!(
        source.contains(needle),
        "inventory service must emit the documented Internal-error message",
    );
    // The `unreachable!()` pattern must not be reintroduced anywhere in this
    // file (it was the foot-gun that prompted the cleanup).
    assert!(
        !source.contains("unreachable!()"),
        "inventory service must not use unreachable!() -- use AppError::Internal",
    );
}

// =========================================================================
// §6.8 / §2.1 -- Sync-server CI grep gates (read sibling repo files; skip
// gracefully when sync-server/ is absent, since the integration binary
// runs from src-tauri/ and the sync-server tree is optional in test envs).
// =========================================================================

fn sync_server_root() -> Option<PathBuf> {
    let candidate = manifest_dir().parent()?.join("sync-server");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn read_sync_server_sources() -> Option<Vec<(PathBuf, String)>> {
    let root = sync_server_root()?;
    let src = root.join("src");
    if !src.is_dir() {
        return None;
    }
    let mut out = Vec::new();
    visit_dir(&src, &mut out);
    Some(out)
}

fn visit_dir(dir: &PathBuf, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_dir(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e, "ts" | "js" | "mjs"))
        {
            if let Ok(contents) = fs::read_to_string(&path) {
                out.push((path, contents));
            }
        }
    }
}

#[test]
fn grep_no_dev_only_secret_fallback_in_sync_server() {
    // Phase-09 BLOCKER-3 (per pre-ship audit): the legacy
    // `JWT_SECRET ?? 'dev-only-secret'` fallback is removed. CI gate so it
    // cannot be reintroduced. Skip silently when sync-server/ is absent.
    let Some(sources) = read_sync_server_sources() else {
        eprintln!("sync-server/ not present -- skipping grep gate");
        return;
    };
    for (path, contents) in sources {
        assert!(
            !contents.contains("'dev-only-secret'") && !contents.contains("\"dev-only-secret\""),
            "{} must not embed dev-only-secret fallback (phase-09 BLOCKER-3)",
            path.display(),
        );
    }
}

#[test]
fn def_007_g19_tauri_plugin_os_registered_in_lib_rs() {
    // DEF-007 G19: the phase-02 build spec required `tauri-plugin-os` to be
    // registered in `lib.rs::run()` so the frontend can read `os::locale()`
    // for the first-launch ar-forcing detector and for diagnostics. The
    // plugin is declared in Cargo.toml AND wired into the Tauri Builder
    // chain; this sentinel pins the registration so a future cleanup of
    // the Builder chain cannot silently drop it.
    let source = read_source("src/lib.rs");
    assert!(
        source.contains("tauri_plugin_os"),
        "tauri-plugin-os import must remain in lib.rs",
    );
    assert!(
        source.contains(".plugin(tauri_plugin_os::init())"),
        "tauri-plugin-os MUST be wired into the Tauri Builder chain via .plugin(tauri_plugin_os::init())",
    );
}

#[test]
fn def_007_g19_jsonwebtoken_crate_available_for_rs256_verification() {
    // DEF-007 G19 sibling: the phase-02 spec listed `jsonwebtoken` as a
    // required dep for the future client-side RS256 verifier (G08). The
    // crate is in Cargo.toml; this sentinel pins it so a future
    // dep-cleanup pass cannot drop it before G08 lands.
    let cargo_toml = read_source("Cargo.toml");
    assert!(
        cargo_toml.contains("jsonwebtoken"),
        "jsonwebtoken crate MUST remain in Cargo.toml -- it gates the future \
         client-side RS256 verifier (DEF-007 G08)",
    );
}

#[test]
fn grep_no_sync_store_env_var_comment_in_sync_server() {
    // Phase-09 §4 cleanup: the mentioned-but-never-implemented
    // `SYNC_STORE=memory|prisma` env var must not survive as a stale comment.
    // CI gate; skip silently when sync-server/ is absent.
    let Some(sources) = read_sync_server_sources() else {
        eprintln!("sync-server/ not present -- skipping grep gate");
        return;
    };
    for (path, contents) in sources {
        for needle in [
            "SYNC_STORE=memory",
            "SYNC_STORE=prisma",
            "process.env.SYNC_STORE",
        ] {
            assert!(
                !contents.contains(needle),
                "{} contains stale {needle} reference (phase-09 §4 cleanup)",
                path.display(),
            );
        }
    }
}
