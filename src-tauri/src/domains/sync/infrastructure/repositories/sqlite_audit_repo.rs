//! sqlx-backed implementation of `AuditRepo`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Arguments, SqlitePool};

use crate::db::Tx;
use crate::domains::sync::domain::entities::AuditEntry;
use crate::domains::sync::domain::repositories::{AuditFilter, AuditRepo};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::AppResult;

#[derive(Clone)]
pub struct SqliteAuditRepo {
    pool: SqlitePool,
}

impl SqliteAuditRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditRepo for SqliteAuditRepo {
    async fn append(&self, tx: &mut Tx<'_>, entry: &AuditEntry) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO audit_log (\
                id, actor_user_id, action, entity, entity_id, delta, ip, device_id, at, \
                created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                origin_device_id, entity_id_tenant\
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(entry.id.to_string())
        .bind(entry.actor_user_id.to_string())
        .bind(entry.action.as_str())
        .bind(&entry.entity)
        .bind(&entry.entity_id)
        .bind(serde_json::to_string(&entry.delta)?)
        .bind(entry.ip.as_deref())
        .bind(&entry.device_id)
        .bind(entry.at.to_rfc3339())
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .bind(entry.deleted_at.map(|d| d.to_rfc3339()))
        .bind(entry.version)
        .bind(entry.dirty as i64)
        .bind(entry.last_synced_at.map(|d| d.to_rfc3339()))
        .bind(entry.origin_device_id.as_deref())
        .bind(&entry.entity_id_tenant)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn list_by_tenant(
        &self,
        entity_id_tenant: &str,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AuditEntry>> {
        let rows: Vec<AuditRow> = sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor_user_id, action, entity, entity_id, delta, ip, device_id, at, \
                    created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                    origin_device_id, entity_id_tenant \
             FROM audit_log \
             WHERE entity_id_tenant = ? AND deleted_at IS NULL \
             ORDER BY at DESC \
             LIMIT ? OFFSET ?",
        )
        .bind(entity_id_tenant)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(AuditRow::into_domain).collect()
    }

    async fn query(&self, filter: &AuditFilter) -> AppResult<Vec<AuditEntry>> {
        let filter = filter.clone().clamp();

        let mut sql = String::from(
            "SELECT id, actor_user_id, action, entity, entity_id, delta, ip, device_id, at, \
                    created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                    origin_device_id, entity_id_tenant \
             FROM audit_log \
             WHERE entity_id_tenant = ? AND deleted_at IS NULL",
        );
        let mut args = sqlx::sqlite::SqliteArguments::default();
        args.add(&filter.entity_id_tenant)
            .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;

        if let Some(actor) = filter.actor_user_id.as_deref() {
            sql.push_str(" AND actor_user_id = ?");
            args.add(actor)
                .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        }
        if let Some(action) = filter.action.as_deref() {
            sql.push_str(" AND action = ?");
            args.add(action)
                .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        }
        if let Some(entity) = filter.entity.as_deref() {
            sql.push_str(" AND entity = ?");
            args.add(entity)
                .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        }
        if let Some(prefix) = filter.entity_id_prefix.as_deref() {
            sql.push_str(" AND entity_id LIKE ?");
            args.add(format!("{prefix}%"))
                .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        }
        if let Some(from) = filter.from_utc {
            sql.push_str(" AND at >= ?");
            args.add(from.to_rfc3339())
                .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        }
        if let Some(to) = filter.to_utc {
            sql.push_str(" AND at <= ?");
            args.add(to.to_rfc3339())
                .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        }
        if let Some(text) = filter.free_text.as_deref() {
            if !text.is_empty() {
                sql.push_str(" AND (INSTR(delta, ?) > 0 OR INSTR(entity_id, ?) > 0)");
                args.add(text)
                    .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
                args.add(text)
                    .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
            }
        }

        sql.push_str(" ORDER BY at DESC, id DESC LIMIT ? OFFSET ?");
        args.add(filter.limit)
            .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;
        args.add(filter.offset)
            .map_err(|e| crate::error::AppError::Database(format!("bind: {e}")))?;

        let rows: Vec<AuditRow> = sqlx::query_as_with::<_, AuditRow, _>(&sql, args)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(AuditRow::into_domain).collect()
    }

    async fn vacuum_unsynced_safe(&self, cutoff: DateTime<Utc>) -> AppResult<u64> {
        let result = sqlx::query(
            "DELETE FROM audit_log \
             WHERE at < ? AND dirty = 0 AND deleted_at IS NULL",
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn oldest_at(&self, entity_id_tenant: &str) -> AppResult<Option<DateTime<Utc>>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT at FROM audit_log \
             WHERE entity_id_tenant = ? AND deleted_at IS NULL \
             ORDER BY at ASC LIMIT 1",
        )
        .bind(entity_id_tenant)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            None => Ok(None),
            Some((s,)) => {
                let dt = chrono::DateTime::parse_from_rfc3339(&s)
                    .map_err(|e| crate::error::AppError::Validation(format!("datetime: {e}")))?
                    .with_timezone(&Utc);
                Ok(Some(dt))
            }
        }
    }
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: String,
    actor_user_id: String,
    action: String,
    entity: String,
    entity_id: String,
    delta: String,
    ip: Option<String>,
    device_id: String,
    at: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id_tenant: String,
}

impl AuditRow {
    fn into_domain(self) -> AppResult<AuditEntry> {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::AppError::Validation(format!("datetime: {e}")))
        };

        // Parse via the enum's single source of truth so this never drifts
        // from `AuditAction::as_str` when a new action is added.
        let action = AuditAction::from_db_str(&self.action).ok_or_else(|| {
            crate::error::AppError::Validation(format!("unknown audit action: {}", self.action))
        })?;

        Ok(AuditEntry {
            id: uuid::Uuid::parse_str(&self.id)?,
            actor_user_id: uuid::Uuid::parse_str(&self.actor_user_id)?,
            action,
            entity: self.entity,
            entity_id: self.entity_id,
            delta: serde_json::from_str(&self.delta).unwrap_or(serde_json::Value::Null),
            ip: self.ip,
            device_id: self.device_id,
            at: parse_dt(&self.at)?,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
            deleted_at: self.deleted_at.as_deref().map(parse_dt).transpose()?,
            version: self.version,
            dirty: self.dirty != 0,
            last_synced_at: self.last_synced_at.as_deref().map(parse_dt).transpose()?,
            origin_device_id: self.origin_device_id,
            entity_id_tenant: self.entity_id_tenant,
        })
    }
}
