//! IDC System -- Tauri application library.
//!
//! Modes:
//! - **Standalone**: normal Tauri window app with full sync engine.
//! - **Embedded**: headless mode for Business OS integration (auth only).

pub mod config;
pub mod db;
pub mod domains;
pub mod embedded;
pub mod error;
pub mod state;
pub mod sync;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::db::migrations;
use crate::domains::audit::commands::{audit_query, audit_vacuum_now, diagnostics_summary};
use crate::domains::audit::domain::repositories::MetricsRepo;
use crate::domains::audit::infrastructure::SqliteMetricsRepo;
use crate::domains::audit::service::{
    AuditQueryService as AuditQuerySvc, AuditVacuumJob as AuditVacuumSvc,
    DiagnosticsService as DiagnosticsSvc,
};
use crate::domains::auth::commands::{
    auth_current_user, auth_is_locked, auth_lock, auth_login, auth_logout, auth_unlock,
    users_create, users_create_first_admin, users_get, users_list, users_reset_password,
    users_soft_delete, users_update,
};
use crate::domains::auth::domain::repositories::UserRepo;
use crate::domains::auth::infrastructure::SqliteUserRepo;
use crate::domains::auth::{AuthService, UserService};
use crate::domains::catalog::commands::{
    check_subtypes_create, check_subtypes_list_by_type, check_subtypes_soft_delete,
    check_subtypes_update, check_types_create, check_types_get, check_types_list,
    check_types_soft_delete, check_types_toggle_subtypes, check_types_update,
    doctor_pricing_soft_delete, doctor_pricing_upsert, doctors_create, doctors_get, doctors_list,
    doctors_set_active, doctors_soft_delete, doctors_update, inventory_catalog_create,
    inventory_catalog_get, inventory_catalog_list, inventory_catalog_soft_delete,
    inventory_catalog_update, inventory_consumption_create, inventory_consumption_list_by_type,
    inventory_consumption_soft_delete, inventory_consumption_update,
    operator_specialties_soft_delete, operator_specialties_upsert, operators_create, operators_get,
    operators_list, operators_set_active, operators_soft_delete, operators_update,
    pricing_effective,
};
use crate::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo, InventoryConsumptionRepo,
    InventoryItemRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use crate::domains::catalog::infrastructure::{
    SqliteCheckSubtypeRepo, SqliteCheckTypeRepo, SqliteDoctorPricingRepo, SqliteDoctorRepo,
    SqliteInventoryConsumptionRepo, SqliteInventoryItemRepo, SqliteOperatorRepo,
    SqliteOperatorSpecialtyRepo,
};
use crate::domains::catalog::service::CatalogServicesConfig;
use crate::domains::catalog::CatalogServices;
use crate::domains::inventory::commands::{
    inventory_create_adjustment, inventory_get_item, inventory_list_adjustments,
    inventory_list_items, inventory_recompute_on_hand,
};
use crate::domains::inventory::service::InventoryAdjustmentServiceConfig;
use crate::domains::inventory::InventoryAdjustmentService;
use crate::domains::patients::commands::{
    patients_create, patients_get, patients_search, patients_update,
};
use crate::domains::patients::domain::repositories::PatientRepo;
use crate::domains::patients::infrastructure::SqlitePatientRepo;
use crate::domains::patients::PatientService;
use crate::domains::reports::commands::{
    reports_daily_close, reports_dashboard_kpis, reports_dashboard_tops, reports_doctor_drilldown,
    reports_doctor_earnings, reports_export_daily_close_pdf, reports_export_doctors_csv,
    reports_export_operators_csv, reports_export_visits_csv, reports_operator_drilldown,
    reports_operator_earnings, reports_visits,
};
use crate::domains::reports::domain::repositories::ReportsReadModel;
use crate::domains::reports::infrastructure::SqliteReportsReadModel;
use crate::domains::reports::service::ReportsServiceConfig;
use crate::domains::reports::ReportsService;
use crate::domains::settings::commands::{settings_get, settings_list, settings_update};
use crate::domains::settings::domain::repositories::SettingRepo;
use crate::domains::settings::infrastructure::SqliteSettingRepo;
use crate::domains::settings::service::SettingsService;
use crate::domains::shifts::commands::{
    shifts_clock_in, shifts_clock_out, shifts_edit, shifts_history_today, shifts_list_open,
    shifts_list_overlaps, shifts_soft_delete,
};
use crate::domains::shifts::domain::repositories::OperatorShiftRepo;
use crate::domains::shifts::infrastructure::SqliteOperatorShiftRepo;
use crate::domains::shifts::ShiftService;
use crate::domains::sync::commands::{
    config_get_sync_server_url, config_set_sync_server_url, device_info, sync_list_conflicts,
    sync_outbox_count, sync_resolve_conflict, sync_status, sync_trigger_pull, sync_trigger_push,
};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use crate::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo,
};
use crate::domains::visits::commands::{
    receipts_reprint, shifts_lines_run_today, visits_checks_grid, visits_create_draft,
    visits_discard, visits_get, visits_list_drafts_by_check, visits_list_today_by_check,
    visits_list_workspace, visits_lock, visits_pricing_resolve, visits_qualified_operators,
    visits_update_draft, visits_void,
};
use crate::domains::visits::domain::repositories::{InventoryAdjustmentRepo, VisitRepo};
use crate::domains::visits::infrastructure::{SqliteInventoryAdjustmentRepo, SqliteVisitRepo};
use crate::domains::visits::service::VisitServiceConfig;
use crate::domains::visits::VisitService;
use crate::state::{AppState, AppStateConfig};
use crate::sync::{SyncEngine, SyncEngineHandle};

/// Standalone-mode embedded flag (PRD §5.3 / phase-01 §7.35).
fn embedded_mode_enabled() -> bool {
    std::env::var("IDC_EMBEDDED_MODE").unwrap_or_else(|_| "0".into()) == "1"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if embedded::is_embedded_mode() {
        if !embedded_mode_enabled() {
            tracing::info!("embedded_mode=disabled (IDC_EMBEDDED_MODE != 1)");
        }
        tracing::info!("startup: embedded mode detected (TORCH_EMBEDDED_MODE=true)");
        match embedded::EmbeddedConfig::from_env() {
            Ok(embedded_config) => {
                let rt = tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime for embedded mode");
                if let Err(e) = rt.block_on(embedded::run_embedded(embedded_config)) {
                    tracing::error!(error = %e, "embedded mode failed");
                    std::process::exit(1);
                }
                return;
            }
            Err(e) => {
                tracing::error!(error = %e, "invalid embedded mode configuration");
                tracing::error!("required env vars: TORCH_IPC_PORT, TORCH_RUN_ID");
                std::process::exit(1);
            }
        }
    }

    tracing::info!("embedded_mode=disabled");
    tracing::info!("startup: running in standalone mode");

    let cancel = CancellationToken::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup({
            let cancel = cancel.clone();
            move |app| {
                if cfg!(debug_assertions) {
                    app.handle().plugin(
                        tauri_plugin_log::Builder::default()
                            .level(log::LevelFilter::Info)
                            .build(),
                    )?;
                }

                let handle = app.handle().clone();
                let cancel = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = bootstrap(&handle, cancel).await {
                        error!(error = %e, "bootstrap failed");
                    }
                });
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            // sync
            sync_status,
            sync_outbox_count,
            sync_trigger_push,
            sync_trigger_pull,
            sync_list_conflicts,
            sync_resolve_conflict,
            device_info,
            config_set_sync_server_url,
            config_get_sync_server_url,
            // auth
            auth_login,
            auth_logout,
            auth_current_user,
            auth_lock,
            auth_unlock,
            auth_is_locked,
            // users
            users_list,
            users_get,
            users_create,
            users_update,
            users_soft_delete,
            users_reset_password,
            users_create_first_admin,
            // settings
            settings_list,
            settings_get,
            settings_update,
            // catalog: check_types
            check_types_list,
            check_types_get,
            check_types_create,
            check_types_update,
            check_types_toggle_subtypes,
            check_types_soft_delete,
            // catalog: check_subtypes
            check_subtypes_list_by_type,
            check_subtypes_create,
            check_subtypes_update,
            check_subtypes_soft_delete,
            // catalog: doctors
            doctors_list,
            doctors_get,
            doctors_create,
            doctors_update,
            doctors_set_active,
            doctors_soft_delete,
            // catalog: doctor pricing
            doctor_pricing_upsert,
            doctor_pricing_soft_delete,
            pricing_effective,
            // catalog: operators
            operators_list,
            operators_get,
            operators_create,
            operators_update,
            operators_set_active,
            operators_soft_delete,
            // catalog: operator specialties
            operator_specialties_upsert,
            operator_specialties_soft_delete,
            // catalog: inventory items
            inventory_catalog_list,
            inventory_catalog_get,
            inventory_catalog_create,
            inventory_catalog_update,
            inventory_catalog_soft_delete,
            // catalog: consumption map
            inventory_consumption_create,
            inventory_consumption_update,
            inventory_consumption_soft_delete,
            inventory_consumption_list_by_type,
            // shifts
            shifts_clock_in,
            shifts_clock_out,
            shifts_list_open,
            shifts_history_today,
            shifts_edit,
            shifts_soft_delete,
            shifts_list_overlaps,
            shifts_lines_run_today,
            // patients
            patients_search,
            patients_create,
            patients_get,
            patients_update,
            // visits
            visits_checks_grid,
            visits_list_today_by_check,
            visits_list_drafts_by_check,
            visits_list_workspace,
            visits_get,
            visits_create_draft,
            visits_update_draft,
            visits_discard,
            visits_qualified_operators,
            visits_lock,
            visits_void,
            visits_pricing_resolve,
            receipts_reprint,
            // inventory operations
            inventory_list_items,
            inventory_get_item,
            inventory_list_adjustments,
            inventory_create_adjustment,
            inventory_recompute_on_hand,
            // reports
            reports_dashboard_kpis,
            reports_dashboard_tops,
            reports_visits,
            reports_doctor_earnings,
            reports_doctor_drilldown,
            reports_operator_earnings,
            reports_operator_drilldown,
            reports_daily_close,
            reports_export_visits_csv,
            reports_export_doctors_csv,
            reports_export_operators_csv,
            reports_export_daily_close_pdf,
            // audit + diagnostics (phase 8)
            audit_query,
            audit_vacuum_now,
            diagnostics_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn bootstrap(
    app: &tauri::AppHandle,
    cancel: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db_path = resolve_db_path(app)?;
    info!(path = ?db_path, "opening local database");
    let pool = crate::db::init_pool(&db_path).await?;
    migrations::run(&pool).await?;

    let device_id = resolve_device_id(&pool).await?;
    let app_version = app.package_info().version.to_string();
    let entity_id_tenant = "unscoped".to_string();
    let initial_server_url = std::env::var("IDC_SYNC_SERVER_URL").ok();

    let outbox_repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let audit_repo: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state_repo: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let metrics_repo: Arc<dyn MetricsRepo> = Arc::new(SqliteMetricsRepo::new(pool.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));
    let setting_repo: Arc<dyn SettingRepo> = Arc::new(SqliteSettingRepo::new(pool.clone()));

    let check_type_repo: Arc<dyn CheckTypeRepo> = Arc::new(SqliteCheckTypeRepo::new(pool.clone()));
    let check_subtype_repo: Arc<dyn CheckSubtypeRepo> =
        Arc::new(SqliteCheckSubtypeRepo::new(pool.clone()));
    let doctor_repo: Arc<dyn DoctorRepo> = Arc::new(SqliteDoctorRepo::new(pool.clone()));
    let doctor_pricing_repo: Arc<dyn DoctorPricingRepo> =
        Arc::new(SqliteDoctorPricingRepo::new(pool.clone()));
    let operator_repo: Arc<dyn OperatorRepo> = Arc::new(SqliteOperatorRepo::new(pool.clone()));
    let operator_specialty_repo: Arc<dyn OperatorSpecialtyRepo> =
        Arc::new(SqliteOperatorSpecialtyRepo::new(pool.clone()));
    let inventory_item_repo: Arc<dyn InventoryItemRepo> =
        Arc::new(SqliteInventoryItemRepo::new(pool.clone()));
    let consumption_repo: Arc<dyn InventoryConsumptionRepo> =
        Arc::new(SqliteInventoryConsumptionRepo::new(pool.clone()));
    let shift_repo: Arc<dyn OperatorShiftRepo> =
        Arc::new(SqliteOperatorShiftRepo::new(pool.clone()));
    let patient_repo: Arc<dyn PatientRepo> = Arc::new(SqlitePatientRepo::new(pool.clone()));
    let visit_repo: Arc<dyn VisitRepo> = Arc::new(SqliteVisitRepo::new(pool.clone()));
    let adjustment_repo: Arc<dyn InventoryAdjustmentRepo> =
        Arc::new(SqliteInventoryAdjustmentRepo::new(pool.clone()));

    let engine_handle: SyncEngineHandle = SyncEngine::spawn(
        crate::sync::engine::SyncEngineConfig {
            pool: pool.clone(),
            outbox_repo: outbox_repo.clone(),
            audit_repo: audit_repo.clone(),
            state_repo: state_repo.clone(),
            device_id: device_id.clone(),
            app_version: app_version.clone(),
            initial_server_url: initial_server_url.clone(),
            initial_token: None,
            entity_id_tenant: entity_id_tenant.clone(),
        },
        app.clone(),
        cancel.clone(),
    );

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        device_id.clone(),
    ));
    let user_service = Arc::new(UserService::new(
        pool.clone(),
        user_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        device_id.clone(),
    ));
    let settings_service = Arc::new(SettingsService::new(
        pool.clone(),
        setting_repo,
        audit_repo.clone(),
        outbox_repo.clone(),
        device_id.clone(),
    ));

    let catalog_services = CatalogServices::new(CatalogServicesConfig {
        pool: pool.clone(),
        check_type_repo,
        check_subtype_repo,
        doctor_repo,
        doctor_pricing_repo,
        operator_repo: operator_repo.clone(),
        operator_specialty_repo,
        inventory_item_repo,
        consumption_repo,
        audit_repo: audit_repo.clone(),
        outbox_repo: outbox_repo.clone(),
        device_id: device_id.clone(),
        app_handle: app.clone(),
    });

    let shift_service = Arc::new(ShiftService::new(
        pool.clone(),
        shift_repo.clone(),
        operator_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        device_id.clone(),
    ));

    let patient_service = Arc::new(PatientService::new(
        pool.clone(),
        patient_repo.clone(),
        audit_repo.clone(),
        outbox_repo.clone(),
        device_id.clone(),
    ));

    let receipts_dir = resolve_receipts_dir(app)?;

    let inventory_adjustment_service = Arc::new(InventoryAdjustmentService::new(
        InventoryAdjustmentServiceConfig {
            pool: pool.clone(),
            items_repo: catalog_services.inventory_item_repo.clone(),
            consumption_repo: catalog_services.consumption_repo.clone(),
            adjustments_repo: adjustment_repo.clone(),
            audit_repo: audit_repo.clone(),
            outbox_repo: outbox_repo.clone(),
            device_id: device_id.clone(),
        },
    ));

    let audit_query_service = Arc::new(AuditQuerySvc::new(audit_repo.clone()));
    let audit_vacuum_job = Arc::new(AuditVacuumSvc::new(
        pool.clone(),
        audit_repo.clone(),
        metrics_repo.clone(),
        outbox_repo.clone(),
        state_repo.clone(),
        device_id.clone(),
    ));
    let diagnostics_service = Arc::new(DiagnosticsSvc::new(
        metrics_repo.clone(),
        outbox_repo.clone(),
        state_repo.clone(),
    ));

    // Spawn the daily audit-vacuum scheduler. Phase-08 §4 + §7.2.
    {
        let job = audit_vacuum_job.clone();
        let cancel = cancel.clone();
        let tenant = entity_id_tenant.clone();
        tokio::spawn(async move {
            job.run_scheduler(tenant, cancel).await;
        });
    }

    let reports_read_model: Arc<dyn ReportsReadModel> =
        Arc::new(SqliteReportsReadModel::new(pool.clone()));
    let reports_service = Arc::new(ReportsService::new(ReportsServiceConfig {
        pool: pool.clone(),
        read_model: reports_read_model,
        audit_repo: audit_repo.clone(),
        outbox_repo: outbox_repo.clone(),
        device_id: device_id.clone(),
    }));

    let visit_service = Arc::new(VisitService::new(VisitServiceConfig {
        pool: pool.clone(),
        visits: visit_repo,
        adjustments: adjustment_repo,
        patients: patient_repo,
        check_types: catalog_services.check_type_repo.clone(),
        check_subtypes: catalog_services.check_subtype_repo.clone(),
        doctors: catalog_services.doctor_repo.clone(),
        doctor_pricing: catalog_services.doctor_pricing_repo.clone(),
        operators: operator_repo.clone(),
        operator_specialties: catalog_services.operator_specialty_repo.clone(),
        consumption: catalog_services.consumption_repo.clone(),
        inventory_items: catalog_services.inventory_item_repo.clone(),
        shifts: shift_repo,
        audit_repo: audit_repo.clone(),
        outbox_repo: outbox_repo.clone(),
        receipts_dir,
        device_id: device_id.clone(),
    }));

    let state = AppState::new(AppStateConfig {
        db_pool: pool,
        sync_engine: engine_handle,
        auth_service,
        user_service,
        settings_service,
        catalog_services,
        shift_service,
        patient_service,
        visit_service,
        inventory_adjustment_service,
        reports_service,
        audit_query_service,
        audit_vacuum_job,
        diagnostics_service,
        user_repo,
        device_id,
        app_version,
        sync_server_url: initial_server_url,
    });
    app.manage(state);

    info!("bootstrap complete");
    Ok(())
}

fn resolve_db_path(
    app: &tauri::AppHandle,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("idc-local.db"))
}

fn resolve_receipts_dir(
    app: &tauri::AppHandle,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir = app.path().app_data_dir()?.join("receipts");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

async fn resolve_device_id(
    pool: &sqlx::SqlitePool,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let repo = SqliteSyncStateRepo::new(pool.clone());
    let candidate = Uuid::now_v7().to_string();
    let device_id = repo.ensure_device_id(&candidate).await?;
    Ok(device_id)
}
