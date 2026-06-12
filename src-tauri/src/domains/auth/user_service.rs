//! UserService: superadmin-gated CRUD + reset_password with audit-first
//! ordering. Public auth flows (login, refresh, lock) live in `service.rs`.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::entities::{normalize_email, User};
use crate::domains::auth::domain::repositories::UserRepo;
use crate::domains::auth::domain::services::hash_password;
use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct UserCreateInput {
    pub email: String,
    pub name: String,
    pub role: UserRole,
    pub password: String,
    pub entity_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct UserUpdateInput {
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: Option<UserRole>,
}

#[derive(Clone)]
pub struct UserService {
    pool: sqlx::SqlitePool,
    user_repo: Arc<dyn UserRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl UserService {
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
            writer: AuditWriter::new(audit_repo, outbox_repo, device_id.clone()),
            device_id,
        }
    }

    fn require_superadmin(role: UserRole) -> AppResult<()> {
        if role != UserRole::Superadmin {
            Err(AppError::Validation(
                "this action requires the superadmin role".into(),
            ))
        } else {
            Ok(())
        }
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        input: UserCreateInput,
    ) -> AppResult<User> {
        Self::require_superadmin(actor_role)?;
        let password_hash = hash_password(&input.password)?;
        let user = User::try_new(
            &input.email,
            &input.name,
            input.role,
            password_hash,
            input.entity_id.clone(),
            Some(self.device_id.clone()),
        )?;

        if self
            .user_repo
            .get_by_email(&user.email, &input.entity_id)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "user with email {} already exists",
                user.email
            )));
        }

        let id = user.id;
        let entity_id = input.entity_id.clone();
        let write = CreateUserWrite {
            user: user.clone(),
            user_repo: self.user_repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "users",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;

        self.user_repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::Internal("user vanished post-create".into()))
    }

    pub async fn update(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        target_id: Uuid,
        input: UserUpdateInput,
    ) -> AppResult<User> {
        Self::require_superadmin(actor_role)?;
        let current = self
            .user_repo
            .get_by_id(target_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("user {target_id}")))?;

        let email = input
            .email
            .as_ref()
            .map(|e| normalize_email(e))
            .transpose()?;
        let updated = current
            .clone()
            .with_updated_fields(input.name, email, input.role)?;

        let write = UpdateUserWrite {
            before: current,
            after: updated.clone(),
            user_repo: self.user_repo.clone(),
        };

        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "users",
                &target_id.to_string(),
                &updated.entity_id,
                None,
                write,
            )
            .await?;

        self.user_repo
            .get_by_id(target_id)
            .await?
            .ok_or_else(|| AppError::Internal("user vanished post-update".into()))
    }

    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        target_id: Uuid,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        let current = self
            .user_repo
            .get_by_id(target_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("user {target_id}")))?;
        let deleted = current.clone().soft_deleted();
        let write = UpdateUserWrite {
            before: current,
            after: deleted.clone(),
            user_repo: self.user_repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "users",
                &target_id.to_string(),
                &deleted.entity_id,
                None,
                write,
            )
            .await?;
        Ok(())
    }

    pub async fn reset_password(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        target_id: Uuid,
        new_password: &str,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        let current = self
            .user_repo
            .get_by_id(target_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("user {target_id}")))?;
        let new_hash = hash_password(new_password)?;
        let updated = current.clone().with_new_password_hash(new_hash);
        let write = UpdateUserWrite {
            before: current,
            after: updated.clone(),
            user_repo: self.user_repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::PasswordChange,
                "users",
                &target_id.to_string(),
                &updated.entity_id,
                None,
                write,
            )
            .await?;
        Ok(())
    }
}

#[derive(Serialize)]
pub(crate) struct UserPushPayload {
    id: String,
    email: String,
    name: String,
    // Omit entirely when the password did not change. Serializing `null`
    // lets the server coerce it to '' and wipe the stored hash (DEF: user
    // update destroys credential). `skip_serializing_if` drops the key so
    // the server's update path leaves passwordHash untouched.
    #[serde(skip_serializing_if = "Option::is_none")]
    password_hash: Option<String>,
    role: String,
    is_active: bool,
    entity_id: String,
    version: i64,
    updated_at: String,
    deleted_at: Option<String>,
}

pub(crate) fn to_push_payload(user: &User, include_hash: bool) -> UserPushPayload {
    UserPushPayload {
        id: user.id.to_string(),
        email: user.email.clone(),
        name: user.name.clone(),
        password_hash: if include_hash {
            Some(user.password_hash.clone())
        } else {
            None
        },
        role: user.role.as_str().to_string(),
        is_active: user.is_active,
        entity_id: user.entity_id.clone(),
        version: user.version,
        updated_at: user.updated_at.to_rfc3339(),
        deleted_at: user.deleted_at.map(|d| d.to_rfc3339()),
    }
}

struct CreateUserWrite {
    user: User,
    user_repo: Arc<dyn UserRepo>,
}

#[async_trait]
impl BusinessWrite for CreateUserWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(Value::Null)
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.user_repo.upsert(tx, &self.user).await?;
        let after = serde_json::json!({
            "id": self.user.id.to_string(),
            "email": self.user.email,
            "name": self.user.name,
            "role": self.user.role.as_str(),
            "is_active": self.user.is_active,
        });
        let payload = serde_json::to_vec(&to_push_payload(&self.user, true))?;
        let outbox = OutboxOp::new("users", self.user.id.to_string(), payload);
        Ok((after, vec![outbox]))
    }
}

struct UpdateUserWrite {
    before: User,
    after: User,
    user_repo: Arc<dyn UserRepo>,
}

#[async_trait]
impl BusinessWrite for UpdateUserWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(serde_json::json!({
            "email": self.before.email,
            "name": self.before.name,
            "role": self.before.role.as_str(),
            "is_active": self.before.is_active,
            "deleted_at": self.before.deleted_at,
        }))
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.user_repo.upsert(tx, &self.after).await?;
        let after = serde_json::json!({
            "email": self.after.email,
            "name": self.after.name,
            "role": self.after.role.as_str(),
            "is_active": self.after.is_active,
            "deleted_at": self.after.deleted_at,
        });
        let include_hash = self.after.password_hash != self.before.password_hash;
        let payload = serde_json::to_vec(&to_push_payload(&self.after, include_hash))?;
        let outbox = OutboxOp::new("users", self.after.id.to_string(), payload);
        Ok((after, vec![outbox]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_superadmin_accepts_superadmin_role() {
        assert!(UserService::require_superadmin(UserRole::Superadmin).is_ok());
    }

    #[test]
    fn require_superadmin_rejects_receptionist() {
        let err = UserService::require_superadmin(UserRole::Receptionist).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn require_superadmin_rejects_accountant() {
        let err = UserService::require_superadmin(UserRole::Accountant).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn to_push_payload_omits_password_hash_when_flag_false() {
        let user = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            "$argon2id$v=19$LOCAL_HASH".into(),
            "tenant-1".into(),
            None,
        )
        .unwrap();
        let payload = to_push_payload(&user, false);
        assert!(payload.password_hash.is_none());
        let json = serde_json::to_string(&payload).unwrap();
        assert!(!json.contains("LOCAL_HASH"));
        // The key must be ABSENT, not `null`. A `null` lets the server coerce
        // it to '' and wipe the stored hash; an absent key leaves it untouched.
        assert!(
            !json.contains("password_hash"),
            "unchanged-password push must omit the field entirely, got: {json}"
        );
    }

    #[test]
    fn to_push_payload_includes_password_hash_when_flag_true() {
        let user = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            "$argon2id$v=19$LOCAL_HASH".into(),
            "tenant-1".into(),
            None,
        )
        .unwrap();
        let payload = to_push_payload(&user, true);
        assert_eq!(
            payload.password_hash.as_deref(),
            Some("$argon2id$v=19$LOCAL_HASH")
        );
    }

    #[test]
    fn to_push_payload_preserves_version_email_role_and_active_state() {
        let user = User::try_new(
            "A@B.IO",
            "Mariam",
            UserRole::Superadmin,
            "$h".into(),
            "tenant-1".into(),
            Some("dev-1".into()),
        )
        .unwrap();
        let payload = to_push_payload(&user, false);
        assert_eq!(payload.email, "a@b.io");
        assert_eq!(payload.role, "superadmin");
        assert!(payload.is_active);
        assert_eq!(payload.version, 1);
        assert_eq!(payload.entity_id, "tenant-1");
    }
}
