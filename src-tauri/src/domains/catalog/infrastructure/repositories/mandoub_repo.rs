//! SQLite implementation of `MandoubRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::Mandoub;
use crate::domains::catalog::domain::repositories::{CatalogListFilter, MandoubRepo};
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, like_prefix, parse_dt, parse_dt_opt, parse_uuid};

#[derive(Clone)]
pub struct SqliteMandoubRepo {
    pool: SqlitePool,
}

impl SqliteMandoubRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MandoubRepo for SqliteMandoubRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, m: &Mandoub) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO mandoubs (\
                id, name, phone, is_active, notes, \
                created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               name = excluded.name, phone = excluded.phone, \
               is_active = excluded.is_active, notes = excluded.notes, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(m.id.to_string())
        .bind(&m.name)
        .bind(m.phone.as_deref())
        .bind(m.is_active as i64)
        .bind(m.notes.as_deref())
        .bind(dt_to_str(m.created_at))
        .bind(dt_to_str(m.updated_at))
        .bind(dt_opt_to_str(m.deleted_at))
        .bind(m.version)
        .bind(m.dirty as i64)
        .bind(dt_opt_to_str(m.last_synced_at))
        .bind(m.origin_device_id.as_deref())
        .bind(&m.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Mandoub>> {
        let row: Option<MandoubRow> =
            sqlx::query_as::<_, MandoubRow>("SELECT * FROM mandoubs WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(MandoubRow::into_domain).transpose()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<Mandoub>> {
        let rows: Vec<MandoubRow> =
            sqlx::query_as::<_, MandoubRow>("SELECT * FROM mandoubs ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(MandoubRow::into_domain).collect()
    }

    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<Mandoub>> {
        let mut sql = String::from("SELECT * FROM mandoubs WHERE entity_id = ?");
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
            sql.push_str(" AND name LIKE ? ESCAPE '\\'");
        }
        sql.push_str(" ORDER BY name ASC");

        let mut q = sqlx::query_as::<_, MandoubRow>(&sql).bind(&filter.entity_id);
        if let Some(p) = like.as_ref() {
            q = q.bind(p);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(MandoubRow::into_domain).collect()
    }
}

#[derive(sqlx::FromRow)]
struct MandoubRow {
    id: String,
    name: String,
    phone: Option<String>,
    is_active: i64,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl MandoubRow {
    fn into_domain(self) -> AppResult<Mandoub> {
        Ok(Mandoub {
            id: parse_uuid(&self.id)?,
            name: self.name,
            phone: self.phone,
            is_active: self.is_active != 0,
            notes: self.notes,
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
