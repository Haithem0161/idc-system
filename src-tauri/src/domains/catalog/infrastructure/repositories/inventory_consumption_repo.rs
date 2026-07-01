//! SQLite implementation of `InventoryConsumptionRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::InventoryConsumptionMap;
use crate::domains::catalog::domain::repositories::InventoryConsumptionRepo;
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, parse_dt, parse_dt_opt, parse_uuid, parse_uuid_opt};

#[derive(Clone)]
pub struct SqliteInventoryConsumptionRepo {
    pool: SqlitePool,
}

impl SqliteInventoryConsumptionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InventoryConsumptionRepo for SqliteInventoryConsumptionRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, c: &InventoryConsumptionMap) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO inventory_consumption_map (\
                id, check_type_id, check_subtype_id, item_id, quantity_per_check, on_dye_only, \
                created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               check_type_id = excluded.check_type_id, \
               check_subtype_id = excluded.check_subtype_id, \
               item_id = excluded.item_id, \
               quantity_per_check = excluded.quantity_per_check, \
               on_dye_only = excluded.on_dye_only, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(c.id.to_string())
        .bind(c.check_type_id.to_string())
        .bind(c.check_subtype_id.map(|id| id.to_string()))
        .bind(c.item_id.to_string())
        .bind(c.quantity_per_check)
        .bind(c.on_dye_only as i64)
        .bind(dt_to_str(c.created_at))
        .bind(dt_to_str(c.updated_at))
        .bind(dt_opt_to_str(c.deleted_at))
        .bind(c.version)
        .bind(c.dirty as i64)
        .bind(dt_opt_to_str(c.last_synced_at))
        .bind(c.origin_device_id.as_deref())
        .bind(&c.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<InventoryConsumptionMap>> {
        let row: Option<ConsumptionRow> = sqlx::query_as::<_, ConsumptionRow>(
            "SELECT * FROM inventory_consumption_map WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(ConsumptionRow::into_domain).transpose()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<InventoryConsumptionMap>> {
        let rows: Vec<ConsumptionRow> = sqlx::query_as::<_, ConsumptionRow>(
            "SELECT * FROM inventory_consumption_map ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ConsumptionRow::into_domain).collect()
    }

    async fn list_by_check_type(
        &self,
        check_type_id: Uuid,
    ) -> AppResult<Vec<InventoryConsumptionMap>> {
        let rows: Vec<ConsumptionRow> = sqlx::query_as::<_, ConsumptionRow>(
            "SELECT * FROM inventory_consumption_map \
             WHERE check_type_id = ? AND deleted_at IS NULL ORDER BY created_at ASC",
        )
        .bind(check_type_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ConsumptionRow::into_domain).collect()
    }

    async fn list_by_item(&self, item_id: Uuid) -> AppResult<Vec<InventoryConsumptionMap>> {
        let rows: Vec<ConsumptionRow> = sqlx::query_as::<_, ConsumptionRow>(
            "SELECT * FROM inventory_consumption_map \
             WHERE item_id = ? AND deleted_at IS NULL ORDER BY created_at ASC",
        )
        .bind(item_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ConsumptionRow::into_domain).collect()
    }

    async fn find_match(
        &self,
        check_type_id: Uuid,
        check_subtype_id: Option<Uuid>,
        item_id: Uuid,
        on_dye_only: bool,
    ) -> AppResult<Option<InventoryConsumptionMap>> {
        let row: Option<ConsumptionRow> = match check_subtype_id {
            Some(sub) => {
                sqlx::query_as::<_, ConsumptionRow>(
                    "SELECT * FROM inventory_consumption_map \
                     WHERE check_type_id = ? AND check_subtype_id = ? AND item_id = ? AND on_dye_only = ? \
                     AND deleted_at IS NULL LIMIT 1",
                )
                .bind(check_type_id.to_string())
                .bind(sub.to_string())
                .bind(item_id.to_string())
                .bind(on_dye_only as i64)
                .fetch_optional(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, ConsumptionRow>(
                    "SELECT * FROM inventory_consumption_map \
                     WHERE check_type_id = ? AND check_subtype_id IS NULL AND item_id = ? AND on_dye_only = ? \
                     AND deleted_at IS NULL LIMIT 1",
                )
                .bind(check_type_id.to_string())
                .bind(item_id.to_string())
                .bind(on_dye_only as i64)
                .fetch_optional(&self.pool)
                .await?
            }
        };
        row.map(ConsumptionRow::into_domain).transpose()
    }
}

#[derive(sqlx::FromRow)]
struct ConsumptionRow {
    id: String,
    check_type_id: String,
    check_subtype_id: Option<String>,
    item_id: String,
    quantity_per_check: i64,
    on_dye_only: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl ConsumptionRow {
    fn into_domain(self) -> AppResult<InventoryConsumptionMap> {
        Ok(InventoryConsumptionMap {
            id: parse_uuid(&self.id)?,
            check_type_id: parse_uuid(&self.check_type_id)?,
            check_subtype_id: parse_uuid_opt(self.check_subtype_id.as_deref())?,
            item_id: parse_uuid(&self.item_id)?,
            quantity_per_check: self.quantity_per_check,
            on_dye_only: self.on_dye_only != 0,
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
