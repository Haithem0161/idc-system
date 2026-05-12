//! `PatientService`: create / get / search / rename / soft_delete.
//!
//! Soft-delete refuses when any non-deleted visit references the patient
//! (§7.34). Each mutator goes through `AuditWriter::with_audit` so the
//! audit row precedes the business write (PRD §4.3).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::patients::domain::entities::{Patient, PatientNewInput};
use crate::domains::patients::domain::repositories::PatientRepo;
use crate::domains::patients::service::push_payloads::PatientPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct PatientCreateInput {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatientUpdateInput {
    pub name: String,
}

#[derive(Clone)]
pub struct PatientService {
    pool: sqlx::SqlitePool,
    patients: Arc<dyn PatientRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl PatientService {
    pub fn new(
        pool: sqlx::SqlitePool,
        patients: Arc<dyn PatientRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            patients,
            writer: AuditWriter::new(audit_repo, outbox_repo, device_id.clone()),
            device_id,
        }
    }

    pub fn repo(&self) -> Arc<dyn PatientRepo> {
        self.patients.clone()
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Patient> {
        self.patients
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("patient {id}")))
    }

    pub async fn search(
        &self,
        entity_id: &str,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<Patient>> {
        self.patients.search(entity_id, query, limit).await
    }

    pub async fn list_recent(&self, entity_id: &str, limit: i64) -> AppResult<Vec<Patient>> {
        self.patients.list_recent(entity_id, limit).await
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        entity_id: &str,
        input: PatientCreateInput,
    ) -> AppResult<Patient> {
        let patient = Patient::try_new(PatientNewInput {
            name: input.name,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = patient.id;
        let write = UpsertPatientWrite {
            before: None,
            after: patient,
            repo: self.patients.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "patients",
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
        patient_id: Uuid,
        input: PatientUpdateInput,
    ) -> AppResult<Patient> {
        let current = self.get(patient_id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().rename(&input.name)?;
        let write = UpsertPatientWrite {
            before: Some(current),
            after: updated,
            repo: self.patients.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "patients",
                &patient_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.get(patient_id).await
    }

    /// §7.34: refuse if the patient is referenced by any non-deleted visit.
    pub async fn soft_delete(&self, actor_user_id: Uuid, patient_id: Uuid) -> AppResult<()> {
        let current = self.get(patient_id).await?;
        if current.deleted_at.is_some() {
            return Ok(());
        }
        let live_refs = self.patients.count_live_visits(patient_id).await?;
        if live_refs > 0 {
            return Err(AppError::Conflict(
                "patient is referenced by visits; cannot delete".into(),
            ));
        }
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_delete();
        let write = UpsertPatientWrite {
            before: Some(current),
            after: updated,
            repo: self.patients.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "patients",
                &patient_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertPatientWrite {
    before: Option<Patient>,
    after: Patient,
    repo: Arc<dyn PatientRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertPatientWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(PatientPushPayload::from(b))?,
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(PatientPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&PatientPushPayload::from(&self.after))?;
        let op = OutboxOp::new("patients", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
