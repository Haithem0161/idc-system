//! SQLite implementation of `InventoryAdjustmentRepo`.
//!
//! Append-only at the application layer; the §7.33 trigger enforces it at
//! the storage layer. `recompute_item_quantity` runs inside the same tx as
//! the append so the new delta is visible to the SUM.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::visits::domain::entities::{AdjustmentReason, InventoryAdjustment};
use crate::domains::visits::domain::repositories::InventoryAdjustmentRepo;
use crate::error::{AppError, AppResult};

fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

fn parse_dt_opt(s: Option<&str>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(parse_dt).transpose()
}

fn parse_uuid_opt(s: Option<&str>) -> AppResult<Option<Uuid>> {
    s.map(|x| Uuid::parse_str(x).map_err(|e| AppError::Validation(format!("uuid: {e}"))))
        .transpose()
}

fn dt_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn dt_opt_str(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(|d| d.to_rfc3339())
}

#[derive(sqlx::FromRow)]
struct AdjustmentRow {
    id: String,
    item_id: String,
    delta: i64,
    reason: String,
    visit_id: Option<String>,
    note: Option<String>,
    by_user_id: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl AdjustmentRow {
    fn into_domain(self) -> AppResult<InventoryAdjustment> {
        Ok(InventoryAdjustment {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            item_id: Uuid::parse_str(&self.item_id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            delta: self.delta,
            reason: AdjustmentReason::parse(&self.reason)
                .ok_or_else(|| AppError::Validation(format!("reason: {}", self.reason)))?,
            visit_id: parse_uuid_opt(self.visit_id.as_deref())?,
            note: self.note,
            by_user_id: Uuid::parse_str(&self.by_user_id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
            deleted_at: parse_dt_opt(self.deleted_at.as_deref())?,
            version: self.version,
            dirty: self.dirty != 0,
            last_synced_at: parse_dt_opt(self.last_synced_at.as_deref())?,
            origin_device_id: self.origin_device_id,
            entity_id: self.entity_id,
        })
    }
}

const COLUMNS: &str = "id, item_id, delta, reason, visit_id, note, by_user_id, \
                       created_at, updated_at, deleted_at, version, dirty, \
                       last_synced_at, origin_device_id, entity_id";

#[derive(Clone)]
pub struct SqliteInventoryAdjustmentRepo {
    pool: SqlitePool,
}

impl SqliteInventoryAdjustmentRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InventoryAdjustmentRepo for SqliteInventoryAdjustmentRepo {
    async fn append(&self, tx: &mut Tx<'_>, a: &InventoryAdjustment) -> AppResult<()> {
        let sql = format!(
            "INSERT INTO inventory_adjustments ({COLUMNS}) VALUES \
             (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        );
        sqlx::query(&sql)
            .bind(a.id.to_string())
            .bind(a.item_id.to_string())
            .bind(a.delta)
            .bind(a.reason.as_str())
            .bind(a.visit_id.map(|u| u.to_string()))
            .bind(a.note.as_deref())
            .bind(a.by_user_id.to_string())
            .bind(dt_str(a.created_at))
            .bind(dt_str(a.updated_at))
            .bind(dt_opt_str(a.deleted_at))
            .bind(a.version)
            .bind(a.dirty as i64)
            .bind(dt_opt_str(a.last_synced_at))
            .bind(a.origin_device_id.as_deref())
            .bind(&a.entity_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    async fn list_consume_for_visit(&self, visit_id: Uuid) -> AppResult<Vec<InventoryAdjustment>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM inventory_adjustments \
             WHERE visit_id = ? AND reason = 'consume_visit' AND deleted_at IS NULL \
             ORDER BY created_at ASC, origin_device_id ASC, id ASC"
        );
        let rows = sqlx::query_as::<_, AdjustmentRow>(&sql)
            .bind(visit_id.to_string())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(AdjustmentRow::into_domain).collect()
    }

    async fn list_by_item(
        &self,
        entity_id: &str,
        item_id: Uuid,
        limit: i64,
    ) -> AppResult<Vec<InventoryAdjustment>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM inventory_adjustments \
             WHERE entity_id = ? AND item_id = ? AND deleted_at IS NULL \
             ORDER BY created_at DESC, origin_device_id ASC, id ASC LIMIT ?"
        );
        let rows = sqlx::query_as::<_, AdjustmentRow>(&sql)
            .bind(entity_id)
            .bind(item_id.to_string())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(AdjustmentRow::into_domain).collect()
    }

    async fn recompute_item_quantity(&self, tx: &mut Tx<'_>, item_id: Uuid) -> AppResult<i64> {
        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(delta), 0) FROM inventory_adjustments \
             WHERE item_id = ? AND deleted_at IS NULL",
        )
        .bind(item_id.to_string())
        .fetch_one(&mut **tx)
        .await?;
        let new_total = row.0.unwrap_or(0);
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE inventory_items SET quantity_on_hand = ?, updated_at = ?, \
             version = version + 1, dirty = 1 WHERE id = ?",
        )
        .bind(new_total)
        .bind(now)
        .bind(item_id.to_string())
        .execute(&mut **tx)
        .await?;
        Ok(new_total)
    }
}
