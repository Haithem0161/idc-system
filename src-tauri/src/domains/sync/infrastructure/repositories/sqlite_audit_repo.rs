//! sqlx-backed implementation of `AuditRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::Tx;
use crate::domains::sync::domain::entities::AuditEntry;
use crate::domains::sync::domain::repositories::AuditRepo;
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
        use chrono::Utc;

        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::AppError::Validation(format!("datetime: {e}")))
        };

        let action = match self.action.as_str() {
            "create" => AuditAction::Create,
            "update" => AuditAction::Update,
            "soft_delete" => AuditAction::SoftDelete,
            "lock" => AuditAction::Lock,
            "void" => AuditAction::Void,
            "discard" => AuditAction::Discard,
            "clock_in" => AuditAction::ClockIn,
            "clock_out" => AuditAction::ClockOut,
            "password_change" => AuditAction::PasswordChange,
            "login" => AuditAction::Login,
            "logout" => AuditAction::Logout,
            "conflict_resolve" => AuditAction::ConflictResolve,
            "vacuum" => AuditAction::Vacuum,
            "daily_close_run" => AuditAction::DailyCloseRun,
            other => {
                return Err(crate::error::AppError::Validation(format!(
                    "unknown audit action: {other}"
                )))
            }
        };

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
