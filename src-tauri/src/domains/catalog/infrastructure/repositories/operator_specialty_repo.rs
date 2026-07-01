//! SQLite implementation of `OperatorSpecialtyRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::OperatorSpecialty;
use crate::domains::catalog::domain::repositories::OperatorSpecialtyRepo;
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, parse_dt, parse_dt_opt, parse_uuid};

#[derive(Clone)]
pub struct SqliteOperatorSpecialtyRepo {
    pool: SqlitePool,
}

impl SqliteOperatorSpecialtyRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OperatorSpecialtyRepo for SqliteOperatorSpecialtyRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, sp: &OperatorSpecialty) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO operator_specialties (\
                id, operator_id, check_type_id, created_at, updated_at, deleted_at, \
                version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               operator_id = excluded.operator_id, check_type_id = excluded.check_type_id, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(sp.id.to_string())
        .bind(sp.operator_id.to_string())
        .bind(sp.check_type_id.to_string())
        .bind(dt_to_str(sp.created_at))
        .bind(dt_to_str(sp.updated_at))
        .bind(dt_opt_to_str(sp.deleted_at))
        .bind(sp.version)
        .bind(sp.dirty as i64)
        .bind(dt_opt_to_str(sp.last_synced_at))
        .bind(sp.origin_device_id.as_deref())
        .bind(&sp.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<OperatorSpecialty>> {
        let row: Option<OperatorSpecialtyRow> = sqlx::query_as::<_, OperatorSpecialtyRow>(
            "SELECT * FROM operator_specialties WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(OperatorSpecialtyRow::into_domain).transpose()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<OperatorSpecialty>> {
        let rows: Vec<OperatorSpecialtyRow> = sqlx::query_as::<_, OperatorSpecialtyRow>(
            "SELECT * FROM operator_specialties ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(OperatorSpecialtyRow::into_domain)
            .collect()
    }

    async fn list_by_operator(&self, operator_id: Uuid) -> AppResult<Vec<OperatorSpecialty>> {
        let rows: Vec<OperatorSpecialtyRow> = sqlx::query_as::<_, OperatorSpecialtyRow>(
            "SELECT * FROM operator_specialties WHERE operator_id = ? AND deleted_at IS NULL \
             ORDER BY created_at ASC",
        )
        .bind(operator_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(OperatorSpecialtyRow::into_domain)
            .collect()
    }

    async fn find_match(
        &self,
        operator_id: Uuid,
        check_type_id: Uuid,
    ) -> AppResult<Option<OperatorSpecialty>> {
        let row: Option<OperatorSpecialtyRow> = sqlx::query_as::<_, OperatorSpecialtyRow>(
            "SELECT * FROM operator_specialties \
             WHERE operator_id = ? AND check_type_id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(operator_id.to_string())
        .bind(check_type_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(OperatorSpecialtyRow::into_domain).transpose()
    }
}

#[derive(sqlx::FromRow)]
struct OperatorSpecialtyRow {
    id: String,
    operator_id: String,
    check_type_id: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl OperatorSpecialtyRow {
    fn into_domain(self) -> AppResult<OperatorSpecialty> {
        Ok(OperatorSpecialty {
            id: parse_uuid(&self.id)?,
            operator_id: parse_uuid(&self.operator_id)?,
            check_type_id: parse_uuid(&self.check_type_id)?,
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
