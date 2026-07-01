//! SQLite implementation of `InventoryItemRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::InventoryItem;
use crate::domains::catalog::domain::repositories::{CatalogListFilter, InventoryItemRepo};
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, like_prefix, parse_dt, parse_dt_opt, parse_uuid};

#[derive(Clone)]
pub struct SqliteInventoryItemRepo {
    pool: SqlitePool,
}

impl SqliteInventoryItemRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InventoryItemRepo for SqliteInventoryItemRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, item: &InventoryItem) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO inventory_items (\
                id, name_ar, name_en, unit, quantity_on_hand, low_stock_threshold, is_active, \
                created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               name_ar = excluded.name_ar, name_en = excluded.name_en, unit = excluded.unit, \
               quantity_on_hand = excluded.quantity_on_hand, \
               low_stock_threshold = excluded.low_stock_threshold, \
               is_active = excluded.is_active, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(item.id.to_string())
        .bind(&item.name_ar)
        .bind(item.name_en.as_deref())
        .bind(&item.unit)
        .bind(item.quantity_on_hand)
        .bind(item.low_stock_threshold)
        .bind(item.is_active as i64)
        .bind(dt_to_str(item.created_at))
        .bind(dt_to_str(item.updated_at))
        .bind(dt_opt_to_str(item.deleted_at))
        .bind(item.version)
        .bind(item.dirty as i64)
        .bind(dt_opt_to_str(item.last_synced_at))
        .bind(item.origin_device_id.as_deref())
        .bind(&item.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<InventoryItem>> {
        let row: Option<InventoryItemRow> =
            sqlx::query_as::<_, InventoryItemRow>("SELECT * FROM inventory_items WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(InventoryItemRow::into_domain).transpose()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<InventoryItem>> {
        let rows: Vec<InventoryItemRow> =
            sqlx::query_as::<_, InventoryItemRow>("SELECT * FROM inventory_items ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(InventoryItemRow::into_domain)
            .collect()
    }

    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<InventoryItem>> {
        let mut sql = String::from("SELECT * FROM inventory_items WHERE entity_id = ?");
        if !filter.include_deleted {
            sql.push_str(" AND deleted_at IS NULL");
        }
        if !filter.include_inactive {
            sql.push_str(" AND is_active = 1");
        }
        let like = filter
            .query
            .as_ref()
            .filter(|q| q.trim().chars().count() >= 2)
            .map(|q| like_prefix(q.trim()));
        if like.is_some() {
            sql.push_str(" AND (name_ar LIKE ? ESCAPE '\\' OR name_en LIKE ? ESCAPE '\\')");
        }
        sql.push_str(" ORDER BY name_ar ASC");

        let mut q = sqlx::query_as::<_, InventoryItemRow>(&sql).bind(&filter.entity_id);
        if let Some(p) = like.as_ref() {
            q = q.bind(p).bind(p);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(InventoryItemRow::into_domain)
            .collect()
    }

    async fn count_live_consumption_refs(&self, item_id: Uuid) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM inventory_consumption_map \
             WHERE item_id = ? AND deleted_at IS NULL",
        )
        .bind(item_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }
}

#[derive(sqlx::FromRow)]
struct InventoryItemRow {
    id: String,
    name_ar: String,
    name_en: Option<String>,
    unit: String,
    quantity_on_hand: i64,
    low_stock_threshold: i64,
    is_active: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl InventoryItemRow {
    fn into_domain(self) -> AppResult<InventoryItem> {
        Ok(InventoryItem {
            id: parse_uuid(&self.id)?,
            name_ar: self.name_ar,
            name_en: self.name_en,
            unit: self.unit,
            quantity_on_hand: self.quantity_on_hand,
            low_stock_threshold: self.low_stock_threshold,
            is_active: self.is_active != 0,
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
