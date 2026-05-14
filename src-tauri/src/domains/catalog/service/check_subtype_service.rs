//! `CheckSubtypeService`: superadmin-gated CRUD with parent-state guard
//! (§7.2) and audit-first writes; emits `catalog:pricing_changed`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tauri::AppHandle;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::check_subtype::{
    CheckSubtypeNewInput, CheckSubtypeUpdate,
};
use crate::domains::catalog::domain::entities::CheckSubtype;
use crate::domains::catalog::domain::repositories::{CheckSubtypeRepo, CheckTypeRepo};
use crate::domains::catalog::events::{
    emit_pricing_changed, PricingChangeKind, PricingChangedPayload,
};
use crate::domains::catalog::service::push_payloads::CheckSubtypePushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct CheckSubtypeCreateInput {
    pub check_type_id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub price_iqd: i64,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CheckSubtypeUpdateInput {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub price_iqd: Option<i64>,
    pub sort_order: Option<i64>,
}

#[derive(Clone)]
pub struct CheckSubtypeService<R: tauri::Runtime = tauri::Wry> {
    pool: sqlx::SqlitePool,
    check_type_repo: Arc<dyn CheckTypeRepo>,
    repo: Arc<dyn CheckSubtypeRepo>,
    writer: AuditWriter,
    device_id: String,
    app: AppHandle<R>,
}

impl<R: tauri::Runtime> CheckSubtypeService<R> {
    pub fn new(
        pool: sqlx::SqlitePool,
        check_type_repo: Arc<dyn CheckTypeRepo>,
        repo: Arc<dyn CheckSubtypeRepo>,
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

    async fn require_subtyped_parent(&self, check_type_id: Uuid) -> AppResult<()> {
        let ct = self
            .check_type_repo
            .get_by_id(check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {check_type_id}")))?;
        if ct.deleted_at.is_some() {
            return Err(AppError::Validation("parent check_type is deleted".into()));
        }
        if !ct.has_subtypes {
            return Err(AppError::Validation(
                "parent check_type does not allow subtypes (errors:catalog.parent_not_subtyped)"
                    .into(),
            ));
        }
        Ok(())
    }

    pub async fn list_by_type(&self, check_type_id: Uuid) -> AppResult<Vec<CheckSubtype>> {
        self.repo.list_by_type(check_type_id).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<CheckSubtype> {
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_subtype {id}")))
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: CheckSubtypeCreateInput,
    ) -> AppResult<CheckSubtype> {
        Self::require_superadmin(actor_role)?;
        self.require_subtyped_parent(input.check_type_id).await?;
        let sub = CheckSubtype::try_new(CheckSubtypeNewInput {
            check_type_id: input.check_type_id,
            name_ar: input.name_ar,
            name_en: input.name_en,
            price_iqd: input.price_iqd,
            sort_order: input.sort_order,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = sub.id;
        let write = UpsertCheckSubtypeWrite {
            before: None,
            after: sub,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "check_subtypes",
                &id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;
        let saved = self.get(id).await?;
        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::CheckSubtype,
                changed_entity_id: saved.id,
                check_type_id: Some(saved.check_type_id),
                check_subtype_id: Some(saved.id),
                doctor_id: None,
                changed_at: Utc::now(),
            },
        );
        Ok(saved)
    }

    pub async fn update(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
        input: CheckSubtypeUpdateInput,
    ) -> AppResult<CheckSubtype> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_updated_fields(CheckSubtypeUpdate {
            name_ar: input.name_ar,
            name_en: input.name_en,
            price_iqd: input.price_iqd,
            sort_order: input.sort_order,
        })?;
        let write = UpsertCheckSubtypeWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "check_subtypes",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        let saved = self.get(id).await?;
        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::CheckSubtype,
                changed_entity_id: saved.id,
                check_type_id: Some(saved.check_type_id),
                check_subtype_id: Some(saved.id),
                doctor_id: None,
                changed_at: Utc::now(),
            },
        );
        Ok(saved)
    }

    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_deleted();
        let write = UpsertCheckSubtypeWrite {
            before: Some(current),
            after: updated.clone(),
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "check_subtypes",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::CheckSubtype,
                changed_entity_id: id,
                check_type_id: Some(updated.check_type_id),
                check_subtype_id: Some(id),
                doctor_id: None,
                changed_at: Utc::now(),
            },
        );
        Ok(())
    }
}

struct UpsertCheckSubtypeWrite {
    before: Option<CheckSubtype>,
    after: CheckSubtype,
    repo: Arc<dyn CheckSubtypeRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertCheckSubtypeWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(CheckSubtypePushPayload::from(b))?,
            None => Value::Null,
        })
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(CheckSubtypePushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&CheckSubtypePushPayload::from(&self.after))?;
        let op = OutboxOp::new("check_subtypes", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
