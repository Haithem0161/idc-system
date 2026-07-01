//! SQLite implementation of `CheckTypeRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::CheckType;
use crate::domains::catalog::domain::repositories::{CatalogListFilter, CheckTypeRepo};
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, like_prefix, parse_dt, parse_dt_opt, parse_uuid};

#[derive(Clone)]
pub struct SqliteCheckTypeRepo {
    pool: SqlitePool,
}

impl SqliteCheckTypeRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CheckTypeRepo for SqliteCheckTypeRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, ct: &CheckType) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO check_types (\
                id, name_ar, name_en, has_subtypes, base_price_iqd, dye_supported, \
                sort_order, is_active, created_at, updated_at, deleted_at, \
                version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               name_ar = excluded.name_ar, name_en = excluded.name_en, \
               has_subtypes = excluded.has_subtypes, base_price_iqd = excluded.base_price_iqd, \
               dye_supported = excluded.dye_supported, \
               sort_order = excluded.sort_order, is_active = excluded.is_active, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(ct.id.to_string())
        .bind(&ct.name_ar)
        .bind(ct.name_en.as_deref())
        .bind(ct.has_subtypes as i64)
        .bind(ct.base_price_iqd)
        .bind(ct.dye_supported as i64)
        .bind(ct.sort_order)
        .bind(ct.is_active as i64)
        .bind(dt_to_str(ct.created_at))
        .bind(dt_to_str(ct.updated_at))
        .bind(dt_opt_to_str(ct.deleted_at))
        .bind(ct.version)
        .bind(ct.dirty as i64)
        .bind(dt_opt_to_str(ct.last_synced_at))
        .bind(ct.origin_device_id.as_deref())
        .bind(&ct.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<CheckType>> {
        let row: Option<CheckTypeRow> =
            sqlx::query_as::<_, CheckTypeRow>("SELECT * FROM check_types WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(CheckTypeRow::into_domain).transpose()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<CheckType>> {
        let rows: Vec<CheckTypeRow> =
            sqlx::query_as::<_, CheckTypeRow>("SELECT * FROM check_types ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(CheckTypeRow::into_domain).collect()
    }

    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<CheckType>> {
        let mut sql = String::from("SELECT * FROM check_types WHERE entity_id = ?");
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
        sql.push_str(" ORDER BY sort_order ASC, name_ar ASC");

        let mut q = sqlx::query_as::<_, CheckTypeRow>(&sql).bind(&filter.entity_id);
        if let Some(p) = like.as_ref() {
            q = q.bind(p).bind(p);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(CheckTypeRow::into_domain).collect()
    }

    async fn count_live_subtypes(&self, check_type_id: Uuid) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM check_subtypes WHERE check_type_id = ? AND deleted_at IS NULL",
        )
        .bind(check_type_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    async fn count_live_references(&self, check_type_id: Uuid) -> AppResult<i64> {
        let id = check_type_id.to_string();
        let (subtypes,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM check_subtypes WHERE check_type_id = ? AND deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;
        let (pricings,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM doctor_check_pricing WHERE check_type_id = ? AND deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;
        let (specialties,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM operator_specialties WHERE check_type_id = ? AND deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;
        let (consumption,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM inventory_consumption_map WHERE check_type_id = ? AND deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;
        Ok(subtypes + pricings + specialties + consumption)
    }
}

#[derive(sqlx::FromRow)]
struct CheckTypeRow {
    id: String,
    name_ar: String,
    name_en: Option<String>,
    has_subtypes: i64,
    base_price_iqd: Option<i64>,
    dye_supported: i64,
    sort_order: i64,
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

impl CheckTypeRow {
    fn into_domain(self) -> AppResult<CheckType> {
        Ok(CheckType {
            id: parse_uuid(&self.id)?,
            name_ar: self.name_ar,
            name_en: self.name_en,
            has_subtypes: self.has_subtypes != 0,
            base_price_iqd: self.base_price_iqd,
            dye_supported: self.dye_supported != 0,
            sort_order: self.sort_order,
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
