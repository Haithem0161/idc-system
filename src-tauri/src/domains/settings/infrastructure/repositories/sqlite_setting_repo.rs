//! sqlx-backed implementation of `SettingRepo`.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;

use crate::db::Tx;
use crate::domains::settings::domain::entities::Setting;
use crate::domains::settings::domain::repositories::SettingRepo;
use crate::domains::settings::domain::value_objects::SettingValue;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct SqliteSettingRepo {
    pool: SqlitePool,
}

impl SqliteSettingRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SettingRepo for SqliteSettingRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO settings (\
                id, key, value, value_type, created_at, updated_at, deleted_at, \
                version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(entity_id, key) WHERE deleted_at IS NULL DO UPDATE SET \
               value = excluded.value, \
               value_type = excluded.value_type, \
               updated_at = excluded.updated_at, \
               deleted_at = excluded.deleted_at, \
               version = excluded.version, \
               dirty = excluded.dirty",
        )
        .bind(setting.id.to_string())
        .bind(&setting.key)
        .bind(setting.value.as_storage())
        .bind(setting.value.value_type())
        .bind(setting.created_at.to_rfc3339())
        .bind(setting.updated_at.to_rfc3339())
        .bind(setting.deleted_at.map(|d| d.to_rfc3339()))
        .bind(setting.version)
        .bind(setting.dirty as i64)
        .bind(setting.last_synced_at.map(|d| d.to_rfc3339()))
        .bind(setting.origin_device_id.as_deref())
        .bind(&setting.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_key(&self, key: &str, entity_id: &str) -> AppResult<Option<Setting>> {
        // Deterministic even if a duplicate ever slips past the reconcile:
        // newest row wins, tie-broken by id, capped to one.
        let row: Option<SettingRow> = sqlx::query_as::<_, SettingRow>(
            "SELECT * FROM settings \
             WHERE key = ? AND entity_id = ? AND deleted_at IS NULL \
             ORDER BY updated_at DESC, id DESC \
             LIMIT 1",
        )
        .bind(key)
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(SettingRow::into_domain).transpose()
    }

    async fn list(&self, entity_id: &str) -> AppResult<Vec<Setting>> {
        let rows: Vec<SettingRow> = sqlx::query_as::<_, SettingRow>(
            "SELECT * FROM settings WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY key ASC, updated_at DESC, id DESC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SettingRow::into_domain).collect()
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<Setting>> {
        let rows: Vec<SettingRow> =
            sqlx::query_as::<_, SettingRow>("SELECT * FROM settings ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(SettingRow::into_domain).collect()
    }

    async fn list_live_by_entity(&self, entity_id: &str) -> AppResult<Vec<Setting>> {
        let rows: Vec<SettingRow> = sqlx::query_as::<_, SettingRow>(
            "SELECT * FROM settings WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY key ASC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SettingRow::into_domain).collect()
    }

    async fn has_live_key(&self, key: &str, entity_id: &str) -> AppResult<bool> {
        let found: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM settings \
             WHERE key = ? AND entity_id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(key)
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(found.is_some())
    }

    async fn update_row_by_id(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()> {
        sqlx::query(
            "UPDATE settings SET \
                entity_id = ?, value = ?, value_type = ?, updated_at = ?, \
                deleted_at = ?, version = ?, dirty = ? \
             WHERE id = ?",
        )
        .bind(&setting.entity_id)
        .bind(setting.value.as_storage())
        .bind(setting.value.value_type())
        .bind(setting.updated_at.to_rfc3339())
        .bind(setting.deleted_at.map(|d| d.to_rfc3339()))
        .bind(setting.version)
        .bind(setting.dirty as i64)
        .bind(setting.id.to_string())
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SettingRow {
    id: String,
    key: String,
    value: String,
    value_type: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl SettingRow {
    fn into_domain(self) -> AppResult<Setting> {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("datetime: {e}")))
        };
        let value = SettingValue::parse(&self.value_type, &self.value).ok_or_else(|| {
            AppError::Validation(format!(
                "cannot parse setting {} as {}: {}",
                self.key, self.value_type, self.value
            ))
        })?;
        Ok(Setting {
            id: uuid::Uuid::parse_str(&self.id)?,
            key: self.key,
            value,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
            deleted_at: self.deleted_at.as_deref().map(parse_dt).transpose()?,
            version: self.version,
            dirty: self.dirty != 0,
            last_synced_at: self.last_synced_at.as_deref().map(parse_dt).transpose()?,
            origin_device_id: self.origin_device_id,
            entity_id: self.entity_id,
        })
    }
}
