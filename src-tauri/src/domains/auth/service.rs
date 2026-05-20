//! AuthService: orchestrates online + offline login, refresh, change-password,
//! lock/unlock, and the underlying server HTTP calls.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domains::auth::domain::entities::{normalize_email, User};
use crate::domains::auth::domain::repositories::UserRepo;
use crate::domains::auth::domain::services::{hash_password, verify_password};
use crate::domains::auth::domain::value_objects::{LoginMode, UserRole};
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::encode_audit_payload;
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
pub struct LoginResult {
    pub mode: LoginMode,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: UserRole,
    pub entity_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerLoginResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: DateTime<Utc>,
    user: ServerUser,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerUser {
    id: String,
    email: String,
    name: String,
    role: String,
    #[serde(rename = "entityId")]
    entity_id: String,
    #[serde(rename = "passwordHash")]
    password_hash: Option<String>,
}

#[derive(Clone)]
pub struct AuthService {
    pool: sqlx::SqlitePool,
    user_repo: Arc<dyn UserRepo>,
    audit_repo: Arc<dyn AuditRepo>,
    outbox_repo: Arc<dyn OutboxRepo>,
    device_id: String,
    http: reqwest::Client,
}

impl AuthService {
    pub fn new(
        pool: sqlx::SqlitePool,
        user_repo: Arc<dyn UserRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            user_repo,
            audit_repo,
            outbox_repo,
            device_id,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Online-first login. Falls back to verifying against the locally cached
    /// `users.password_hash` when the server is unreachable.
    pub async fn login(
        &self,
        server_url: Option<&str>,
        email: &str,
        password: &str,
        entity_id_hint: &str,
    ) -> AppResult<LoginResult> {
        let email = normalize_email(email)?;

        if let Some(url) = server_url.filter(|u| !u.is_empty()) {
            match self.online_login(url, &email, password).await {
                Ok(result) => return Ok(result),
                Err(AppError::NotAuthenticated) => return Err(AppError::NotAuthenticated),
                Err(AppError::Network(_)) | Err(AppError::SyncUnavailable(_)) => {
                    // Fall through to offline.
                }
                Err(other) => return Err(other),
            }
        }

        self.offline_login(&email, password, entity_id_hint).await
    }

    /// DEF-005 fix: emit a single `audit_log` row with `action='login'` and
    /// `delta = { method: "password", mode: "online"|"offline" }` so the
    /// audit log can reconstruct who logged in, when, and through which path.
    /// Sibling outbox row pushes the same audit through the sync engine so the
    /// server-side audit query (phase-08 §3) sees client logins.
    async fn write_login_audit(
        &self,
        user_id: Uuid,
        entity_id: &str,
        mode: LoginMode,
    ) -> AppResult<()> {
        let mode_str = match mode {
            LoginMode::Online => "online",
            LoginMode::Offline => "offline",
        };
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id: user_id,
            action: AuditAction::Login,
            entity: "users".into(),
            entity_id: user_id.to_string(),
            delta: serde_json::json!({ "method": "password", "mode": mode_str }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.to_string(),
        });
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.audit_repo.append(&mut tx, &audit).await?;
        let audit_payload = encode_audit_payload(&audit)?;
        let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox_repo.enqueue(&mut tx, &audit_outbox).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }

    /// DEF-007 G18 fix: emit a single `audit_log` row with `action='logout'`
    /// so the audit log can reconstruct who logged out and when (without
    /// it, a logout silently clears the session and the forensic trail
    /// has a hole between the prior `login` row and the next state-changing
    /// action). Mirrors `write_login_audit` -- one tx, audit row + outbox
    /// push, same `entity_id_tenant` scoping. Called from
    /// `auth_logout_impl` before the session is cleared so the
    /// `actor_user_id` is still resolvable.
    pub async fn write_logout_audit(&self, user_id: Uuid, entity_id: &str) -> AppResult<()> {
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id: user_id,
            action: AuditAction::Logout,
            entity: "users".into(),
            entity_id: user_id.to_string(),
            delta: serde_json::json!({ "mode": "manual" }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.to_string(),
        });
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.audit_repo.append(&mut tx, &audit).await?;
        let audit_payload = encode_audit_payload(&audit)?;
        let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox_repo.enqueue(&mut tx, &audit_outbox).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }

    async fn online_login(
        &self,
        server_url: &str,
        email: &str,
        password: &str,
    ) -> AppResult<LoginResult> {
        let url = format!("{}/auth/login", server_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .header("X-Device-Id", &self.device_id)
            .json(&serde_json::json!({
                "email": email,
                "password": password,
                "deviceId": self.device_id,
            }))
            .send()
            .await
            .map_err(AppError::from)?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::NotAuthenticated);
        }
        if !status.is_success() {
            return Err(AppError::SyncUnavailable(format!(
                "login {status}: {}",
                resp.text().await.unwrap_or_default()
            )));
        }
        let body: ServerLoginResponse = resp.json().await.map_err(AppError::from)?;

        let role = UserRole::parse(&body.user.role)
            .ok_or_else(|| AppError::Validation(format!("invalid role: {}", body.user.role)))?;
        let id = Uuid::parse_str(&body.user.id)?;

        // Refresh or insert the local row so offline login keeps working.
        let password_hash = body.user.password_hash.clone().unwrap_or_default();
        if !password_hash.is_empty() {
            self.upsert_local_user(
                id,
                &body.user.email,
                &body.user.name,
                role,
                &body.user.entity_id,
                &password_hash,
            )
            .await?;
        }

        let result = LoginResult {
            mode: LoginMode::Online,
            user_id: id,
            email: body.user.email,
            name: body.user.name,
            role,
            entity_id: body.user.entity_id,
            access_token: Some(body.access_token),
            refresh_token: Some(body.refresh_token),
            access_token_expires_at: Some(body.expires_at),
        };
        self.write_login_audit(result.user_id, &result.entity_id, LoginMode::Online)
            .await?;
        Ok(result)
    }

    async fn offline_login(
        &self,
        email: &str,
        password: &str,
        entity_id_hint: &str,
    ) -> AppResult<LoginResult> {
        let user = self
            .user_repo
            .get_by_email(email, entity_id_hint)
            .await?
            .ok_or(AppError::NotAuthenticated)?;
        if !user.is_active {
            return Err(AppError::NotAuthenticated);
        }
        verify_password(password, &user.password_hash)?;

        let result = LoginResult {
            mode: LoginMode::Offline,
            user_id: user.id,
            email: user.email,
            name: user.name,
            role: user.role,
            entity_id: user.entity_id,
            access_token: None,
            refresh_token: None,
            access_token_expires_at: None,
        };
        self.write_login_audit(result.user_id, &result.entity_id, LoginMode::Offline)
            .await?;
        Ok(result)
    }

    async fn upsert_local_user(
        &self,
        id: Uuid,
        email: &str,
        name: &str,
        role: UserRole,
        entity_id: &str,
        password_hash: &str,
    ) -> AppResult<()> {
        let now = Utc::now();
        let user = User {
            id,
            email: normalize_email(email)?,
            name: name.to_string(),
            password_hash: password_hash.to_string(),
            role,
            is_active: true,
            last_login_at: Some(now),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: false,
            last_synced_at: Some(now),
            origin_device_id: Some(self.device_id.clone()),
            entity_id: entity_id.to_string(),
        };
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.user_repo.upsert(&mut tx, &user).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }

    /// Best-effort: register the same superadmin on the sync server so the
    /// sync engine can authenticate. Returns `Ok(())` when the server already
    /// has users (409) or is unreachable -- the local bootstrap stands on its
    /// own; remote registration is an enhancement, not a precondition.
    pub async fn bootstrap_remote_superadmin(
        &self,
        server_url: &str,
        id: Uuid,
        email: &str,
        name: &str,
        password: &str,
        entity_id: &str,
    ) -> AppResult<()> {
        let url = format!(
            "{}/auth/bootstrap-superadmin",
            server_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "id": id.to_string(),
                "email": email,
                "name": name,
                "password": password,
                "entityId": entity_id,
            }))
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => Ok(()),
            Ok(r) if r.status() == reqwest::StatusCode::CONFLICT => {
                tracing::info!(
                    "sync server already has users; skipping remote bootstrap"
                );
                Ok(())
            }
            Ok(r) => {
                tracing::warn!(
                    status = %r.status(),
                    "remote bootstrap returned non-success; continuing offline"
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(error = %e, "remote bootstrap unreachable; continuing offline");
                Ok(())
            }
        }
    }

    /// Bootstrap a first superadmin when the local user table is empty.
    /// Idempotent: returns the existing first user if any user exists.
    pub async fn create_first_admin(
        &self,
        email: &str,
        name: &str,
        password: &str,
        entity_id: &str,
    ) -> AppResult<User> {
        if self.user_repo.count().await? > 0 {
            return Err(AppError::Conflict("a user already exists".into()));
        }
        let password_hash = hash_password(password)?;
        let user = User::try_new(
            email,
            name,
            UserRole::Superadmin,
            password_hash,
            entity_id.to_string(),
            Some(self.device_id.clone()),
        )?;

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.user_repo.upsert(&mut tx, &user).await?;

        // Audit + outbox in the same tx, audit-first.
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id: user.id,
            action: AuditAction::Create,
            entity: "users".into(),
            entity_id: user.id.to_string(),
            delta: serde_json::json!({
                ".": { "from": null, "to": { "email": user.email, "role": "superadmin" } }
            }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: user.entity_id.clone(),
        });
        self.audit_repo.append(&mut tx, &audit).await?;

        let audit_payload = encode_audit_payload(&audit)?;
        let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox_repo.enqueue(&mut tx, &audit_outbox).await?;

        let user_payload = serde_json::to_vec(&user)?;
        let user_outbox = OutboxOp::new("users", user.id.to_string(), user_payload);
        self.outbox_repo.enqueue(&mut tx, &user_outbox).await?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(user)
    }

    /// Verify a password against a stored user row (used by lock/unlock).
    pub async fn verify_user_password(&self, user_id: Uuid, password: &str) -> AppResult<()> {
        let user = self
            .user_repo
            .get_by_id(user_id)
            .await?
            .ok_or(AppError::NotAuthenticated)?;
        verify_password(password, &user.password_hash)
    }

    /// DEF-007 G01: rotate access + refresh tokens against the server's
    /// `/auth/refresh` endpoint. Returns the new pair so the IPC wrapper
    /// can persist them in `AppState` and emit `auth:refreshed`. The
    /// `device_id` header is sent so the server's per-device session
    /// tracking (phase-02 §3 `RefreshToken.deviceId`) stays correct.
    ///
    /// Errors:
    /// - `AppError::NotAuthenticated` when no `server_url` is set OR the
    ///   server returns 401 (revoked or expired refresh token).
    /// - `AppError::Network` on connection failure.
    /// - `AppError::SyncUnavailable` on non-401 non-2xx responses.
    pub async fn refresh(
        &self,
        server_url: Option<&str>,
        refresh_token: &str,
    ) -> AppResult<RefreshResult> {
        let server_url = server_url
            .filter(|u| !u.is_empty())
            .ok_or(AppError::NotAuthenticated)?;
        let url = format!("{}/auth/refresh", server_url.trim_end_matches('/'));

        let resp = self
            .http
            .post(&url)
            .header("X-Device-Id", &self.device_id)
            .json(&ServerRefreshRequest {
                refresh_token: refresh_token.to_string(),
            })
            .send()
            .await
            .map_err(AppError::from)?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::NotAuthenticated);
        }
        if !status.is_success() {
            return Err(AppError::SyncUnavailable(format!(
                "refresh {status}: {}",
                resp.text().await.unwrap_or_default()
            )));
        }
        let body: ServerRefreshResponse = resp.json().await.map_err(AppError::from)?;
        Ok(RefreshResult {
            access_token: body.access_token,
            refresh_token: body.refresh_token,
            access_token_expires_at: body.expires_at,
            refreshed_at: Utc::now(),
        })
    }

    /// DEF-007 G31: change the current user's password. ONLINE-REQUIRED.
    ///
    /// Per phase-02.md §4 Tauri `AuthService::change_password` step 1, this
    /// operation MUST refuse to run when the device is offline -- the
    /// server is the canonical password store and a local-only rotation
    /// would leak through into a re-sync that overwrote the hash on the
    /// next pull. The offline signal is "no `server_url` configured" (and
    /// downstream HTTP failures translate to `AppError::OfflineNotAllowed`
    /// rather than `Network` so the UI can render the right message).
    ///
    /// On success: server rotates the `passwordHash` and revokes existing
    /// refresh tokens; we also rotate the local cached hash so the next
    /// offline login uses the new password.
    pub async fn change_password(
        &self,
        server_url: Option<&str>,
        access_token: &str,
        user_id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> AppResult<()> {
        let server_url = server_url
            .filter(|u| !u.is_empty())
            .ok_or(AppError::OfflineNotAllowed)?;
        if new_password.len() < 8 {
            return Err(AppError::Validation(
                "new password must be at least 8 characters".into(),
            ));
        }

        let url = format!("{}/auth/change-password", server_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(access_token)
            .header("X-Device-Id", &self.device_id)
            .json(&serde_json::json!({
                "oldPassword": current_password,
                "newPassword": new_password,
            }))
            .send()
            .await
            .map_err(|e| {
                // Translate connection-level failures to OfflineNotAllowed
                // per §4 step 1 -- the call MUST NOT half-succeed. We probe
                // `is_timeout` / `is_connect` directly to avoid losing the
                // distinction inside the broader `AppError::from` mapping.
                if e.is_timeout() || e.is_connect() {
                    AppError::OfflineNotAllowed
                } else {
                    AppError::from(e)
                }
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::NotAuthenticated);
        }
        if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
            return Err(AppError::Validation(format!(
                "change-password rejected: {}",
                resp.text().await.unwrap_or_default()
            )));
        }
        if !status.is_success() {
            return Err(AppError::SyncUnavailable(format!(
                "change-password {status}: {}",
                resp.text().await.unwrap_or_default()
            )));
        }

        // Rotate the local cached hash so offline login keeps working with
        // the new password on the next session. We DO bump `version` and
        // mark dirty -- a `password_change` audit row goes through the
        // outbox so the audit query (phase-08 §3) sees the rotation.
        let user = self
            .user_repo
            .get_by_id(user_id)
            .await?
            .ok_or(AppError::NotAuthenticated)?;
        let new_hash = hash_password(new_password)?;
        let updated = user.clone().with_new_password_hash(new_hash);

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.user_repo.upsert(&mut tx, &updated).await?;
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id: user_id,
            action: AuditAction::PasswordChange,
            entity: "users".into(),
            entity_id: user_id.to_string(),
            delta: serde_json::json!({ "method": "user", "mode": "online" }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: user.entity_id.clone(),
        });
        self.audit_repo.append(&mut tx, &audit).await?;
        let audit_payload = encode_audit_payload(&audit)?;
        let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox_repo.enqueue(&mut tx, &audit_outbox).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }
}

/// DEF-007 G01: result of a successful `/auth/refresh` rotation. The IPC
/// wrapper persists the new tokens in `AppState` and emits
/// `auth:refreshed` with `{ refreshed_at }`.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshResult {
    pub access_token: String,
    pub refresh_token: String,
    pub access_token_expires_at: DateTime<Utc>,
    pub refreshed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
struct ServerRefreshRequest {
    #[serde(rename = "refreshToken")]
    refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerRefreshResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: DateTime<Utc>,
}
