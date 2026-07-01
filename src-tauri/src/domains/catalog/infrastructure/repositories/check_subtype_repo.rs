//! SQLite implementation of `CheckSubtypeRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::CheckSubtype;
use crate::domains::catalog::domain::repositories::CheckSubtypeRepo;
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, parse_dt, parse_dt_opt, parse_uuid};

#[derive(Clone)]
pub struct SqliteCheckSubtypeRepo {
    pool: SqlitePool,
}

impl SqliteCheckSubtypeRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CheckSubtypeRepo for SqliteCheckSubtypeRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, sub: &CheckSubtype) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO check_subtypes (\
                id, check_type_id, name_ar, name_en, price_iqd, sort_order, \
                created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               name_ar = excluded.name_ar, name_en = excluded.name_en, \
               price_iqd = excluded.price_iqd, sort_order = excluded.sort_order, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(sub.id.to_string())
        .bind(sub.check_type_id.to_string())
        .bind(&sub.name_ar)
        .bind(sub.name_en.as_deref())
        .bind(sub.price_iqd)
        .bind(sub.sort_order)
        .bind(dt_to_str(sub.created_at))
        .bind(dt_to_str(sub.updated_at))
        .bind(dt_opt_to_str(sub.deleted_at))
        .bind(sub.version)
        .bind(sub.dirty as i64)
        .bind(dt_opt_to_str(sub.last_synced_at))
        .bind(sub.origin_device_id.as_deref())
        .bind(&sub.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<CheckSubtype>> {
        let row: Option<CheckSubtypeRow> =
            sqlx::query_as::<_, CheckSubtypeRow>("SELECT * FROM check_subtypes WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(CheckSubtypeRow::into_domain).transpose()
    }

    async fn list_by_type(&self, check_type_id: Uuid) -> AppResult<Vec<CheckSubtype>> {
        let rows: Vec<CheckSubtypeRow> = sqlx::query_as::<_, CheckSubtypeRow>(
            "SELECT * FROM check_subtypes WHERE check_type_id = ? AND deleted_at IS NULL \
             ORDER BY sort_order ASC, name_ar ASC",
        )
        .bind(check_type_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(CheckSubtypeRow::into_domain).collect()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<CheckSubtype>> {
        let rows: Vec<CheckSubtypeRow> =
            sqlx::query_as::<_, CheckSubtypeRow>("SELECT * FROM check_subtypes ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(CheckSubtypeRow::into_domain).collect()
    }
}

#[derive(sqlx::FromRow)]
struct CheckSubtypeRow {
    id: String,
    check_type_id: String,
    name_ar: String,
    name_en: Option<String>,
    price_iqd: i64,
    sort_order: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl CheckSubtypeRow {
    fn into_domain(self) -> AppResult<CheckSubtype> {
        Ok(CheckSubtype {
            id: parse_uuid(&self.id)?,
            check_type_id: parse_uuid(&self.check_type_id)?,
            name_ar: self.name_ar,
            name_en: self.name_en,
            price_iqd: self.price_iqd,
            sort_order: self.sort_order,
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
