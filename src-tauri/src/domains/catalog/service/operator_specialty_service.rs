//! `OperatorSpecialtyService`: upsert + soft-delete with audit writes.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::operator_specialty::OperatorSpecialtyNewInput;
use crate::domains::catalog::domain::entities::OperatorSpecialty;
use crate::domains::catalog::domain::repositories::OperatorSpecialtyRepo;
use crate::domains::catalog::service::push_payloads::OperatorSpecialtyPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct OperatorSpecialtyInput {
    pub operator_id: Uuid,
    pub check_type_id: Uuid,
}

#[derive(Clone)]
pub struct OperatorSpecialtyService {
    pool: sqlx::SqlitePool,
    repo: Arc<dyn OperatorSpecialtyRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl OperatorSpecialtyService {
    pub fn new(
        pool: sqlx::SqlitePool,
        repo: Arc<dyn OperatorSpecialtyRepo>,
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

    pub async fn list_by_operator(&self, operator_id: Uuid) -> AppResult<Vec<OperatorSpecialty>> {
        self.repo.list_by_operator(operator_id).await
    }

    pub async fn upsert(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: OperatorSpecialtyInput,
    ) -> AppResult<OperatorSpecialty> {
        Self::require_superadmin(actor_role)?;
        let existing = self
            .repo
            .find_match(input.operator_id, input.check_type_id)
            .await?;
        if let Some(s) = existing {
            return Ok(s);
        }
        let specialty = OperatorSpecialty::try_new(OperatorSpecialtyNewInput {
            operator_id: input.operator_id,
            check_type_id: input.check_type_id,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = specialty.id;
        let write = UpsertSpecialtyWrite {
            after: specialty,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "operator_specialties",
                &id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::Internal("specialty vanished post-upsert".into()))
    }

    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        let current = self
            .repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("operator_specialty {id}")))?;
        let entity_id = current.entity_id.clone();
        let updated = current.soft_deleted();
        let write = UpsertSpecialtyWrite {
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "operator_specialties",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertSpecialtyWrite {
    after: OperatorSpecialty,
    repo: Arc<dyn OperatorSpecialtyRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertSpecialtyWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(Value::Null)
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(OperatorSpecialtyPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&OperatorSpecialtyPushPayload::from(&self.after))?;
        let op = OutboxOp::new("operator_specialties", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
