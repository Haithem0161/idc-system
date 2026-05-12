//! `DoctorService`: superadmin-gated CRUD + soft-delete cascade to pricings.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::doctor::{DoctorNewInput, DoctorUpdate};
use crate::domains::catalog::domain::entities::{Doctor, DoctorCheckPricing};
use crate::domains::catalog::domain::repositories::{DoctorPricingRepo, DoctorRepo};
use crate::domains::catalog::service::push_payloads::{
    DoctorPricingPushPayload, DoctorPushPayload,
};
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct DoctorCreateInput {
    pub name: String,
    pub specialty: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DoctorUpdateInput {
    pub name: Option<String>,
    pub specialty: Option<Option<String>>,
    pub phone: Option<Option<String>>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone)]
pub struct DoctorService {
    pool: sqlx::SqlitePool,
    repo: Arc<dyn DoctorRepo>,
    pricing_repo: Arc<dyn DoctorPricingRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl DoctorService {
    pub fn new(
        pool: sqlx::SqlitePool,
        repo: Arc<dyn DoctorRepo>,
        pricing_repo: Arc<dyn DoctorPricingRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            repo,
            pricing_repo,
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
    ) -> AppResult<Vec<Doctor>> {
        if let Some(q) = query.as_ref().filter(|q| q.trim().chars().count() >= 2) {
            self.repo
                .search_fts(entity_id, q.trim(), include_inactive)
                .await
        } else {
            self.repo
                .list(
                    crate::domains::catalog::domain::repositories::CatalogListFilter {
                        entity_id: entity_id.to_string(),
                        include_deleted: false,
                        include_inactive,
                        query: None,
                    },
                )
                .await
        }
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Doctor> {
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("doctor {id}")))
    }

    pub async fn get_with_pricings(
        &self,
        id: Uuid,
    ) -> AppResult<(Doctor, Vec<DoctorCheckPricing>)> {
        let doctor = self.get(id).await?;
        let pricings = self.pricing_repo.list_by_doctor(id).await?;
        Ok((doctor, pricings))
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: DoctorCreateInput,
    ) -> AppResult<Doctor> {
        Self::require_superadmin(actor_role)?;
        let doctor = Doctor::try_new(DoctorNewInput {
            name: input.name,
            specialty: input.specialty,
            phone: input.phone,
            notes: input.notes,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = doctor.id;
        let write = UpsertDoctorWrite {
            before: None,
            after: doctor,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "doctors",
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
        input: DoctorUpdateInput,
    ) -> AppResult<Doctor> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_updated_fields(DoctorUpdate {
            name: input.name,
            specialty: input.specialty,
            phone: input.phone,
            notes: input.notes,
        })?;
        let write = UpsertDoctorWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "doctors",
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
    ) -> AppResult<Doctor> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_active(is_active);
        let write = UpsertDoctorWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "doctors",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.get(id).await
    }

    /// Soft delete the doctor and cascade soft-delete every live pricing row.
    /// Each pricing soft-delete emits its own outbox op so the server stays
    /// in sync (§7.22 mirror).
    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let pricings = self.pricing_repo.list_by_doctor(id).await?;

        let doctor_after = current.clone().soft_deleted();
        let pricings_after: Vec<DoctorCheckPricing> =
            pricings.iter().cloned().map(|p| p.soft_deleted()).collect();

        let write = SoftDeleteDoctorWrite {
            before: current,
            doctor_after,
            pricings_after,
            doctor_repo: self.repo.clone(),
            pricing_repo: self.pricing_repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "doctors",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertDoctorWrite {
    before: Option<Doctor>,
    after: Doctor,
    repo: Arc<dyn DoctorRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertDoctorWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(DoctorPushPayload::from(b))?,
            None => Value::Null,
        })
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(DoctorPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&DoctorPushPayload::from(&self.after))?;
        let op = OutboxOp::new("doctors", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}

struct SoftDeleteDoctorWrite {
    before: Doctor,
    doctor_after: Doctor,
    pricings_after: Vec<DoctorCheckPricing>,
    doctor_repo: Arc<dyn DoctorRepo>,
    pricing_repo: Arc<dyn DoctorPricingRepo>,
}

#[async_trait]
impl BusinessWrite for SoftDeleteDoctorWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(serde_json::to_value(DoctorPushPayload::from(&self.before))?)
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.doctor_repo.upsert(tx, &self.doctor_after).await?;
        let mut ops = Vec::with_capacity(1 + self.pricings_after.len());
        let payload = serde_json::to_vec(&DoctorPushPayload::from(&self.doctor_after))?;
        ops.push(OutboxOp::new(
            "doctors",
            self.doctor_after.id.to_string(),
            payload,
        ));
        for p in &self.pricings_after {
            self.pricing_repo.upsert(tx, p).await?;
            let bytes = serde_json::to_vec(&DoctorPricingPushPayload::from(p))?;
            ops.push(OutboxOp::new(
                "doctor_check_pricing",
                p.id.to_string(),
                bytes,
            ));
        }
        let after_json = serde_json::to_value(DoctorPushPayload::from(&self.doctor_after))?;
        Ok((after_json, ops))
    }
}
