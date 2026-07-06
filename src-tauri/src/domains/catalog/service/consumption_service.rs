//! `ConsumptionService`: subtype-required (§7.9) + dye-supported (§7.34)
//! cross-row invariant enforcement on upsert / accept_push.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::inventory_consumption::ConsumptionMapNewInput;
use crate::domains::catalog::domain::entities::InventoryConsumptionMap;
use crate::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, InventoryConsumptionRepo,
};
use crate::domains::catalog::service::push_payloads::ConsumptionPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct ConsumptionCreateInput {
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub item_id: Uuid,
    pub quantity_per_check: i64,
    #[serde(default)]
    pub on_dye_only: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsumptionUpdateInput {
    pub id: Uuid,
    pub quantity_per_check: i64,
    pub on_dye_only: bool,
}

#[derive(Clone)]
pub struct ConsumptionService {
    pool: sqlx::SqlitePool,
    check_type_repo: Arc<dyn CheckTypeRepo>,
    subtype_repo: Arc<dyn CheckSubtypeRepo>,
    repo: Arc<dyn InventoryConsumptionRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl ConsumptionService {
    pub fn new(
        pool: sqlx::SqlitePool,
        check_type_repo: Arc<dyn CheckTypeRepo>,
        subtype_repo: Arc<dyn CheckSubtypeRepo>,
        repo: Arc<dyn InventoryConsumptionRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            check_type_repo,
            subtype_repo,
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

    pub async fn list_by_check_type(
        &self,
        check_type_id: Uuid,
    ) -> AppResult<Vec<InventoryConsumptionMap>> {
        self.repo.list_by_check_type(check_type_id).await
    }

    pub async fn list_by_item(&self, item_id: Uuid) -> AppResult<Vec<InventoryConsumptionMap>> {
        self.repo.list_by_item(item_id).await
    }

    async fn enforce_invariants(
        &self,
        check_type_id: Uuid,
        check_subtype_id: Option<Uuid>,
        on_dye_only: bool,
    ) -> AppResult<()> {
        let ct = self
            .check_type_repo
            .get_by_id(check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {check_type_id}")))?;
        if ct.deleted_at.is_some() {
            return Err(AppError::Validation("parent check_type is deleted".into()));
        }
        match (ct.has_subtypes, check_subtype_id) {
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
        if on_dye_only {
            let dye_available = ct.dye_price_iqd.is_some()
                || self
                    .subtype_repo
                    .list_by_type(ct.id)
                    .await?
                    .iter()
                    .any(|s| s.dye_price_iqd.is_some());
            if !dye_available {
                return Err(AppError::Validation(
                    "parent check_type does not support dye (errors:consumption.dye_not_supported_on_parent)"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: ConsumptionCreateInput,
    ) -> AppResult<InventoryConsumptionMap> {
        Self::require_superadmin(actor_role)?;
        self.enforce_invariants(
            input.check_type_id,
            input.check_subtype_id,
            input.on_dye_only,
        )
        .await?;

        // Idempotent upsert by tuple (check_type_id, subtype, item_id, on_dye_only).
        if let Some(existing) = self
            .repo
            .find_match(
                input.check_type_id,
                input.check_subtype_id,
                input.item_id,
                input.on_dye_only,
            )
            .await?
        {
            let updated = existing
                .clone()
                .updated_with(input.quantity_per_check, input.on_dye_only)?;
            return self
                .commit_upsert(
                    actor_user_id,
                    entity_id,
                    AuditAction::Update,
                    updated.id,
                    updated,
                )
                .await;
        }

        let row = InventoryConsumptionMap::try_new(ConsumptionMapNewInput {
            check_type_id: input.check_type_id,
            check_subtype_id: input.check_subtype_id,
            item_id: input.item_id,
            quantity_per_check: input.quantity_per_check,
            on_dye_only: input.on_dye_only,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = row.id;
        self.commit_upsert(actor_user_id, entity_id, AuditAction::Create, id, row)
            .await
    }

    pub async fn update(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        input: ConsumptionUpdateInput,
    ) -> AppResult<InventoryConsumptionMap> {
        Self::require_superadmin(actor_role)?;
        let current =
            self.repo.get_by_id(input.id).await?.ok_or_else(|| {
                AppError::NotFound(format!("inventory_consumption_map {}", input.id))
            })?;
        self.enforce_invariants(
            current.check_type_id,
            current.check_subtype_id,
            input.on_dye_only,
        )
        .await?;
        let entity_id = current.entity_id.clone();
        let updated = current.updated_with(input.quantity_per_check, input.on_dye_only)?;
        self.commit_upsert(
            actor_user_id,
            &entity_id,
            AuditAction::Update,
            updated.id,
            updated,
        )
        .await
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
            .ok_or_else(|| AppError::NotFound(format!("inventory_consumption_map {id}")))?;
        let entity_id = current.entity_id.clone();
        let updated = current.soft_deleted();
        self.commit_upsert(
            actor_user_id,
            &entity_id,
            AuditAction::SoftDelete,
            updated.id,
            updated,
        )
        .await
        .map(|_| ())
    }

    async fn commit_upsert(
        &self,
        actor_user_id: Uuid,
        entity_id: &str,
        action: AuditAction,
        id: Uuid,
        row: InventoryConsumptionMap,
    ) -> AppResult<InventoryConsumptionMap> {
        let write = ConsumptionWrite {
            after: row.clone(),
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                action,
                "inventory_consumption_map",
                &id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;
        Ok(row)
    }
}

struct ConsumptionWrite {
    after: InventoryConsumptionMap,
    repo: Arc<dyn InventoryConsumptionRepo>,
}

#[async_trait]
impl BusinessWrite for ConsumptionWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(Value::Null)
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(ConsumptionPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&ConsumptionPushPayload::from(&self.after))?;
        let op = OutboxOp::new(
            "inventory_consumption_map",
            self.after.id.to_string(),
            payload,
        );
        Ok((after_json, vec![op]))
    }
}
