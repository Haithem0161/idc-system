//! SQLite implementation of `DoctorRepo` with FTS5 search.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::Doctor;
use crate::domains::catalog::domain::repositories::{CatalogListFilter, DoctorRepo};
use crate::error::AppResult;

use super::common::{dt_opt_to_str, dt_to_str, like_prefix, parse_dt, parse_dt_opt, parse_uuid};

#[derive(Clone)]
pub struct SqliteDoctorRepo {
    pool: SqlitePool,
}

impl SqliteDoctorRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DoctorRepo for SqliteDoctorRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, doc: &Doctor) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO doctors (\
                id, name, specialty, phone, is_active, notes, \
                default_cut_kind, default_cut_value, created_at, updated_at, \
                deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               name = excluded.name, specialty = excluded.specialty, phone = excluded.phone, \
               is_active = excluded.is_active, notes = excluded.notes, \
               default_cut_kind = excluded.default_cut_kind, \
               default_cut_value = excluded.default_cut_value, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(doc.id.to_string())
        .bind(&doc.name)
        .bind(doc.specialty.as_deref())
        .bind(doc.phone.as_deref())
        .bind(doc.is_active as i64)
        .bind(doc.notes.as_deref())
        .bind(doc.default_cut_kind.as_deref())
        .bind(doc.default_cut_value)
        .bind(dt_to_str(doc.created_at))
        .bind(dt_to_str(doc.updated_at))
        .bind(dt_opt_to_str(doc.deleted_at))
        .bind(doc.version)
        .bind(doc.dirty as i64)
        .bind(dt_opt_to_str(doc.last_synced_at))
        .bind(doc.origin_device_id.as_deref())
        .bind(&doc.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Doctor>> {
        let row: Option<DoctorRow> =
            sqlx::query_as::<_, DoctorRow>("SELECT * FROM doctors WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(DoctorRow::into_domain).transpose()
    }

    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<Doctor>> {
        let mut sql = String::from("SELECT * FROM doctors WHERE entity_id = ?");
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
            sql.push_str(" AND (name LIKE ? ESCAPE '\\' OR specialty LIKE ? ESCAPE '\\')");
        }
        sql.push_str(" ORDER BY name ASC");

        let mut q = sqlx::query_as::<_, DoctorRow>(&sql).bind(&filter.entity_id);
        if let Some(p) = like.as_ref() {
            q = q.bind(p).bind(p);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(DoctorRow::into_domain).collect()
    }

    async fn search_fts(
        &self,
        entity_id: &str,
        query: &str,
        include_inactive: bool,
    ) -> AppResult<Vec<Doctor>> {
        // Sanitize the FTS query: strip MATCH-special chars to avoid syntax
        // errors when users type free text. Append `*` for prefix matching.
        let cleaned = sanitize_fts(query);
        if cleaned.is_empty() {
            return Ok(vec![]);
        }
        let mut sql = String::from(
            "SELECT d.* FROM doctors d \
             JOIN doctors_fts f ON f.rowid = d.rowid \
             WHERE doctors_fts MATCH ? \
             AND d.entity_id = ? \
             AND d.deleted_at IS NULL",
        );
        if !include_inactive {
            sql.push_str(" AND d.is_active = 1");
        }
        sql.push_str(" ORDER BY rank LIMIT 100");

        let rows: Vec<DoctorRow> = sqlx::query_as::<_, DoctorRow>(&sql)
            .bind(cleaned)
            .bind(entity_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(DoctorRow::into_domain).collect()
    }
}

fn sanitize_fts(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let tokens: Vec<String> = trimmed
        .split_whitespace()
        .map(|t| {
            let clean: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("{clean}*")
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    tokens.join(" ")
}

#[derive(sqlx::FromRow)]
struct DoctorRow {
    id: String,
    name: String,
    specialty: Option<String>,
    phone: Option<String>,
    is_active: i64,
    notes: Option<String>,
    default_cut_kind: Option<String>,
    default_cut_value: Option<i64>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl DoctorRow {
    fn into_domain(self) -> AppResult<Doctor> {
        Ok(Doctor {
            id: parse_uuid(&self.id)?,
            name: self.name,
            specialty: self.specialty,
            phone: self.phone,
            is_active: self.is_active != 0,
            notes: self.notes,
            default_cut_kind: self.default_cut_kind,
            default_cut_value: self.default_cut_value,
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
