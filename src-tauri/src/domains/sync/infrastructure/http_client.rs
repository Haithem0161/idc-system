//! Typed HTTP client for the sync server.
//!
//! All sync HTTP goes through this client; the frontend never talks to the
//! server directly (capability `http:default` is intentionally not granted).

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct PushOp {
    pub op_id: String,
    pub entity: String,
    pub entity_id: String,
    pub op: String,
    /// MessagePack-encoded payload, transported as base64 (lets us reuse the
    /// generic JSON transport without binary multipart).
    pub payload_b64: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushResponseOp {
    pub op_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConflict {
    pub op_id: String,
    pub entity: String,
    pub entity_id: String,
    pub server_payload: serde_json::Value,
    pub local_payload: serde_json::Value,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushRejectedOp {
    pub op_id: String,
    pub code: String,
    pub message: String,
    #[allow(dead_code)]
    pub status_code: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushResult {
    pub accepted: Vec<PushResponseOp>,
    pub conflicts: Vec<ServerConflict>,
    /// Per-op rejections (validation / authorization). Server isolates these
    /// instead of aborting the batch; the client parks them so one poison op
    /// never strands the rest of the queue. `#[serde(default)]` keeps the
    /// client compatible with older servers that omit the field.
    #[serde(default)]
    pub rejected: Vec<PushRejectedOp>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullChange {
    pub entity: String,
    pub entity_id: String,
    pub payload: serde_json::Value,
    pub updated_at: String,
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullResponse {
    pub changes: Vec<PullChange>,
    pub next_cursor: String,
}

#[derive(Clone)]
pub struct SyncHttpClient {
    base_url: String,
    client: reqwest::Client,
    device_id: String,
    app_version: String,
}

impl SyncHttpClient {
    pub fn new(base_url: String, device_id: String, app_version: String) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .user_agent(format!("idc-system/{app_version}"))
            .build()
            .map_err(AppError::from)?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            device_id,
            app_version,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn push(&self, token: &str, ops: &[PushOp]) -> AppResult<PushResult> {
        #[derive(Serialize)]
        struct Body<'a> {
            ops: &'a [PushOp],
        }

        let url = format!("{}/sync/push", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(token)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .json(&Body { ops })
            .send()
            .await
            .map_err(AppError::from)?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired);
        }
        if status == reqwest::StatusCode::UPGRADE_REQUIRED {
            // 426: this app version is too old. Distinct error so the engine
            // surfaces an upgrade prompt instead of retrying forever.
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::UpgradeRequired(body));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::SyncUnavailable(format!("push {status}: {body}")));
        }
        let result: PushResult = resp.json().await.map_err(AppError::from)?;
        Ok(result)
    }

    pub async fn pull(&self, token: &str, since: Option<&str>) -> AppResult<PullResponse> {
        let mut url = format!("{}/sync/pull", self.base_url);
        if let Some(cursor) = since {
            url.push_str(&format!("?since={cursor}"));
        }
        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .send()
            .await
            .map_err(AppError::from)?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired);
        }
        if status == reqwest::StatusCode::UPGRADE_REQUIRED {
            // 426: this app version is too old. Distinct error so the engine
            // surfaces an upgrade prompt instead of retrying forever.
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::UpgradeRequired(body));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::SyncUnavailable(format!("pull {status}: {body}")));
        }
        resp.json().await.map_err(AppError::from)
    }

    /// Reconcile in-flight outbox rows whose push ack was lost (e.g. the app
    /// crashed between the server applying an op and the client deleting the
    /// outbox row). Given the candidate `op_ids` (outbox rows with
    /// `attempts > 0`), returns those the server has already processed so the
    /// caller can drop them via `reconcile_outbox_lookup_response` instead of
    /// replaying. Pairs with the server's `/sync/lookup-op` route.
    ///
    /// NOTE: not yet wired into a boot-time reconciliation pass -- a tracked
    /// follow-up. The additive-entity poison this would have guarded against is
    /// already neutralized server-side (idempotent `INSERT OR IGNORE` plus the
    /// `processed_ops` dedupe table), so this is a defense-in-depth hardening,
    /// not a correctness gap. Retained as the ready transport for that pass.
    pub async fn lookup_op(&self, token: &str, op_ids: &[String]) -> AppResult<Vec<String>> {
        #[derive(Serialize)]
        struct Body<'a> {
            op_ids: &'a [String],
        }
        #[derive(Deserialize)]
        struct LookupResp {
            found: Vec<String>,
        }
        let url = format!("{}/sync/lookup-op", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(token)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .json(&Body { op_ids })
            .send()
            .await
            .map_err(AppError::from)?;
        if !resp.status().is_success() {
            return Err(AppError::SyncUnavailable(format!(
                "lookup-op {}",
                resp.status()
            )));
        }
        let body: LookupResp = resp.json().await.map_err(AppError::from)?;
        Ok(body.found)
    }

    /// Lightweight connectivity probe against the server's `/healthz` route.
    /// Carries the same `X-Device-Id` / `X-App-Version` headers as every other
    /// sync call (offline-first invariant: all sync HTTP is identified).
    ///
    /// NOTE: reserved for a future boot/pre-push connectivity check that would
    /// distinguish Offline from Online before issuing the first push -- a
    /// tracked follow-up. The push/pull loops already classify reqwest
    /// connect/timeout errors as Offline, so this is an accuracy refinement
    /// rather than a correctness gap.
    pub async fn healthz(&self) -> AppResult<bool> {
        let url = format!("{}/healthz", self.base_url);
        match self
            .client
            .get(&url)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .send()
            .await
        {
            Ok(r) => Ok(r.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn resolve_conflict(
        &self,
        token: &str,
        op_id: &str,
        resolve_op_id: &str,
        choice: &str,
        merged: Option<serde_json::Value>,
    ) -> AppResult<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            choice: &'a str,
            resolve_op_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            merged: Option<serde_json::Value>,
        }
        let url = format!("{}/sync/conflicts/{op_id}/resolve", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(token)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .json(&Body {
                choice,
                resolve_op_id,
                merged,
            })
            .send()
            .await
            .map_err(AppError::from)?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired);
        }
        if status == reqwest::StatusCode::CONFLICT {
            // Phase-08 §7.22: 409 ALREADY_RESOLVED with prior body.
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Conflict(format!("ALREADY_RESOLVED: {body}")));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::SyncUnavailable(format!(
                "resolve {status}: {body}"
            )));
        }
        Ok(())
    }

    pub async fn list_conflicts(&self, token: &str) -> AppResult<Vec<ServerConflict>> {
        #[derive(Deserialize)]
        struct Body {
            conflicts: Vec<ServerConflict>,
        }
        let url = format!("{}/sync/conflicts", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .send()
            .await
            .map_err(AppError::from)?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired);
        }
        if status == reqwest::StatusCode::UPGRADE_REQUIRED {
            // 426: this app version is too old. Distinct error so the engine
            // surfaces an upgrade prompt instead of retrying forever.
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::UpgradeRequired(body));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::SyncUnavailable(format!(
                "list_conflicts {status}: {body}"
            )));
        }
        let body: Body = resp.json().await.map_err(AppError::from)?;
        Ok(body.conflicts)
    }

    pub async fn audit_query(
        &self,
        token: &str,
        params: &AuditQueryParams,
    ) -> AppResult<AuditQueryResponse> {
        let mut url = reqwest::Url::parse(&format!("{}/audit/query", self.base_url))
            .map_err(|e| AppError::Validation(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("from", &params.from);
            q.append_pair("to", &params.to);
            if let Some(actor) = &params.actor {
                q.append_pair("actor", actor);
            }
            if let Some(action) = &params.action {
                q.append_pair("action", action);
            }
            if let Some(entity) = &params.entity {
                q.append_pair("entity", entity);
            }
            if let Some(prefix) = &params.entity_id_prefix {
                q.append_pair("entity_id_prefix", prefix);
            }
            if let Some(text) = &params.text {
                q.append_pair("text", text);
            }
            if let Some(cursor) = &params.cursor {
                q.append_pair("cursor", cursor);
            }
            if let Some(limit) = params.limit {
                q.append_pair("limit", &limit.to_string());
            }
        }
        let resp = self
            .client
            .get(url)
            .bearer_auth(token)
            .header("X-Device-Id", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .send()
            .await
            .map_err(AppError::from)?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired);
        }
        if status == reqwest::StatusCode::UPGRADE_REQUIRED {
            // 426: this app version is too old. Distinct error so the engine
            // surfaces an upgrade prompt instead of retrying forever.
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::UpgradeRequired(body));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::SyncUnavailable(format!(
                "audit_query {status}: {body}"
            )));
        }
        resp.json().await.map_err(AppError::from)
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct AuditQueryParams {
    pub from: String,
    pub to: String,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub entity: Option<String>,
    pub entity_id_prefix: Option<String>,
    pub text: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuditQueryResponse {
    pub rows: Vec<serde_json::Value>,
    pub next_cursor: Option<String>,
}

/// Encode raw bytes to base64 for JSON transport.
pub fn encode_payload(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
