//! Sync engine orchestrator.
//!
//! Boots once at app start, holds the typed HTTP client, drives the push and
//! pull loops on a single Tokio task, exposes a small command surface via
//! `mpsc`, and emits Tauri events for UI status.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use crate::domains::sync::domain::value_objects::SyncStatus;
use crate::domains::sync::infrastructure::{ServerConflict, SyncHttpClient};
use crate::error::AppResult;

pub const STATUS_EVENT: &str = "sync:status";
pub const CONFLICT_EVENT: &str = "sync:conflict";
pub const PROGRESS_EVENT: &str = "sync:progress";
pub const AUTH_EXPIRED_EVENT: &str = "auth:session_expired";

const PUSH_INTERVAL: Duration = Duration::from_secs(15);
const PULL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug)]
enum Cmd {
    TriggerPush,
    TriggerPull,
    SetToken(Option<String>),
    SetServerUrl(String),
    ResolveConflict {
        op_id: String,
        choice: String,
        merged: Option<serde_json::Value>,
        reply: tokio::sync::oneshot::Sender<AppResult<()>>,
    },
    ListConflicts {
        reply: tokio::sync::oneshot::Sender<AppResult<Vec<ServerConflict>>>,
    },
}

#[derive(Clone)]
pub struct SyncEngineHandle {
    tx: mpsc::Sender<Cmd>,
    status: Arc<RwLock<SyncStatus>>,
    outbox_repo: Arc<dyn OutboxRepo>,
}

impl SyncEngineHandle {
    pub async fn trigger_push(&self) {
        let _ = self.tx.send(Cmd::TriggerPush).await;
    }

    pub async fn trigger_pull(&self) {
        let _ = self.tx.send(Cmd::TriggerPull).await;
    }

    pub async fn set_token(&self, token: Option<String>) {
        let _ = self.tx.send(Cmd::SetToken(token)).await;
    }

    pub async fn set_server_url(&self, url: String) {
        let _ = self.tx.send(Cmd::SetServerUrl(url)).await;
    }

    pub async fn status(&self) -> SyncStatus {
        *self.status.read().await
    }

    pub fn outbox_repo(&self) -> Arc<dyn OutboxRepo> {
        self.outbox_repo.clone()
    }

    pub async fn resolve_conflict(
        &self,
        op_id: String,
        choice: String,
        merged: Option<serde_json::Value>,
    ) -> AppResult<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Cmd::ResolveConflict {
                op_id,
                choice,
                merged,
                reply: reply_tx,
            })
            .await
            .map_err(|_| crate::error::AppError::Internal("engine offline".into()))?;
        reply_rx
            .await
            .map_err(|_| crate::error::AppError::Internal("engine dropped reply".into()))?
    }

    pub async fn list_conflicts(&self) -> AppResult<Vec<ServerConflict>> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Cmd::ListConflicts { reply: reply_tx })
            .await
            .map_err(|_| crate::error::AppError::Internal("engine offline".into()))?;
        reply_rx
            .await
            .map_err(|_| crate::error::AppError::Internal("engine dropped reply".into()))?
    }
}

pub struct SyncEngineConfig {
    pub pool: SqlitePool,
    pub outbox_repo: Arc<dyn OutboxRepo>,
    pub audit_repo: Arc<dyn AuditRepo>,
    pub state_repo: Arc<dyn SyncStateRepo>,
    pub device_id: String,
    pub app_version: String,
    pub initial_server_url: Option<String>,
    pub initial_token: Option<String>,
    pub entity_id_tenant: String,
}

pub struct SyncEngine {
    pool: SqlitePool,
    outbox_repo: Arc<dyn OutboxRepo>,
    #[allow(dead_code)]
    audit_repo: Arc<dyn AuditRepo>,
    state_repo: Arc<dyn SyncStateRepo>,
    http: Arc<Mutex<Option<SyncHttpClient>>>,
    device_id: String,
    app_version: String,
    entity_id_tenant: String,
    token: Arc<RwLock<Option<String>>>,
    status: Arc<RwLock<SyncStatus>>,
}

impl SyncEngine {
    /// Start the engine on a Tokio task. Returns a handle for command issuing
    /// and a `CancellationToken` for graceful shutdown.
    pub fn spawn(
        config: SyncEngineConfig,
        app: AppHandle,
        cancel: CancellationToken,
    ) -> SyncEngineHandle {
        let (tx, rx) = mpsc::channel::<Cmd>(64);
        let status = Arc::new(RwLock::new(SyncStatus::Idle));

        let http = match config.initial_server_url.as_deref() {
            Some(url) if !url.is_empty() => SyncHttpClient::new(
                url.to_string(),
                config.device_id.clone(),
                config.app_version.clone(),
            )
            .ok(),
            _ => None,
        };

        let engine = Self {
            pool: config.pool,
            outbox_repo: config.outbox_repo.clone(),
            audit_repo: config.audit_repo,
            state_repo: config.state_repo,
            http: Arc::new(Mutex::new(http)),
            device_id: config.device_id,
            app_version: config.app_version,
            entity_id_tenant: config.entity_id_tenant,
            token: Arc::new(RwLock::new(config.initial_token)),
            status: status.clone(),
        };

        let handle = SyncEngineHandle {
            tx,
            status: status.clone(),
            outbox_repo: config.outbox_repo,
        };

        tokio::spawn(async move {
            engine.run(rx, app, cancel).await;
        });

        handle
    }

    async fn run(self, mut rx: mpsc::Receiver<Cmd>, app: AppHandle, cancel: CancellationToken) {
        let mut push_tick = interval(PUSH_INTERVAL);
        push_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut pull_tick = interval(PULL_INTERVAL);
        pull_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!(device = %self.device_id, "sync engine started");
        let _ = app.emit(STATUS_EVENT, SyncStatus::Idle);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("sync engine: cancelled");
                    break;
                }
                Some(cmd) = rx.recv() => {
                    self.handle_cmd(&app, cmd).await;
                }
                _ = push_tick.tick() => {
                    self.do_push(&app).await;
                }
                _ = pull_tick.tick() => {
                    self.do_pull(&app).await;
                }
            }
        }

        info!("sync engine: stopped");
    }

    async fn handle_cmd(&self, app: &AppHandle, cmd: Cmd) {
        match cmd {
            Cmd::TriggerPush => self.do_push(app).await,
            Cmd::TriggerPull => self.do_pull(app).await,
            Cmd::SetToken(t) => {
                *self.token.write().await = t;
            }
            Cmd::SetServerUrl(url) => {
                match SyncHttpClient::new(url, self.device_id.clone(), self.app_version.clone()) {
                    Ok(client) => {
                        *self.http.lock().await = Some(client);
                    }
                    Err(e) => warn!(error = %e, "failed to build sync http client"),
                }
            }
            Cmd::ResolveConflict {
                op_id,
                choice,
                merged,
                reply,
            } => {
                let result = self.resolve_conflict_inner(op_id, choice, merged).await;
                let _ = reply.send(result);
            }
            Cmd::ListConflicts { reply } => {
                let result = self.list_conflicts_inner().await;
                let _ = reply.send(result);
            }
        }
    }

    async fn current_http(&self) -> Option<SyncHttpClient> {
        self.http.lock().await.clone()
    }

    async fn set_status(&self, app: &AppHandle, status: SyncStatus) {
        *self.status.write().await = status;
        if let Err(e) = app.emit(STATUS_EVENT, status) {
            debug!(error = %e, "emit status failed");
        }
    }

    async fn do_push(&self, app: &AppHandle) {
        let Some(http) = self.current_http().await else {
            self.set_status(app, SyncStatus::Offline).await;
            return;
        };
        let token = self.token.read().await.clone();

        self.set_status(app, SyncStatus::Pushing).await;
        match crate::sync::pusher::run_step(
            &self.pool,
            self.outbox_repo.clone(),
            self.state_repo.clone(),
            &http,
            token.as_deref(),
            &self.entity_id_tenant,
        )
        .await
        {
            Ok(outcome) => {
                if outcome.session_expired {
                    let _ = app.emit(AUTH_EXPIRED_EVENT, ());
                    self.set_status(app, SyncStatus::Error).await;
                    return;
                }
                for conflict in &outcome.conflicts {
                    let _ = app.emit(CONFLICT_EVENT, conflict);
                }
                if outcome.pushed > 0 {
                    let _ = app.emit(
                        PROGRESS_EVENT,
                        serde_json::json!({ "pushed": outcome.pushed }),
                    );
                }
                self.set_status(app, SyncStatus::Idle).await;
            }
            Err(e) => {
                error!(error = %e, "push step failed");
                self.set_status(app, SyncStatus::Error).await;
            }
        }
    }

    async fn do_pull(&self, app: &AppHandle) {
        let Some(http) = self.current_http().await else {
            self.set_status(app, SyncStatus::Offline).await;
            return;
        };
        let token = self.token.read().await.clone();

        self.set_status(app, SyncStatus::Pulling).await;
        match crate::sync::puller::run_step(
            &self.pool,
            self.state_repo.clone(),
            &http,
            token.as_deref(),
            &self.entity_id_tenant,
        )
        .await
        {
            Ok(outcome) => {
                if outcome.session_expired {
                    let _ = app.emit(AUTH_EXPIRED_EVENT, ());
                    self.set_status(app, SyncStatus::Error).await;
                    return;
                }
                self.set_status(app, SyncStatus::Idle).await;
            }
            Err(e) => {
                error!(error = %e, "pull step failed");
                self.set_status(app, SyncStatus::Error).await;
            }
        }
    }

    async fn resolve_conflict_inner(
        &self,
        op_id: String,
        choice: String,
        merged: Option<serde_json::Value>,
    ) -> AppResult<()> {
        let Some(http) = self.current_http().await else {
            return Err(crate::error::AppError::SyncUnavailable(
                "no server configured".into(),
            ));
        };
        let token = self.token.read().await.clone();
        let token = token.unwrap_or_default();
        // Phase-08 §7.22 idempotency: derive a stable resolve_op_id so a
        // mid-flight network failure doesn't double-apply on retry.
        let resolve_op_id = stable_resolve_op_id(&op_id, &choice, merged.as_ref());
        http.resolve_conflict(&token, &op_id, &resolve_op_id, &choice, merged)
            .await?;
        // Release the parked outbox row so it can be re-pushed under the
        // chosen resolution.
        if let Ok(parsed) = uuid::Uuid::parse_str(&op_id) {
            let _ = self.outbox_repo.delete_acked(&[parsed]).await;
        }
        Ok(())
    }

    async fn list_conflicts_inner(&self) -> AppResult<Vec<ServerConflict>> {
        let Some(http) = self.current_http().await else {
            return Ok(Vec::new());
        };
        let token = self.token.read().await.clone().unwrap_or_default();
        http.list_conflicts(&token).await
    }
}

/// `sha256(op_id|choice|merged_canonical_json)` (phase-08 §7.22).
fn stable_resolve_op_id(op_id: &str, choice: &str, merged: Option<&serde_json::Value>) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(op_id.as_bytes());
    hasher.update([b'|']);
    hasher.update(choice.as_bytes());
    hasher.update([b'|']);
    if let Some(v) = merged {
        // serde_json `to_string` is canonical for the same input shape -- not
        // RFC8785 canonical, but stable for the same `merged` dict the UI sent.
        let canon = serde_json::to_string(v).unwrap_or_default();
        hasher.update(canon.as_bytes());
    }
    let digest = hasher.finalize();
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_op_id_is_stable() {
        let a = stable_resolve_op_id("op-1", "local", None);
        let b = stable_resolve_op_id("op-1", "local", None);
        assert_eq!(a, b);
        let c = stable_resolve_op_id("op-1", "server", None);
        assert_ne!(a, c);
        let merged = serde_json::json!({"k":"v"});
        let d = stable_resolve_op_id("op-1", "merged", Some(&merged));
        let e = stable_resolve_op_id("op-1", "merged", Some(&merged));
        assert_eq!(d, e);
    }
}
