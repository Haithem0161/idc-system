//! `InventoryAdjustmentService`: operational workflows over
//! `inventory_adjustments` per Phase 6 §4.
//!
//! Every mutator routes through `AuditWriter::with_audit` so the audit row is
//! attached to the same SQLite tx as the business write (PRD §4.3). A single
//! `with_audit` call covers the primary `inventory_adjustments` create; a
//! second `audit_log` row for the corresponding `inventory_items` update is
//! written inline inside the closure so both audit rows commit atomically
//! with the adjustment and the recomputed on-hand (phase-06 §7.11).

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::{InventoryConsumptionMap, InventoryItem};
use crate::domains::catalog::domain::repositories::{
    CatalogListFilter, InventoryConsumptionRepo, InventoryItemRepo,
};
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::domains::visits::domain::entities::{AdjustmentReason, InventoryAdjustment};
use crate::domains::visits::domain::repositories::InventoryAdjustmentRepo;
use crate::domains::visits::service::push_payloads::InventoryAdjustmentPushPayload;
use crate::error::{AppError, AppResult};

/// Stock status pill states surfaced by the inventory list (PRD §7.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockStatus {
    Ok,
    Low,
    Neg,
}

impl StockStatus {
    pub fn compute(quantity_on_hand: i64, low_stock_threshold: i64) -> Self {
        if quantity_on_hand < 0 {
            Self::Neg
        } else if quantity_on_hand <= low_stock_threshold {
            Self::Low
        } else {
            Self::Ok
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryItemWithStatus {
    pub item: InventoryItem,
    pub status: StockStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemDetail {
    pub item: InventoryItem,
    pub status: StockStatus,
    pub consumption_map: Vec<InventoryConsumptionMap>,
    pub recent_adjustments: Vec<InventoryAdjustment>,
}

/// Input for `inventory::create_adjustment` IPC.
#[derive(Debug, Clone, Deserialize)]
pub struct AdjustmentInput {
    pub item_id: Uuid,
    pub reason: AdjustmentReason,
    /// Raw delta value as supplied by the UI. For `writeoff` the UI submits a
    /// positive quantity which the service negates; for `receive` a positive
    /// integer; for `count_correction` a signed non-zero integer.
    pub delta: i64,
    pub note: Option<String>,
}

#[derive(Clone)]
pub struct InventoryAdjustmentServiceConfig {
    pub pool: sqlx::SqlitePool,
    pub items_repo: Arc<dyn InventoryItemRepo>,
    pub consumption_repo: Arc<dyn InventoryConsumptionRepo>,
    pub adjustments_repo: Arc<dyn InventoryAdjustmentRepo>,
    pub audit_repo: Arc<dyn AuditRepo>,
    pub outbox_repo: Arc<dyn OutboxRepo>,
    pub device_id: String,
}

#[derive(Clone)]
pub struct InventoryAdjustmentService {
    pool: sqlx::SqlitePool,
    items: Arc<dyn InventoryItemRepo>,
    consumption: Arc<dyn InventoryConsumptionRepo>,
    adjustments: Arc<dyn InventoryAdjustmentRepo>,
    audit: Arc<dyn AuditRepo>,
    outbox: Arc<dyn OutboxRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl InventoryAdjustmentService {
    pub fn new(cfg: InventoryAdjustmentServiceConfig) -> Self {
        let writer = AuditWriter::new(
            cfg.audit_repo.clone(),
            cfg.outbox_repo.clone(),
            cfg.device_id.clone(),
        );
        Self {
            pool: cfg.pool,
            items: cfg.items_repo,
            consumption: cfg.consumption_repo,
            adjustments: cfg.adjustments_repo,
            audit: cfg.audit_repo,
            outbox: cfg.outbox_repo,
            writer,
            device_id: cfg.device_id,
        }
    }

    fn entity_id_tenant(entity_id: &str) -> &str {
        entity_id
    }

    /// List items with computed status. `status_filter` is applied in-memory
    /// after the catalog repo returns the live rows; partial indexes added in
    /// `migrations/006_inventory_ops.sql` keep the underlying scan fast.
    #[instrument(skip(self))]
    pub async fn list_items(
        &self,
        entity_id: &str,
        status_filter: Option<StockStatus>,
        include_inactive: bool,
        query: Option<String>,
    ) -> AppResult<Vec<InventoryItemWithStatus>> {
        let filter = CatalogListFilter {
            entity_id: entity_id.into(),
            include_deleted: false,
            include_inactive,
            query: query.and_then(|q| {
                let trimmed = q.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
        };
        let rows = self.items.list(filter).await?;
        let mut out: Vec<InventoryItemWithStatus> = rows
            .into_iter()
            .map(|item| {
                let status = StockStatus::compute(item.quantity_on_hand, item.low_stock_threshold);
                InventoryItemWithStatus { item, status }
            })
            .collect();
        if let Some(s) = status_filter {
            out.retain(|r| r.status == s);
        }
        Ok(out)
    }

    /// Detailed view: item + consumption map + last 20 adjustments.
    #[instrument(skip(self))]
    pub async fn get_item(&self, entity_id: &str, item_id: Uuid) -> AppResult<ItemDetail> {
        let item = self
            .items
            .get_by_id(item_id)
            .await?
            .ok_or_else(|| AppError::NotFound("inventory item".into()))?;
        if item.entity_id != entity_id {
            return Err(AppError::NotFound("inventory item".into()));
        }
        let status = StockStatus::compute(item.quantity_on_hand, item.low_stock_threshold);
        let consumption_map = self.consumption.list_by_item(item_id).await?;
        let recent_adjustments = self
            .adjustments
            .list_by_item(entity_id, item_id, 50)
            .await?;
        Ok(ItemDetail {
            item,
            status,
            consumption_map,
            recent_adjustments,
        })
    }

    /// Paginated adjustments list for an item. `limit` is clamped to `[1,200]`.
    #[instrument(skip(self))]
    pub async fn list_adjustments(
        &self,
        entity_id: &str,
        item_id: Uuid,
        limit: i64,
    ) -> AppResult<Vec<InventoryAdjustment>> {
        let limit = limit.clamp(1, 200);
        self.adjustments
            .list_by_item(entity_id, item_id, limit)
            .await
    }

    /// Create one adjustment. Role gates per reason; superadmin-only for
    /// `count_correction` and `consume_visit` is rejected here (only the lock
    /// workflow emits those). Writes audit-first + recomputes on-hand inside
    /// the same SQLite tx.
    #[instrument(skip(self, input))]
    pub async fn create(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: AdjustmentInput,
    ) -> AppResult<InventoryAdjustment> {
        // Phase-06 §4 permission gates.
        match input.reason {
            AdjustmentReason::Receive | AdjustmentReason::Writeoff => {
                Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
            }
            AdjustmentReason::CountCorrection => {
                Self::require_role(actor_role, &[UserRole::Superadmin])?;
            }
            AdjustmentReason::ConsumeVisit => {
                return Err(AppError::Validation(
                    "consume_visit adjustments are only emitted by the visit lock workflow".into(),
                ));
            }
        }

        // Verify the item exists and belongs to this tenant.
        let item = self
            .items
            .get_by_id(input.item_id)
            .await?
            .ok_or_else(|| AppError::NotFound("inventory item".into()))?;
        if item.entity_id != entity_id {
            return Err(AppError::NotFound("inventory item".into()));
        }
        if item.deleted_at.is_some() {
            return Err(AppError::Validation(
                "cannot adjust a deleted inventory item".into(),
            ));
        }

        // Build the adjustment via the per-reason constructors so the entity
        // invariants (sign rules, non-zero for count_correction, note length)
        // fail fast before opening a tx.
        let note = input.note.as_ref().and_then(|n| {
            let trimmed = n.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        let adjustment = Self::try_build_adjustment_from_input(
            input.reason,
            input.item_id,
            input.delta,
            actor_user_id,
            note,
            entity_id.to_string(),
            self.device_id.clone(),
        )?;

        let write = CreateAdjustmentWrite {
            adjustment: adjustment.clone(),
            adjustments_repo: self.adjustments.clone(),
            audit_repo: self.audit.clone(),
            item_id: input.item_id,
            entity_id_tenant: Self::entity_id_tenant(entity_id).to_string(),
            actor_user_id,
            device_id: self.device_id.clone(),
            item_before: item,
        };

        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "inventory_adjustments",
                &adjustment.id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;

        // Hand back the persisted row (with version/dirty stamped).
        Ok(adjustment)
    }

    /// Superadmin-only debug command: recompute `quantity_on_hand` for one
    /// item from the SUM of its non-deleted adjustments. Writes an audit row
    /// reflecting before/after.
    #[instrument(skip(self))]
    pub async fn recompute_on_hand(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        item_id: Uuid,
    ) -> AppResult<i64> {
        Self::require_role(actor_role, &[UserRole::Superadmin])?;
        let item = self
            .items
            .get_by_id(item_id)
            .await?
            .ok_or_else(|| AppError::NotFound("inventory item".into()))?;
        if item.entity_id != entity_id {
            return Err(AppError::NotFound("inventory item".into()));
        }
        let before = item.quantity_on_hand;
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let after = self
            .adjustments
            .recompute_item_quantity(&mut tx, item_id)
            .await?;
        // Audit row inside the same tx so a failed commit rolls it back.
        let delta = serde_json::json!({
            "quantity_on_hand": { "before": before, "after": after },
            "reason": "recompute",
        });
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id,
            action: AuditAction::Update,
            entity: "inventory_items".into(),
            entity_id: item_id.to_string(),
            delta,
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.to_string(),
        });
        self.audit.append(&mut tx, &audit).await?;
        let audit_payload = rmp_serde::to_vec_named(&audit)?;
        let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox.enqueue(&mut tx, &audit_outbox).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(after)
    }

    fn try_build_adjustment_from_input(
        reason: AdjustmentReason,
        item_id: Uuid,
        delta: i64,
        actor_user_id: Uuid,
        note: Option<String>,
        entity_id: String,
        device_id: String,
    ) -> AppResult<InventoryAdjustment> {
        match reason {
            AdjustmentReason::Receive => InventoryAdjustment::try_receive(
                item_id,
                delta,
                actor_user_id,
                note,
                entity_id,
                Some(device_id),
            ),
            AdjustmentReason::Writeoff => InventoryAdjustment::try_writeoff(
                item_id,
                delta,
                actor_user_id,
                note,
                entity_id,
                Some(device_id),
            ),
            AdjustmentReason::CountCorrection => InventoryAdjustment::try_count_correction(
                item_id,
                delta,
                actor_user_id,
                note,
                entity_id,
                Some(device_id),
            ),
            AdjustmentReason::ConsumeVisit => Err(AppError::Internal(
                "ConsumeVisit reached construction switch after early-return guard".into(),
            )),
        }
    }

    fn require_role(role: UserRole, allowed: &[UserRole]) -> AppResult<()> {
        if allowed.contains(&role) {
            Ok(())
        } else {
            Err(AppError::Validation(format!(
                "this action requires one of: {:?}",
                allowed
            )))
        }
    }
}

/// `BusinessWrite` implementation for the create-adjustment workflow.
///
/// Step ordering inside the closure (phase-06 §7.11):
/// 1. `with_audit` has not yet written its primary audit row; that happens
///    AFTER this closure returns so it can compute the delta from
///    `(before, after)`.
/// 2. Write the second audit row for the `inventory_items` update (before
///    the item is recomputed so the audit row's `before/after` reflect the
///    deterministic change).
/// 3. Append the adjustment row.
/// 4. Recompute `inventory_items.quantity_on_hand` (bumps version + dirty).
/// 5. Return business outbox rows for both the adjustment AND the item.
///
/// Both audit rows are inserted before commit, satisfying §7.11 ("audit rows
/// are always written first; on failure of any subsequent step the entire tx
/// rolls back, leaving NO audit row").
struct CreateAdjustmentWrite {
    adjustment: InventoryAdjustment,
    adjustments_repo: Arc<dyn InventoryAdjustmentRepo>,
    audit_repo: Arc<dyn AuditRepo>,
    item_id: Uuid,
    entity_id_tenant: String,
    actor_user_id: Uuid,
    device_id: String,
    /// Pre-loaded snapshot of the inventory item BEFORE the adjustment. The
    /// item-update outbox payload is reconstructed from this snapshot with
    /// the new `quantity_on_hand` and the bumped `version` so the closure
    /// never needs a second pool connection while the tx is held.
    item_before: InventoryItem,
}

#[async_trait]
impl BusinessWrite for CreateAdjustmentWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        // The "before" snapshot of the adjustment is Null (new row).
        Ok(Value::Null)
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        // 1. Append the adjustment row (the per-reason CHECK + count_correction
        //    trigger enforce sign invariants at the DB layer).
        self.adjustments_repo.append(tx, &self.adjustment).await?;

        // 2. Recompute item.quantity_on_hand inside the tx. The repo bumps
        //    version + dirty so the sync engine picks up the item row in the
        //    same outbox cycle.
        let new_total = self
            .adjustments_repo
            .recompute_item_quantity(tx, self.item_id)
            .await?;

        // 3. Inline a second audit row for the `inventory_items` update so
        //    both audit rows commit atomically with the adjustment + item
        //    recompute (phase-06 §7.11).
        let item_delta = serde_json::json!({
            "quantity_on_hand": {
                "before": self.item_before.quantity_on_hand,
                "after": new_total,
            },
            "reason": self.adjustment.reason.as_str(),
            "adjustment_id": self.adjustment.id.to_string(),
            "adjustment_delta": self.adjustment.delta,
        });
        let item_audit = AuditEntry::create(AuditCreateInput {
            actor_user_id: self.actor_user_id,
            action: AuditAction::Update,
            entity: "inventory_items".into(),
            entity_id: self.item_id.to_string(),
            delta: item_delta,
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: self.entity_id_tenant.clone(),
        });
        self.audit_repo.append(tx, &item_audit).await?;

        // 4. Build outbox rows. The adjustment uses the JSON push payload;
        //    the item push payload is rebuilt from the pre-loaded snapshot
        //    with the updated `quantity_on_hand` + bumped `version`. The
        //    repo's recompute statement bumps `version` by one, so we
        //    mirror that here -- a future schema change that bumps version
        //    differently would require a code fix in both places.
        let mut item_after = self.item_before.clone();
        item_after.quantity_on_hand = new_total;
        item_after.version = self.item_before.version + 1;
        item_after.dirty = true;
        item_after.updated_at = chrono::Utc::now();

        let mut outbox: Vec<OutboxOp> = Vec::new();
        let adj_payload =
            serde_json::to_vec(&InventoryAdjustmentPushPayload::from(&self.adjustment))?;
        outbox.push(OutboxOp::new(
            "inventory_adjustments",
            self.adjustment.id.to_string(),
            adj_payload,
        ));
        let item_payload = serde_json::to_vec(
            &crate::domains::catalog::service::push_payloads::InventoryItemPushPayload::from(
                &item_after,
            ),
        )?;
        outbox.push(OutboxOp::new(
            "inventory_items",
            self.item_id.to_string(),
            item_payload,
        ));

        // Also enqueue the inline item audit row so its outbox push runs in
        // this cycle. The primary `with_audit` row is enqueued by the
        // writer automatically; we only need the inline one here.
        let item_audit_payload = rmp_serde::to_vec_named(&item_audit)?;
        outbox.push(OutboxOp::new(
            "audit_log",
            item_audit.id.to_string(),
            item_audit_payload,
        ));

        // After payload: the persisted adjustment snapshot. `with_audit`
        // computes the `delta` of the primary audit row from
        // `(before=Null, after=this)`, so we hand back the adjustment shape.
        let after = serde_json::to_value(InventoryAdjustmentPushPayload::from(&self.adjustment))?;
        Ok((after, outbox))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_visit_in_construct_switch_returns_internal_error_not_panic() {
        let result = InventoryAdjustmentService::try_build_adjustment_from_input(
            AdjustmentReason::ConsumeVisit,
            Uuid::now_v7(),
            -1,
            Uuid::now_v7(),
            None,
            "tenant-1".to_string(),
            "device-1".to_string(),
        );
        match result {
            Err(AppError::Internal(msg)) => assert_eq!(
                msg,
                "ConsumeVisit reached construction switch after early-return guard",
            ),
            other => {
                panic!("expected AppError::Internal with the documented message, got {other:?}")
            }
        }
    }

    #[test]
    fn receive_in_construct_switch_returns_ok_with_positive_delta() {
        let result = InventoryAdjustmentService::try_build_adjustment_from_input(
            AdjustmentReason::Receive,
            Uuid::now_v7(),
            5,
            Uuid::now_v7(),
            None,
            "tenant-1".to_string(),
            "device-1".to_string(),
        );
        let adj = result.expect("receive with positive delta must succeed");
        assert_eq!(adj.delta, 5);
        assert!(matches!(adj.reason, AdjustmentReason::Receive));
    }

    #[test]
    fn writeoff_in_construct_switch_negates_positive_quantity() {
        let result = InventoryAdjustmentService::try_build_adjustment_from_input(
            AdjustmentReason::Writeoff,
            Uuid::now_v7(),
            3,
            Uuid::now_v7(),
            None,
            "tenant-1".to_string(),
            "device-1".to_string(),
        );
        let adj = result.expect("writeoff with positive ui quantity must succeed");
        assert_eq!(adj.delta, -3);
        assert!(matches!(adj.reason, AdjustmentReason::Writeoff));
    }

    #[test]
    fn count_correction_in_construct_switch_preserves_signed_delta() {
        let result = InventoryAdjustmentService::try_build_adjustment_from_input(
            AdjustmentReason::CountCorrection,
            Uuid::now_v7(),
            -7,
            Uuid::now_v7(),
            None,
            "tenant-1".to_string(),
            "device-1".to_string(),
        );
        let adj = result.expect("count_correction with signed delta must succeed");
        assert_eq!(adj.delta, -7);
        assert!(matches!(adj.reason, AdjustmentReason::CountCorrection));
    }
}
