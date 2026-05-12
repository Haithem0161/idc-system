//! Tauri commands for the inventory operations bounded context.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::{InventoryConsumptionMap, InventoryItem};
use crate::domains::inventory::service::{
    AdjustmentInput, InventoryAdjustmentService, InventoryItemWithStatus, ItemDetail, StockStatus,
};
use crate::domains::visits::domain::entities::{AdjustmentReason, InventoryAdjustment};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// DTO for the item-with-status response.
#[derive(Debug, Serialize)]
pub struct InventoryItemDto {
    pub id: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub unit: String,
    pub quantity_on_hand: i64,
    pub low_stock_threshold: i64,
    pub is_active: bool,
    pub status: StockStatus,
    pub updated_at: String,
    pub created_at: String,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<String>,
    pub entity_id: String,
}

impl From<&InventoryItem> for InventoryItemDto {
    fn from(i: &InventoryItem) -> Self {
        Self {
            id: i.id.to_string(),
            name_ar: i.name_ar.clone(),
            name_en: i.name_en.clone(),
            unit: i.unit.clone(),
            quantity_on_hand: i.quantity_on_hand,
            low_stock_threshold: i.low_stock_threshold,
            is_active: i.is_active,
            status: StockStatus::compute(i.quantity_on_hand, i.low_stock_threshold),
            updated_at: i.updated_at.to_rfc3339(),
            created_at: i.created_at.to_rfc3339(),
            version: i.version,
            dirty: i.dirty,
            last_synced_at: i.last_synced_at.map(|t| t.to_rfc3339()),
            entity_id: i.entity_id.clone(),
        }
    }
}

impl From<&InventoryItemWithStatus> for InventoryItemDto {
    fn from(w: &InventoryItemWithStatus) -> Self {
        let mut dto = InventoryItemDto::from(&w.item);
        dto.status = w.status;
        dto
    }
}

#[derive(Debug, Serialize)]
pub struct ConsumptionMapDto {
    pub id: String,
    pub check_type_id: String,
    pub check_subtype_id: Option<String>,
    pub item_id: String,
    pub quantity_per_check: i64,
    pub on_dye_only: bool,
}

impl From<&InventoryConsumptionMap> for ConsumptionMapDto {
    fn from(m: &InventoryConsumptionMap) -> Self {
        Self {
            id: m.id.to_string(),
            check_type_id: m.check_type_id.to_string(),
            check_subtype_id: m.check_subtype_id.map(|u| u.to_string()),
            item_id: m.item_id.to_string(),
            quantity_per_check: m.quantity_per_check,
            on_dye_only: m.on_dye_only,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InventoryAdjustmentDto {
    pub id: String,
    pub item_id: String,
    pub delta: i64,
    pub reason: String,
    pub visit_id: Option<String>,
    pub note: Option<String>,
    pub by_user_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
    pub entity_id: String,
    /// True when this adjustment reverses a voided visit's consume row (a
    /// positive-delta `consume_visit` row whose `note` references another
    /// adjustment id). Phase-06 §3.Frontend uses this for the reversal
    /// badge in `<ItemAdjustmentsList>`.
    pub is_reversal: bool,
}

impl From<&InventoryAdjustment> for InventoryAdjustmentDto {
    fn from(a: &InventoryAdjustment) -> Self {
        let is_reversal = matches!(a.reason, AdjustmentReason::ConsumeVisit) && a.delta > 0;
        Self {
            id: a.id.to_string(),
            item_id: a.item_id.to_string(),
            delta: a.delta,
            reason: a.reason.as_str().into(),
            visit_id: a.visit_id.map(|u| u.to_string()),
            note: a.note.clone(),
            by_user_id: a.by_user_id.to_string(),
            created_at: a.created_at.to_rfc3339(),
            updated_at: a.updated_at.to_rfc3339(),
            version: a.version,
            entity_id: a.entity_id.clone(),
            is_reversal,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ItemDetailDto {
    pub item: InventoryItemDto,
    pub consumption_map: Vec<ConsumptionMapDto>,
    pub recent_adjustments: Vec<InventoryAdjustmentDto>,
}

impl From<&ItemDetail> for ItemDetailDto {
    fn from(d: &ItemDetail) -> Self {
        let mut item_dto = InventoryItemDto::from(&d.item);
        item_dto.status = d.status;
        Self {
            item: item_dto,
            consumption_map: d
                .consumption_map
                .iter()
                .map(ConsumptionMapDto::from)
                .collect(),
            recent_adjustments: d
                .recent_adjustments
                .iter()
                .map(InventoryAdjustmentDto::from)
                .collect(),
        }
    }
}

async fn actor(state: &AppState) -> AppResult<(Uuid, UserRole, String)> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation(format!("unknown role: {}", ctx.role)))?;
    Ok((id, role, ctx.entity_id))
}

fn service(state: &AppState) -> AppResult<Arc<InventoryAdjustmentService>> {
    state
        .inventory_adjustment_service()
        .ok_or_else(|| AppError::Configuration("inventory service unavailable".into()))
}

#[derive(Debug, Deserialize, Default)]
pub struct ListItemsArgs {
    #[serde(default)]
    pub status: Option<StockStatus>,
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub query: Option<String>,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn inventory_list_items(
    args: ListItemsArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<InventoryItemDto>> {
    let (_, _, entity_id) = actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let rows = svc
        .list_items(&entity_id, args.status, args.include_inactive, args.query)
        .await?;
    Ok(rows.iter().map(InventoryItemDto::from).collect())
}

#[derive(Debug, Deserialize)]
pub struct ItemIdArgs {
    pub id: String,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn inventory_get_item(
    args: ItemIdArgs,
    state: State<'_, AppState>,
) -> AppResult<ItemDetailDto> {
    let (_, _, entity_id) = actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let id = Uuid::parse_str(&args.id)?;
    let d = svc.get_item(&entity_id, id).await?;
    Ok(ItemDetailDto::from(&d))
}

#[derive(Debug, Deserialize)]
pub struct ListAdjustmentsArgs {
    pub item_id: String,
    #[serde(default = "default_adjustment_limit")]
    pub limit: i64,
}

fn default_adjustment_limit() -> i64 {
    50
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn inventory_list_adjustments(
    args: ListAdjustmentsArgs,
    state: State<'_, AppState>,
) -> AppResult<Vec<InventoryAdjustmentDto>> {
    let (_, _, entity_id) = actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let id = Uuid::parse_str(&args.item_id)?;
    let rows = svc.list_adjustments(&entity_id, id, args.limit).await?;
    Ok(rows.iter().map(InventoryAdjustmentDto::from).collect())
}

#[derive(Debug, Deserialize)]
pub struct CreateAdjustmentArgs {
    pub item_id: String,
    pub reason: AdjustmentReason,
    pub delta: i64,
    #[serde(default)]
    pub note: Option<String>,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn inventory_create_adjustment(
    args: CreateAdjustmentArgs,
    state: State<'_, AppState>,
) -> AppResult<InventoryAdjustmentDto> {
    let (user_id, role, entity_id) = actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let id = Uuid::parse_str(&args.item_id)?;
    let adj = svc
        .create(
            user_id,
            role,
            &entity_id,
            AdjustmentInput {
                item_id: id,
                reason: args.reason,
                delta: args.delta,
                note: args.note,
            },
        )
        .await?;
    Ok(InventoryAdjustmentDto::from(&adj))
}

#[derive(Debug, Deserialize)]
pub struct RecomputeArgs {
    pub item_id: String,
}

#[derive(Debug, Serialize)]
pub struct RecomputeResult {
    pub new_on_hand: i64,
}

#[instrument(skip(state))]
#[tauri::command]
pub async fn inventory_recompute_on_hand(
    args: RecomputeArgs,
    state: State<'_, AppState>,
) -> AppResult<RecomputeResult> {
    let (user_id, role, entity_id) = actor(state.inner()).await?;
    let svc = service(state.inner())?;
    let id = Uuid::parse_str(&args.item_id)?;
    let n = svc.recompute_on_hand(user_id, role, &entity_id, id).await?;
    Ok(RecomputeResult { new_on_hand: n })
}
