//! `MandoubService`: superadmin-gated CRUD for the مندوب (representative)
//! catalog entity. Modeled on `OperatorService` but with no base cut and no
//! specialties, so the soft-delete has no cascade.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::mandoub::{MandoubNewInput, MandoubUpdate};
use crate::domains::catalog::domain::entities::Mandoub;
use crate::domains::catalog::domain::repositories::MandoubRepo;
use crate::domains::catalog::service::push_payloads::MandoubPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct MandoubCreateInput {
    pub name: String,
    pub phone: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MandoubUpdateInput {
    pub name: Option<String>,
    pub phone: Option<Option<String>>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone)]
pub struct MandoubService {
    pool: sqlx::SqlitePool,
    repo: Arc<dyn MandoubRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl MandoubService {
    pub fn new(
        pool: sqlx::SqlitePool,
        repo: Arc<dyn MandoubRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            repo,
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

    pub async fn list(
        &self,
        entity_id: &str,
        include_inactive: bool,
        query: Option<String>,
    ) -> AppResult<Vec<Mandoub>> {
        self.repo
            .list(
                crate::domains::catalog::domain::repositories::CatalogListFilter {
                    entity_id: entity_id.to_string(),
                    include_deleted: false,
                    include_inactive,
                    query,
                },
            )
            .await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Mandoub> {
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("mandoub {id}")))
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: MandoubCreateInput,
    ) -> AppResult<Mandoub> {
        Self::require_superadmin(actor_role)?;
        let m = Mandoub::try_new(MandoubNewInput {
            name: input.name,
            phone: input.phone,
            notes: input.notes,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = m.id;
        let write = UpsertMandoubWrite {
            before: None,
            after: m,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "mandoubs",
                &id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;
        self.get(id).await
    }

    pub async fn update(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
        input: MandoubUpdateInput,
    ) -> AppResult<Mandoub> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_updated_fields(MandoubUpdate {
            name: input.name,
            phone: input.phone,
            notes: input.notes,
        })?;
        let write = UpsertMandoubWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "mandoubs",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.get(id).await
    }

    pub async fn set_active(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
        is_active: bool,
    ) -> AppResult<Mandoub> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_active(is_active);
        let write = UpsertMandoubWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "mandoubs",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.get(id).await
    }

    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        // No specialties to cascade and no base cut: a مندوب soft-delete is a
        // single-row tombstone. Existing locked visits keep their snapshotted
        // مندوب name/cut, so dropping the catalog row never rewrites history.
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let after = current.clone().soft_deleted();
        let write = UpsertMandoubWrite {
            before: Some(current),
            after,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "mandoubs",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertMandoubWrite {
    before: Option<Mandoub>,
    after: Mandoub,
    repo: Arc<dyn MandoubRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertMandoubWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(MandoubPushPayload::from(b))?,
            None => Value::Null,
        })
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(MandoubPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&MandoubPushPayload::from(&self.after))?;
        let op = OutboxOp::new("mandoubs", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
