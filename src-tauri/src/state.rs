//! Application state.
//!
//! Single source of truth shared across Tauri commands. Constructed once in
//! `lib.rs::run()` and registered via `Builder::manage(...)`. All mutable
//! fields use `tokio::sync::RwLock`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::domains::audit::{AuditQueryService, AuditVacuumJob, DiagnosticsService};
use crate::domains::auth::domain::repositories::UserRepo;
use crate::domains::auth::{AuthService, UserService};
use crate::domains::catalog::CatalogServices;
use crate::domains::inventory::InventoryAdjustmentService;
use crate::domains::patients::PatientService;
use crate::domains::reports::ReportsService;
use crate::domains::settings::service::SettingsService;
use crate::domains::shifts::ShiftService;
use crate::domains::visits::VisitService;
use crate::sync::SyncEngineHandle;

/// User context received from Business OS (embedded mode) or the auth flow
/// (Phase 2 standalone mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub entity_id: String,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
}

/// Cached settings value (Phase 2 owns the typed shape).
pub type SettingValue = serde_json::Value;

pub struct AppState {
    db_pool: Option<SqlitePool>,
    sync_engine: Option<SyncEngineHandle>,
    auth_service: Option<Arc<AuthService>>,
    user_service: Option<Arc<UserService>>,
    settings_service: Option<Arc<SettingsService>>,
    catalog_services: Option<CatalogServices>,
    shift_service: Option<Arc<ShiftService>>,
    patient_service: Option<Arc<PatientService>>,
    visit_service: Option<Arc<VisitService>>,
    inventory_adjustment_service: Option<Arc<InventoryAdjustmentService>>,
    reports_service: Option<Arc<ReportsService>>,
    audit_query_service: Option<Arc<AuditQueryService>>,
    audit_vacuum_job: Option<Arc<AuditVacuumJob>>,
    diagnostics_service: Option<Arc<DiagnosticsService>>,
    user_repo: Option<Arc<dyn UserRepo>>,
    user_context: RwLock<Option<UserContext>>,
    settings_cache: RwLock<HashMap<String, SettingValue>>,
    locked: RwLock<bool>,
    device_id: String,
    app_version: String,
    token: RwLock<Option<String>>,
    expires_at: RwLock<Option<i64>>,
    refresh_token: RwLock<Option<String>>,
    sync_server_url: RwLock<Option<String>>,
}

pub struct AppStateConfig {
    pub db_pool: SqlitePool,
    pub sync_engine: SyncEngineHandle,
    pub auth_service: Arc<AuthService>,
    pub user_service: Arc<UserService>,
    pub settings_service: Arc<SettingsService>,
    pub catalog_services: CatalogServices,
    pub shift_service: Arc<ShiftService>,
    pub patient_service: Arc<PatientService>,
    pub visit_service: Arc<VisitService>,
    pub inventory_adjustment_service: Arc<InventoryAdjustmentService>,
    pub reports_service: Arc<ReportsService>,
    pub audit_query_service: Arc<AuditQueryService>,
    pub audit_vacuum_job: Arc<AuditVacuumJob>,
    pub diagnostics_service: Arc<DiagnosticsService>,
    pub user_repo: Arc<dyn UserRepo>,
    pub device_id: String,
    pub app_version: String,
    pub sync_server_url: Option<String>,
}

impl AppState {
    pub fn new(cfg: AppStateConfig) -> Self {
        Self {
            db_pool: Some(cfg.db_pool),
            sync_engine: Some(cfg.sync_engine),
            auth_service: Some(cfg.auth_service),
            user_service: Some(cfg.user_service),
            settings_service: Some(cfg.settings_service),
            catalog_services: Some(cfg.catalog_services),
            shift_service: Some(cfg.shift_service),
            patient_service: Some(cfg.patient_service),
            visit_service: Some(cfg.visit_service),
            inventory_adjustment_service: Some(cfg.inventory_adjustment_service),
            reports_service: Some(cfg.reports_service),
            audit_query_service: Some(cfg.audit_query_service),
            audit_vacuum_job: Some(cfg.audit_vacuum_job),
            diagnostics_service: Some(cfg.diagnostics_service),
            user_repo: Some(cfg.user_repo),
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            locked: RwLock::new(false),
            device_id: cfg.device_id,
            app_version: cfg.app_version,
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            refresh_token: RwLock::new(None),
            sync_server_url: RwLock::new(cfg.sync_server_url),
        }
    }

    /// Phase-01 §2.2 test-only constructor: builds a minimal AppState that
    /// satisfies the sync IPC commands (db_pool + sync_engine + device_id +
    /// app_version + sync_server_url) while leaving every later-phase
    /// service as `None`. Production code MUST go through `new(cfg)`.
    pub fn for_sync_tests(
        db_pool: SqlitePool,
        sync_engine: SyncEngineHandle,
        device_id: String,
        app_version: String,
        sync_server_url: Option<String>,
    ) -> Self {
        Self {
            db_pool: Some(db_pool),
            sync_engine: Some(sync_engine),
            auth_service: None,
            user_service: None,
            settings_service: None,
            catalog_services: None,
            shift_service: None,
            patient_service: None,
            visit_service: None,
            inventory_adjustment_service: None,
            reports_service: None,
            audit_query_service: None,
            audit_vacuum_job: None,
            diagnostics_service: None,
            user_repo: None,
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            locked: RwLock::new(false),
            device_id,
            app_version,
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            refresh_token: RwLock::new(None),
            sync_server_url: RwLock::new(sync_server_url),
        }
    }

    /// Phase-02 §2.2 test-only constructor: wires the auth + users + settings
    /// services and the user_repo so the auth / users / settings IPC commands
    /// can be exercised without standing up the full app graph. Phase-01-only
    /// services remain `None`. Production code MUST go through `new(cfg)`.
    #[allow(clippy::too_many_arguments)]
    pub fn for_phase02_tests(
        db_pool: SqlitePool,
        sync_engine: SyncEngineHandle,
        auth_service: Arc<AuthService>,
        user_service: Arc<UserService>,
        settings_service: Arc<SettingsService>,
        user_repo: Arc<dyn UserRepo>,
        device_id: String,
        app_version: String,
        sync_server_url: Option<String>,
    ) -> Self {
        Self {
            db_pool: Some(db_pool),
            sync_engine: Some(sync_engine),
            auth_service: Some(auth_service),
            user_service: Some(user_service),
            settings_service: Some(settings_service),
            catalog_services: None,
            shift_service: None,
            patient_service: None,
            visit_service: None,
            inventory_adjustment_service: None,
            reports_service: None,
            audit_query_service: None,
            audit_vacuum_job: None,
            diagnostics_service: None,
            user_repo: Some(user_repo),
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            locked: RwLock::new(false),
            device_id,
            app_version,
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            refresh_token: RwLock::new(None),
            sync_server_url: RwLock::new(sync_server_url),
        }
    }

    /// Phase-03 §2.2 test-only constructor: wires the catalog services on top
    /// of the phase-02 surface so catalog IPC commands can be exercised
    /// without standing up the full app graph.
    #[allow(clippy::too_many_arguments)]
    pub fn for_phase03_tests(
        db_pool: SqlitePool,
        sync_engine: SyncEngineHandle,
        auth_service: Arc<AuthService>,
        user_service: Arc<UserService>,
        settings_service: Arc<SettingsService>,
        catalog_services: CatalogServices,
        user_repo: Arc<dyn UserRepo>,
        device_id: String,
        app_version: String,
        sync_server_url: Option<String>,
    ) -> Self {
        Self {
            db_pool: Some(db_pool),
            sync_engine: Some(sync_engine),
            auth_service: Some(auth_service),
            user_service: Some(user_service),
            settings_service: Some(settings_service),
            catalog_services: Some(catalog_services),
            shift_service: None,
            patient_service: None,
            visit_service: None,
            inventory_adjustment_service: None,
            reports_service: None,
            audit_query_service: None,
            audit_vacuum_job: None,
            diagnostics_service: None,
            user_repo: Some(user_repo),
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            locked: RwLock::new(false),
            device_id,
            app_version,
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            refresh_token: RwLock::new(None),
            sync_server_url: RwLock::new(sync_server_url),
        }
    }

    pub fn for_embedded() -> Self {
        Self {
            db_pool: None,
            sync_engine: None,
            auth_service: None,
            user_service: None,
            settings_service: None,
            catalog_services: None,
            shift_service: None,
            patient_service: None,
            visit_service: None,
            inventory_adjustment_service: None,
            reports_service: None,
            audit_query_service: None,
            audit_vacuum_job: None,
            diagnostics_service: None,
            user_repo: None,
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            locked: RwLock::new(false),
            device_id: String::new(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            refresh_token: RwLock::new(None),
            sync_server_url: RwLock::new(None),
        }
    }

    pub fn db_pool(&self) -> Option<&SqlitePool> {
        self.db_pool.as_ref()
    }

    pub fn sync_engine(&self) -> &SyncEngineHandle {
        self.sync_engine
            .as_ref()
            .expect("sync engine not available (embedded mode)")
    }

    pub fn try_sync_engine(&self) -> Option<&SyncEngineHandle> {
        self.sync_engine.as_ref()
    }

    pub fn auth_service(&self) -> Option<Arc<AuthService>> {
        self.auth_service.clone()
    }

    pub fn user_service(&self) -> Option<Arc<UserService>> {
        self.user_service.clone()
    }

    pub fn settings_service(&self) -> Option<Arc<SettingsService>> {
        self.settings_service.clone()
    }

    pub fn catalog_services(&self) -> Option<CatalogServices> {
        self.catalog_services.clone()
    }

    pub fn shift_service(&self) -> Option<Arc<ShiftService>> {
        self.shift_service.clone()
    }

    pub fn patient_service(&self) -> Option<Arc<PatientService>> {
        self.patient_service.clone()
    }

    pub fn visit_service(&self) -> Option<Arc<VisitService>> {
        self.visit_service.clone()
    }

    pub fn inventory_adjustment_service(&self) -> Option<Arc<InventoryAdjustmentService>> {
        self.inventory_adjustment_service.clone()
    }

    pub fn reports_service(&self) -> Option<Arc<ReportsService>> {
        self.reports_service.clone()
    }

    pub fn audit_query_service(&self) -> Option<Arc<AuditQueryService>> {
        self.audit_query_service.clone()
    }

    pub fn audit_vacuum_job(&self) -> Option<Arc<AuditVacuumJob>> {
        self.audit_vacuum_job.clone()
    }

    pub fn diagnostics_service(&self) -> Option<Arc<DiagnosticsService>> {
        self.diagnostics_service.clone()
    }

    pub fn user_repo(&self) -> Option<Arc<dyn UserRepo>> {
        self.user_repo.clone()
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    pub async fn set_current_token(&self, token: String, expires_at: i64) {
        *self.token.write().await = Some(token.clone());
        *self.expires_at.write().await = Some(expires_at);
        if let Some(engine) = &self.sync_engine {
            engine.set_token(Some(token)).await;
        }
    }

    pub async fn get_current_token(&self) -> Option<String> {
        self.token.read().await.clone()
    }

    pub async fn get_token_expires_at(&self) -> Option<i64> {
        *self.expires_at.read().await
    }

    /// DEF-007 G01: cache the refresh token returned by `/auth/login` so
    /// `auth_refresh` can rotate it later. Stored in memory only -- a
    /// future hardening phase will persist it via the Tauri stronghold
    /// plugin (see G08/G20/G21 for the public-key pin lifecycle that
    /// shares the same secure-storage surface).
    pub async fn set_refresh_token(&self, token: Option<String>) {
        *self.refresh_token.write().await = token;
    }

    pub async fn get_refresh_token(&self) -> Option<String> {
        self.refresh_token.read().await.clone()
    }

    pub async fn set_current_user(&self, user: UserContext) {
        *self.user_context.write().await = Some(user);
    }

    pub async fn get_current_user(&self) -> Option<UserContext> {
        self.user_context.read().await.clone()
    }

    pub async fn clear_auth(&self) {
        *self.token.write().await = None;
        *self.expires_at.write().await = None;
        *self.refresh_token.write().await = None;
        *self.user_context.write().await = None;
        *self.locked.write().await = false;
        if let Some(engine) = &self.sync_engine {
            engine.set_token(None).await;
        }
    }

    pub async fn is_authenticated(&self) -> bool {
        self.user_context.read().await.is_some()
    }

    pub async fn set_locked(&self, locked: bool) {
        *self.locked.write().await = locked;
    }

    pub async fn is_locked(&self) -> bool {
        *self.locked.read().await
    }

    pub async fn set_setting(&self, key: String, value: SettingValue) {
        self.settings_cache.write().await.insert(key, value);
    }

    pub async fn get_setting(&self, key: &str) -> Option<SettingValue> {
        self.settings_cache.read().await.get(key).cloned()
    }

    pub async fn settings_snapshot(&self) -> Arc<HashMap<String, SettingValue>> {
        Arc::new(self.settings_cache.read().await.clone())
    }

    pub async fn set_sync_server_url(&self, url: String) {
        *self.sync_server_url.write().await = Some(url);
    }

    pub async fn sync_server_url(&self) -> Option<String> {
        self.sync_server_url.read().await.clone()
    }

    pub async fn entity_id_tenant(&self) -> String {
        self.user_context
            .read()
            .await
            .as_ref()
            .map(|u| u.entity_id.clone())
            .unwrap_or_else(|| "unscoped".into())
    }
}
