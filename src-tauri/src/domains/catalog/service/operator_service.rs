//! `OperatorService`: superadmin-gated CRUD + cascading soft-delete to
//! specialties (§7.22).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::operator::{OperatorNewInput, OperatorUpdate};
use crate::domains::catalog::domain::entities::{Operator, OperatorSpecialty};
use crate::domains::catalog::domain::repositories::{OperatorRepo, OperatorSpecialtyRepo};
use crate::domains::catalog::service::push_payloads::{
    OperatorPushPayload, OperatorSpecialtyPushPayload,
};
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct OperatorCreateInput {
    pub name: String,
    pub phone: Option<String>,
    pub base_cut_per_check_iqd: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OperatorUpdateInput {
    pub name: Option<String>,
    pub phone: Option<Option<String>>,
    pub base_cut_per_check_iqd: Option<i64>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone)]
pub struct OperatorService {
    pool: sqlx::SqlitePool,
    repo: Arc<dyn OperatorRepo>,
    specialty_repo: Arc<dyn OperatorSpecialtyRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl OperatorService {
    pub fn new(
        pool: sqlx::SqlitePool,
        repo: Arc<dyn OperatorRepo>,
        specialty_repo: Arc<dyn OperatorSpecialtyRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            repo,
            specialty_repo,
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
    ) -> AppResult<Vec<Operator>> {
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

    pub async fn get(&self, id: Uuid) -> AppResult<Operator> {
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("operator {id}")))
    }

    pub async fn get_with_specialties(
        &self,
        id: Uuid,
    ) -> AppResult<(Operator, Vec<OperatorSpecialty>)> {
        let op = self.get(id).await?;
        let specs = self.specialty_repo.list_by_operator(id).await?;
        Ok((op, specs))
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: OperatorCreateInput,
    ) -> AppResult<Operator> {
        Self::require_superadmin(actor_role)?;
        let op = Operator::try_new(OperatorNewInput {
            name: input.name,
            phone: input.phone,
            base_cut_per_check_iqd: input.base_cut_per_check_iqd,
            notes: input.notes,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = op.id;
        let write = UpsertOperatorWrite {
            before: None,
            after: op,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "operators",
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
        input: OperatorUpdateInput,
    ) -> AppResult<Operator> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_updated_fields(OperatorUpdate {
            name: input.name,
            phone: input.phone,
            base_cut_per_check_iqd: input.base_cut_per_check_iqd,
            notes: input.notes,
        })?;
        let write = UpsertOperatorWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "operators",
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
    ) -> AppResult<Operator> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_active(is_active);
        let write = UpsertOperatorWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "operators",
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
        // Soft-deleting an operator cascades to their specialties so we don't
        // leave orphan FKs. Open shifts are NOT blocked here — the PRD treats
        // a shift on a soft-deleted operator as an audit-trail artefact, not
        // a blocker on the deletion.
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let specialties = self.specialty_repo.list_by_operator(id).await?;
        let operator_after = current.clone().soft_deleted();
        let specialties_after: Vec<OperatorSpecialty> = specialties
            .iter()
            .cloned()
            .map(|s| s.soft_deleted())
            .collect();
        let write = SoftDeleteOperatorWrite {
            before: current,
            operator_after,
            specialties_after,
            operator_repo: self.repo.clone(),
            specialty_repo: self.specialty_repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "operators",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertOperatorWrite {
    before: Option<Operator>,
    after: Operator,
    repo: Arc<dyn OperatorRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertOperatorWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(OperatorPushPayload::from(b))?,
            None => Value::Null,
        })
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(OperatorPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&OperatorPushPayload::from(&self.after))?;
        let op = OutboxOp::new("operators", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}

struct SoftDeleteOperatorWrite {
    before: Operator,
    operator_after: Operator,
    specialties_after: Vec<OperatorSpecialty>,
    operator_repo: Arc<dyn OperatorRepo>,
    specialty_repo: Arc<dyn OperatorSpecialtyRepo>,
}

#[async_trait]
impl BusinessWrite for SoftDeleteOperatorWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(serde_json::to_value(OperatorPushPayload::from(
            &self.before,
        ))?)
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.operator_repo.upsert(tx, &self.operator_after).await?;
        let mut ops = Vec::with_capacity(1 + self.specialties_after.len());
        let payload = serde_json::to_vec(&OperatorPushPayload::from(&self.operator_after))?;
        ops.push(OutboxOp::new(
            "operators",
            self.operator_after.id.to_string(),
            payload,
        ));
        for s in &self.specialties_after {
            self.specialty_repo.upsert(tx, s).await?;
            let bytes = serde_json::to_vec(&OperatorSpecialtyPushPayload::from(s))?;
            ops.push(OutboxOp::new(
                "operator_specialties",
                s.id.to_string(),
                bytes,
            ));
        }
        let after_json = serde_json::to_value(OperatorPushPayload::from(&self.operator_after))?;
        Ok((after_json, ops))
    }
}
