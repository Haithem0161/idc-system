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

use crate::domains::patients::domain::entities::{
    Patient, PatientDemographicsInput, PatientNewInput,
};
use crate::domains::patients::domain::repositories::{
    DuplicateGroup, PatientListFilter, PatientRepo, PatientStats, VisitSummary,
};
use crate::domains::patients::service::push_payloads::PatientPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::domains::visits::domain::repositories::VisitRepo;
use crate::domains::visits::service::push_payloads::VisitPushPayload;
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
    visits: Arc<dyn VisitRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl PatientService {
    pub fn new(
        pool: sqlx::SqlitePool,
        patients: Arc<dyn PatientRepo>,
        visits: Arc<dyn VisitRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            patients,
            visits,
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

    // ---- archive reads ----------------------------------------------------

    pub async fn list(&self, filter: &PatientListFilter) -> AppResult<Vec<Patient>> {
        self.patients.list(filter).await
    }

    pub async fn list_visits(
        &self,
        patient_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<VisitSummary>> {
        self.patients
            .list_visits_by_patient(patient_id, limit, offset)
            .await
    }

    pub async fn stats(&self, patient_id: Uuid) -> AppResult<PatientStats> {
        self.patients.patient_stats(patient_id).await
    }

    pub async fn find_duplicates(&self, entity_id: &str) -> AppResult<Vec<DuplicateGroup>> {
        self.patients.find_duplicates(entity_id).await
    }

    // ---- archive writes ---------------------------------------------------

    /// Replace the patient's demographics (phone/sex/birth_date/file_no/notes).
    pub async fn update_demographics(
        &self,
        actor_user_id: Uuid,
        patient_id: Uuid,
        input: PatientDemographicsInput,
    ) -> AppResult<Patient> {
        let current = self.get(patient_id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().update_demographics(input)?;
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

    /// Un-tombstone a soft-deleted patient.
    pub async fn restore(&self, actor_user_id: Uuid, patient_id: Uuid) -> AppResult<Patient> {
        let current = self.get(patient_id).await?;
        if current.deleted_at.is_none() {
            return Ok(current);
        }
        let entity_id = current.entity_id.clone();
        let restored = current.clone().restore();
        let write = UpsertPatientWrite {
            before: Some(current),
            after: restored,
            repo: self.patients.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                // No dedicated Restore audit action; an un-delete is a state
                // update captured by the before/after delta.
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

    /// Merge `merged_id` INTO `survivor_id`: re-attribute every live visit to
    /// the survivor and tombstone the merged patient -- atomically, in one
    /// audited write that enqueues an outbox op per re-pointed visit PLUS one
    /// for the merged-patient tombstone.
    pub async fn merge(
        &self,
        actor_user_id: Uuid,
        survivor_id: Uuid,
        merged_id: Uuid,
    ) -> AppResult<()> {
        if survivor_id == merged_id {
            return Err(AppError::Validation(
                "cannot merge a patient into itself".into(),
            ));
        }
        let survivor = self.get(survivor_id).await?;
        let merged = self.get(merged_id).await?;
        if survivor.entity_id != merged.entity_id {
            return Err(AppError::Validation(
                "patients belong to different tenants".into(),
            ));
        }
        if merged.deleted_at.is_some() {
            return Err(AppError::Validation("merged patient is deleted".into()));
        }
        let entity_id = merged.entity_id.clone();
        let write = MergePatientWrite {
            survivor_id,
            merged: merged.clone(),
            patients: self.patients.clone(),
            visits: self.visits.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "patients",
                &merged_id.to_string(),
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

/// The whole patient-merge mutation as ONE `BusinessWrite`, because
/// `with_audit` opens one transaction and runs exactly one write+commit. Inside
/// a single tx it: (1) re-attributes every live visit from the merged patient
/// to the survivor and re-upserts each (so each carries a correct push
/// payload), then (2) tombstones the merged patient. It returns one outbox op
/// per re-pointed visit PLUS one for the merged-patient tombstone, so every
/// changed row is queued for push atomically.
struct MergePatientWrite {
    survivor_id: Uuid,
    merged: Patient,
    patients: Arc<dyn PatientRepo>,
    visits: Arc<dyn VisitRepo>,
}

#[async_trait]
impl BusinessWrite for MergePatientWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        // Audit before-state is the merged patient as it stood pre-merge.
        Ok(serde_json::to_value(PatientPushPayload::from(
            &self.merged,
        ))?)
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        let mut ops: Vec<OutboxOp> = Vec::new();

        // (1) Re-attribute each live visit to the survivor, in-tx.
        let visit_ids = self
            .patients
            .live_visit_ids_for_patient(tx, self.merged.id)
            .await?;
        for vid in visit_ids {
            // Load on the tx connection -- get_by_id() would acquire a second
            // pool connection and deadlock on a single-connection pool.
            let visit = self
                .visits
                .get_by_id_tx(tx, vid)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("visit {vid}")))?;
            let repointed = visit.reattribute_patient(self.survivor_id);
            self.visits.upsert(tx, &repointed).await?;
            let payload = serde_json::to_vec(&VisitPushPayload::from(&repointed))?;
            ops.push(OutboxOp::new("visits", repointed.id.to_string(), payload));
        }

        // (2) Tombstone the merged patient.
        let tombstone = self.merged.clone().soft_delete();
        self.patients.upsert(tx, &tombstone).await?;
        let after_json = serde_json::to_value(PatientPushPayload::from(&tombstone))?;
        let payload = serde_json::to_vec(&PatientPushPayload::from(&tombstone))?;
        ops.push(OutboxOp::new("patients", tombstone.id.to_string(), payload));

        Ok((after_json, ops))
    }
}
