//! `DoctorPricingService`: upsert with subtype-required guard (§7.6, §7.20)
//! and `catalog:pricing_changed` event emission.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tauri::AppHandle;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::doctor_pricing::DoctorPricingNewInput;
use crate::domains::catalog::domain::entities::DoctorCheckPricing;
use crate::domains::catalog::domain::repositories::{CheckTypeRepo, DoctorPricingRepo};
use crate::domains::catalog::domain::value_objects::CutKind;
use crate::domains::catalog::events::{
    emit_pricing_changed, PricingChangeKind, PricingChangedPayload,
};
use crate::domains::catalog::service::push_payloads::DoctorPricingPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct DoctorPricingUpsertInput {
    pub doctor_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub price_override_iqd: Option<i64>,
    pub cut_kind: CutKind,
    pub cut_value: i64,
}

#[derive(Clone)]
pub struct DoctorPricingService<R: tauri::Runtime = tauri::Wry> {
    pool: sqlx::SqlitePool,
    check_type_repo: Arc<dyn CheckTypeRepo>,
    repo: Arc<dyn DoctorPricingRepo>,
    writer: AuditWriter,
    device_id: String,
    app: AppHandle<R>,
}

impl<R: tauri::Runtime> DoctorPricingService<R> {
    pub fn new(
        pool: sqlx::SqlitePool,
        check_type_repo: Arc<dyn CheckTypeRepo>,
        repo: Arc<dyn DoctorPricingRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
        app: AppHandle<R>,
    ) -> Self {
        Self {
            pool,
            check_type_repo,
            repo,
            writer: AuditWriter::new(audit_repo, outbox_repo, device_id.clone()),
            device_id,
            app,
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

    pub async fn list_by_doctor(&self, doctor_id: Uuid) -> AppResult<Vec<DoctorCheckPricing>> {
        self.repo.list_by_doctor(doctor_id).await
    }

    pub async fn upsert(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: DoctorPricingUpsertInput,
    ) -> AppResult<DoctorCheckPricing> {
        Self::require_superadmin(actor_role)?;
        let ct = self
            .check_type_repo
            .get_by_id(input.check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {}", input.check_type_id)))?;
        match (ct.has_subtypes, input.check_subtype_id) {
            (true, None) => {
                return Err(AppError::Validation(
                    "check_subtype_id required when parent has subtypes (errors:consumption.subtype_required)"
                        .into(),
                ));
            }
            (false, Some(_)) => {
                return Err(AppError::Validation(
                    "check_subtype_id forbidden when parent has no subtypes (errors:consumption.subtype_forbidden)"
                        .into(),
                ));
            }
            _ => {}
        }

        let existing = self
            .repo
            .find_match(input.doctor_id, input.check_type_id, input.check_subtype_id)
            .await?;

        let (after, action) = match existing {
            Some(current) => {
                let updated = current.clone().updated_with(
                    input.price_override_iqd,
                    input.cut_kind,
                    input.cut_value,
                )?;
                (updated, AuditAction::Update)
            }
            None => {
                let pricing = DoctorCheckPricing::try_new(DoctorPricingNewInput {
                    doctor_id: input.doctor_id,
                    check_type_id: input.check_type_id,
                    check_subtype_id: input.check_subtype_id,
                    price_override_iqd: input.price_override_iqd,
                    cut_kind: input.cut_kind,
                    cut_value: input.cut_value,
                    entity_id: entity_id.to_string(),
                    origin_device_id: Some(self.device_id.clone()),
                })?;
                (pricing, AuditAction::Create)
            }
        };

        let id = after.id;
        let entity_id_owned = entity_id.to_string();
        let write = UpsertPricingWrite {
            after: after.clone(),
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                action,
                "doctor_check_pricing",
                &id.to_string(),
                &entity_id_owned,
                None,
                write,
            )
            .await?;

        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::DoctorPricing,
                changed_entity_id: id,
                check_type_id: Some(after.check_type_id),
                check_subtype_id: after.check_subtype_id,
                doctor_id: Some(after.doctor_id),
                changed_at: Utc::now(),
            },
        );

        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::Internal("pricing vanished post-upsert".into()))
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
            .ok_or_else(|| AppError::NotFound(format!("doctor_check_pricing {id}")))?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_deleted();
        let write = UpsertPricingWrite {
            after: updated.clone(),
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "doctor_check_pricing",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::DoctorPricing,
                changed_entity_id: id,
                check_type_id: Some(updated.check_type_id),
                check_subtype_id: updated.check_subtype_id,
                doctor_id: Some(updated.doctor_id),
                changed_at: Utc::now(),
            },
        );
        Ok(())
    }
}

struct UpsertPricingWrite {
    after: DoctorCheckPricing,
    repo: Arc<dyn DoctorPricingRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertPricingWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(Value::Null)
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(DoctorPricingPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&DoctorPricingPushPayload::from(&self.after))?;
        let op = OutboxOp::new("doctor_check_pricing", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
