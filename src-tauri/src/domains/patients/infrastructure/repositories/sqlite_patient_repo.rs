//! SQLite implementation of `PatientRepo`.
//!
//! - All timestamps stored as RFC3339 TEXT (lex-sortable + millisecond
//!   precision).
//! - Recency lookup uses the `patients_recent` partial index.
//! - FTS5 search joins the virtual `patients_fts` table on `rowid` and
//!   filters tombstones at the SQL layer.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::patients::domain::entities::Patient;
use crate::domains::patients::domain::repositories::PatientRepo;
use crate::error::{AppError, AppResult};

fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

fn parse_dt_opt(s: Option<&str>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(parse_dt).transpose()
}

fn dt_to_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn dt_opt_to_str(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(|d| d.to_rfc3339())
}

#[derive(sqlx::FromRow)]
struct PatientRow {
    id: String,
    name: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl PatientRow {
    fn into_domain(self) -> AppResult<Patient> {
        Ok(Patient {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            name: self.name,
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

#[derive(Clone)]
pub struct SqlitePatientRepo {
    pool: SqlitePool,
}

impl SqlitePatientRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PatientRepo for SqlitePatientRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, p: &Patient) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO patients (\
                id, name, created_at, updated_at, deleted_at, \
                version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
                name = excluded.name, \
                updated_at = excluded.updated_at, \
                deleted_at = excluded.deleted_at, \
                version = excluded.version, \
                dirty = excluded.dirty, \
                last_synced_at = excluded.last_synced_at",
        )
        .bind(p.id.to_string())
        .bind(&p.name)
        .bind(dt_to_str(p.created_at))
        .bind(dt_to_str(p.updated_at))
        .bind(dt_opt_to_str(p.deleted_at))
        .bind(p.version)
        .bind(p.dirty as i64)
        .bind(dt_opt_to_str(p.last_synced_at))
        .bind(p.origin_device_id.as_deref())
        .bind(&p.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Patient>> {
        let row: Option<PatientRow> = sqlx::query_as::<_, PatientRow>(
            "SELECT id, name, created_at, updated_at, deleted_at, version, dirty, \
             last_synced_at, origin_device_id, entity_id \
             FROM patients WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(PatientRow::into_domain).transpose()
    }

    async fn list_recent(&self, entity_id: &str, limit: i64) -> AppResult<Vec<Patient>> {
        let rows = sqlx::query_as::<_, PatientRow>(
            "SELECT id, name, created_at, updated_at, deleted_at, version, dirty, \
             last_synced_at, origin_device_id, entity_id \
             FROM patients WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(entity_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PatientRow::into_domain).collect()
    }

    async fn search(&self, entity_id: &str, query: &str, limit: i64) -> AppResult<Vec<Patient>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_recent(entity_id, limit).await;
        }
        // FTS5 prefix match. Escape internal double-quotes by doubling them
        // (FTS5 literal-quote convention).
        let escaped = trimmed.replace('"', "\"\"");
        let match_expr = format!("\"{escaped}\"*");
        let rows = sqlx::query_as::<_, PatientRow>(
            "SELECT p.id, p.name, p.created_at, p.updated_at, p.deleted_at, p.version, \
                    p.dirty, p.last_synced_at, p.origin_device_id, p.entity_id \
             FROM patients_fts f \
             JOIN patients p ON p.rowid = f.rowid \
             WHERE patients_fts MATCH ? \
               AND p.entity_id = ? AND p.deleted_at IS NULL \
             ORDER BY p.updated_at DESC LIMIT ?",
        )
        .bind(match_expr)
        .bind(entity_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PatientRow::into_domain).collect()
    }

    async fn count_live_visits(&self, patient_id: Uuid) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM visits \
             WHERE patient_id = ? AND deleted_at IS NULL",
        )
        .bind(patient_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}
