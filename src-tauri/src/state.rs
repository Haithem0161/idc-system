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
    user_context: RwLock<Option<UserContext>>,
    settings_cache: RwLock<HashMap<String, SettingValue>>,
    device_id: String,
    app_version: String,
    token: RwLock<Option<String>>,
    expires_at: RwLock<Option<i64>>,
    sync_server_url: RwLock<Option<String>>,
}

impl AppState {
    pub fn new(
        db_pool: SqlitePool,
        sync_engine: SyncEngineHandle,
        device_id: String,
        app_version: String,
        sync_server_url: Option<String>,
    ) -> Self {
        Self {
            db_pool: Some(db_pool),
            sync_engine: Some(sync_engine),
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            device_id,
            app_version,
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            sync_server_url: RwLock::new(sync_server_url),
        }
    }

    /// Minimal state for embedded (Business OS) mode: auth + user context
    /// only. Sync engine and DB pool are absent; any command that depends on
    /// them returns `AppError::Configuration`.
    pub fn for_embedded() -> Self {
        Self {
            db_pool: None,
            sync_engine: None,
            user_context: RwLock::new(None),
            settings_cache: RwLock::new(HashMap::new()),
            device_id: String::new(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
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

    pub async fn set_current_user(&self, user: UserContext) {
        *self.user_context.write().await = Some(user);
    }

    pub async fn get_current_user(&self) -> Option<UserContext> {
        self.user_context.read().await.clone()
    }

    pub async fn clear_auth(&self) {
        *self.token.write().await = None;
        *self.expires_at.write().await = None;
        *self.user_context.write().await = None;
        if let Some(engine) = &self.sync_engine {
            engine.set_token(None).await;
        }
    }

    pub async fn is_authenticated(&self) -> bool {
        self.user_context.read().await.is_some()
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

    /// Tenant identifier used by audit + sync. Resolved from the user context
    /// when available; falls back to a "unscoped" sentinel pre-login (the
    /// engine writes telemetry under this id during Phase-1 smoke).
    pub async fn entity_id_tenant(&self) -> String {
        self.user_context
            .read()
            .await
            .as_ref()
            .map(|u| u.entity_id.clone())
            .unwrap_or_else(|| "unscoped".into())
    }
}
