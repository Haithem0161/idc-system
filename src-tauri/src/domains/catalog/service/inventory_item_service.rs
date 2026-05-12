//! `InventoryItemService`: superadmin-gated CRUD with consumption-reference
//! soft-delete guard (§7.8).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::inventory_item::{
    InventoryItemNewInput, InventoryItemUpdate,
};
use crate::domains::catalog::domain::entities::InventoryItem;
use crate::domains::catalog::domain::repositories::InventoryItemRepo;
use crate::domains::catalog::service::push_payloads::InventoryItemPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct InventoryItemCreateInput {
    pub name_ar: String,
    pub name_en: Option<String>,
    pub unit: String,
    #[serde(default)]
    pub low_stock_threshold: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct InventoryItemUpdateInput {
    pub name_ar: Option<String>,
    pub name_en: Option<Option<String>>,
    pub unit: Option<String>,
    pub low_stock_threshold: Option<i64>,
    pub is_active: Option<bool>,
}

#[derive(Clone)]
pub struct InventoryItemService {
    pool: sqlx::SqlitePool,
    repo: Arc<dyn InventoryItemRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl InventoryItemService {
    pub fn new(
        pool: sqlx::SqlitePool,
        repo: Arc<dyn InventoryItemRepo>,
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
    ) -> AppResult<Vec<InventoryItem>> {
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

    pub async fn get(&self, id: Uuid) -> AppResult<InventoryItem> {
        self.repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("inventory_item {id}")))
    }

    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: InventoryItemCreateInput,
    ) -> AppResult<InventoryItem> {
        Self::require_superadmin(actor_role)?;
        let item = InventoryItem::try_new(InventoryItemNewInput {
            name_ar: input.name_ar,
            name_en: input.name_en,
            unit: input.unit,
            low_stock_threshold: input.low_stock_threshold,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = item.id;
        let write = UpsertItemWrite {
            before: None,
            after: item,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "inventory_items",
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
        input: InventoryItemUpdateInput,
    ) -> AppResult<InventoryItem> {
        Self::require_superadmin(actor_role)?;
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().with_updated_fields(InventoryItemUpdate {
            name_ar: input.name_ar,
            name_en: input.name_en,
            unit: input.unit,
            low_stock_threshold: input.low_stock_threshold,
            is_active: input.is_active,
        })?;
        let write = UpsertItemWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "inventory_items",
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
        let refs = self.repo.count_live_consumption_refs(id).await?;
        if refs > 0 {
            return Err(AppError::Conflict(format!(
                "inventory item is referenced by {refs} non-deleted consumption rows"
            )));
        }
        let current = self.get(id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_deleted();
        let write = UpsertItemWrite {
            before: Some(current),
            after: updated,
            repo: self.repo.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "inventory_items",
                &id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }
}

struct UpsertItemWrite {
    before: Option<InventoryItem>,
    after: InventoryItem,
    repo: Arc<dyn InventoryItemRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertItemWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(InventoryItemPushPayload::from(b))?,
            None => Value::Null,
        })
    }
    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(InventoryItemPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&InventoryItemPushPayload::from(&self.after))?;
        let op = OutboxOp::new("inventory_items", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}
