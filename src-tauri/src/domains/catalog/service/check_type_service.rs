//! `CheckTypeService`: superadmin-gated CRUD + XOR / toggle invariants
//! (§7.1, §7.3) with audit-first writes and `catalog:pricing_changed` event.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tauri::AppHandle;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::check_type::{CheckTypeNewInput, CheckTypeUpdate};
use crate::domains::catalog::domain::entities::CheckType;
use crate::domains::catalog::domain::repositories::CheckTypeRepo;
use crate::domains::catalog::events::{
    emit_pricing_changed, PricingChangeKind, PricingChangedPayload,
};
use crate::domains::catalog::service::push_payloads::CheckTypePushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct CheckTypeCreateInput {
    pub name_ar: String,
    pub name_en: Option<String>,
    pub has_subtypes: bool,
    pub base_price_iqd: Option<i64>,
    pub dye_price_iqd: Option<i64>,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CheckTypeUpdateInput {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub base_price_iqd: Option<Option<i64>>,
    pub dye_price_iqd: Option<Option<i64>>,
    pub sort_order: Option<i64>,
    pub is_active: Option<bool>,
}

#[derive(Clone)]
pub struct CheckTypeService<R: tauri::Runtime = tauri::Wry> {
    pool: sqlx::SqlitePool,
    repo: Arc<dyn CheckTypeRepo>,
    writer: AuditWriter,
    device_id: String,
    app: AppHandle<R>,
}

impl<R: tauri::Runtime> CheckTypeService<R> {
    pub fn new(
        pool: sqlx::SqlitePool,
        repo: Arc<dyn CheckTypeRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
        app: AppHandle<R>,
    ) -> Self {
        Self {
            pool,
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

    pub async fn list(
        &self,
        entity_id: &str,
        include_inactive: bool,
        query: Option<String>,
    ) -> AppResult<Vec<CheckType>> {
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

    pub async fn get(&self, id: Uuid) -> AppResult<CheckType> {
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {id}")))
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: CheckTypeCreateInput,
    ) -> AppResult<CheckType> {
        Self::require_superadmin(actor_role)?;
        let ct = CheckType::try_new(CheckTypeNewInput {
            name_ar: input.name_ar,
            name_en: input.name_en,
            has_subtypes: input.has_subtypes,
            base_price_iqd: input.base_price_iqd,
            dye_price_iqd: input.dye_price_iqd,
            sort_order: input.sort_order,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;

        let id = ct.id;
        let write = UpsertCheckTypeWrite {
            before: None,
            after: ct,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "check_types",
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
                kind: PricingChangeKind::CheckType,
                changed_entity_id: saved.id,
                check_type_id: Some(saved.id),
                check_subtype_id: None,
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
        input: CheckTypeUpdateInput,
    ) -> AppResult<CheckType> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let patch = CheckTypeUpdate {
            name_ar: input.name_ar,
            name_en: input.name_en,
            base_price_iqd: input.base_price_iqd,
            dye_price_iqd: input.dye_price_iqd,
            sort_order: input.sort_order,
            is_active: input.is_active,
        };
        let updated = current.clone().with_updated_fields(patch)?;
        let write = UpsertCheckTypeWrite {
            before: Some(current),
            after: updated.clone(),
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "check_types",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::CheckType,
                changed_entity_id: id,
                check_type_id: Some(id),
                check_subtype_id: None,
                doctor_id: None,
                changed_at: Utc::now(),
            },
        );
        self.get(id).await
    }

    pub async fn toggle_has_subtypes(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
        to_value: bool,
        new_base_price: Option<i64>,
    ) -> AppResult<CheckType> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        if !to_value {
            // 1 -> 0: block when any non-deleted subtypes exist.
            let live = self.repo.count_live_subtypes(id).await?;
            if live > 0 {
                return Err(AppError::Conflict(
                    "soft-delete all subtypes first (errors:catalog.subtypes_exist)".into(),
                ));
            }
        }
        let entity_id = current.entity_id.clone();
        let updated = current
            .clone()
            .toggled_has_subtypes(to_value, new_base_price)?;
        let write = UpsertCheckTypeWrite {
            before: Some(current),
            after: updated.clone(),
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "check_types",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        emit_pricing_changed(
            &self.app,
            PricingChangedPayload {
                kind: PricingChangeKind::CheckType,
                changed_entity_id: id,
                check_type_id: Some(id),
                check_subtype_id: None,
                doctor_id: None,
                changed_at: Utc::now(),
            },
        );
        self.get(id).await
    }

    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        id: Uuid,
    ) -> AppResult<()> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let refs = self.repo.count_live_references(id).await?;
        if refs > 0 {
            return Err(AppError::Conflict(format!(
                "check_type is referenced by {refs} non-deleted rows (errors:catalog.referenced)"
            )));
        }
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_deleted();
        let write = UpsertCheckTypeWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "check_types",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertCheckTypeWrite {
    before: Option<CheckType>,
    after: CheckType,
    repo: Arc<dyn CheckTypeRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertCheckTypeWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(CheckTypePushPayload::from(b))?,
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(CheckTypePushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&CheckTypePushPayload::from(&self.after))?;
        let op = OutboxOp::new("check_types", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
