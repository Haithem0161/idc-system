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
use crate::domains::patients::domain::repositories::{
    DuplicateGroup, PatientListFilter, PatientRepo, PatientStats, VisitSummary,
};
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
    phone: Option<String>,
    sex: Option<String>,
    birth_date: Option<String>,
    file_no: Option<String>,
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

impl PatientRow {
    fn into_domain(self) -> AppResult<Patient> {
        Ok(Patient {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            name: self.name,
            phone: self.phone,
            sex: self.sex,
            birth_date: self.birth_date,
            file_no: self.file_no,
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

/// Column list shared by every `SELECT` that hydrates a `PatientRow`. Keeping
/// it in one place ensures the FromRow struct and all queries stay aligned.
const PATIENT_COLS: &str = "id, name, phone, sex, birth_date, file_no, notes, \
     created_at, updated_at, deleted_at, version, dirty, \
     last_synced_at, origin_device_id, entity_id";

/// Same columns, table-qualified with `p.` for queries that JOIN (e.g. FTS).
const PATIENT_COLS_P: &str = "p.id, p.name, p.phone, p.sex, p.birth_date, \
     p.file_no, p.notes, p.created_at, p.updated_at, p.deleted_at, p.version, \
     p.dirty, p.last_synced_at, p.origin_device_id, p.entity_id";

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
                id, name, phone, sex, birth_date, file_no, notes, \
                created_at, updated_at, deleted_at, \
                version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
                name = excluded.name, \
                phone = excluded.phone, \
                sex = excluded.sex, \
                birth_date = excluded.birth_date, \
                file_no = excluded.file_no, \
                notes = excluded.notes, \
                updated_at = excluded.updated_at, \
                deleted_at = excluded.deleted_at, \
                version = excluded.version, \
                dirty = excluded.dirty, \
                last_synced_at = excluded.last_synced_at",
        )
        .bind(p.id.to_string())
        .bind(&p.name)
        .bind(p.phone.as_deref())
        .bind(p.sex.as_deref())
        .bind(p.birth_date.as_deref())
        .bind(p.file_no.as_deref())
        .bind(p.notes.as_deref())
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
        let sql = format!("SELECT {PATIENT_COLS} FROM patients WHERE id = ?");
        let row: Option<PatientRow> = sqlx::query_as::<_, PatientRow>(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(PatientRow::into_domain).transpose()
    }

    async fn list_recent(&self, entity_id: &str, limit: i64) -> AppResult<Vec<Patient>> {
        let sql = format!(
            "SELECT {PATIENT_COLS} FROM patients \
             WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY updated_at DESC LIMIT ?"
        );
        let rows = sqlx::query_as::<_, PatientRow>(&sql)
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
        let sql = format!(
            "SELECT {PATIENT_COLS_P} \
             FROM patients_fts f \
             JOIN patients p ON p.rowid = f.rowid \
             WHERE patients_fts MATCH ? \
               AND p.entity_id = ? AND p.deleted_at IS NULL \
             ORDER BY p.updated_at DESC LIMIT ?"
        );
        let rows = sqlx::query_as::<_, PatientRow>(&sql)
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

    async fn list(&self, filter: &PatientListFilter) -> AppResult<Vec<Patient>> {
        let tombstone = if filter.include_deleted {
            ""
        } else {
            "AND p.deleted_at IS NULL"
        };
        let order = filter.sort.order_by();

        let trimmed = filter.query.as_deref().map(str::trim).unwrap_or("");
        let rows = if trimmed.is_empty() {
            // Plain scan. order_by() columns are unqualified here, so alias the
            // table to `p` to share the tombstone clause shape.
            let sql = format!(
                "SELECT {PATIENT_COLS_P} FROM patients p \
                 WHERE p.entity_id = ? {tombstone} \
                 ORDER BY p.{order} LIMIT ? OFFSET ?"
            );
            sqlx::query_as::<_, PatientRow>(&sql)
                .bind(&filter.entity_id)
                .bind(filter.limit)
                .bind(filter.offset)
                .fetch_all(&self.pool)
                .await?
        } else {
            let escaped = trimmed.replace('"', "\"\"");
            let match_expr = format!("\"{escaped}\"*");
            let sql = format!(
                "SELECT {PATIENT_COLS_P} \
                 FROM patients_fts f \
                 JOIN patients p ON p.rowid = f.rowid \
                 WHERE patients_fts MATCH ? AND p.entity_id = ? {tombstone} \
                 ORDER BY p.{order} LIMIT ? OFFSET ?"
            );
            sqlx::query_as::<_, PatientRow>(&sql)
                .bind(match_expr)
                .bind(&filter.entity_id)
                .bind(filter.limit)
                .bind(filter.offset)
                .fetch_all(&self.pool)
                .await?
        };
        rows.into_iter().map(PatientRow::into_domain).collect()
    }

    async fn list_visits_by_patient(
        &self,
        patient_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<VisitSummary>> {
        let rows = sqlx::query_as::<_, VisitSummaryRow>(
            "SELECT id, status, locked_at, created_at, total_amount_iqd_snapshot, \
                    check_type_name_ar_snapshot, check_type_name_en_snapshot, \
                    doctor_name_snapshot, void_reason \
             FROM visits \
             WHERE patient_id = ? AND deleted_at IS NULL \
             ORDER BY COALESCE(locked_at, created_at) DESC, id DESC \
             LIMIT ? OFFSET ?",
        )
        .bind(patient_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(VisitSummaryRow::into_domain).collect()
    }

    async fn patient_stats(&self, patient_id: Uuid) -> AppResult<PatientStats> {
        let row: PatientStatsRow = sqlx::query_as::<_, PatientStatsRow>(
            "SELECT \
               COUNT(*) AS total_visits, \
               COALESCE(SUM(CASE WHEN status = 'locked' \
                   THEN total_amount_iqd_snapshot ELSE 0 END), 0) AS total_spent_iqd, \
               MAX(CASE WHEN status = 'locked' THEN locked_at END) AS last_visit_at, \
               COALESCE(SUM(CASE WHEN status = 'draft' THEN 1 ELSE 0 END), 0) AS draft_count, \
               COALESCE(SUM(CASE WHEN status = 'voided' THEN 1 ELSE 0 END), 0) AS voided_count \
             FROM visits \
             WHERE patient_id = ? AND deleted_at IS NULL",
        )
        .bind(patient_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        Ok(PatientStats {
            total_visits: row.total_visits,
            total_spent_iqd: row.total_spent_iqd,
            last_visit_at: parse_dt_opt(row.last_visit_at.as_deref())?,
            draft_count: row.draft_count,
            voided_count: row.voided_count,
        })
    }

    async fn find_duplicates(&self, entity_id: &str) -> AppResult<Vec<DuplicateGroup>> {
        // Pull the lightweight (id, name, phone) projection for all live rows
        // and group in Rust: name on lower(trim(name)) (script-preserving, no
        // unicode decomposition), phone on a digit-only normalization. This
        // keeps phone normalization identical to the duplicate-merge UX and
        // avoids brittle SQL string functions over Arabic text.
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, phone FROM patients \
             WHERE entity_id = ? AND deleted_at IS NULL",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;

        use std::collections::BTreeMap;
        let mut by_name: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
        let mut by_phone: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
        for (id, name, phone) in &rows {
            let uid =
                Uuid::parse_str(id).map_err(|e| AppError::Validation(format!("uuid: {e}")))?;
            let name_key = name.trim().to_lowercase();
            if !name_key.is_empty() {
                by_name.entry(name_key).or_default().push(uid);
            }
            if let Some(p) = phone {
                let digits: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() >= 6 {
                    by_phone.entry(digits).or_default().push(uid);
                }
            }
        }

        let mut groups = Vec::new();
        for (key, ids) in by_name {
            if ids.len() > 1 {
                groups.push(DuplicateGroup {
                    kind: "name".into(),
                    key,
                    patient_ids: ids,
                });
            }
        }
        for (key, ids) in by_phone {
            if ids.len() > 1 {
                groups.push(DuplicateGroup {
                    kind: "phone".into(),
                    key,
                    patient_ids: ids,
                });
            }
        }
        Ok(groups)
    }

    async fn live_visit_ids_for_patient(
        &self,
        tx: &mut Tx<'_>,
        patient_id: Uuid,
    ) -> AppResult<Vec<Uuid>> {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM visits WHERE patient_id = ? AND deleted_at IS NULL")
                .bind(patient_id.to_string())
                .fetch_all(&mut **tx)
                .await?;
        ids.into_iter()
            .map(|(s,)| Uuid::parse_str(&s).map_err(|e| AppError::Validation(format!("uuid: {e}"))))
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct VisitSummaryRow {
    id: String,
    status: String,
    locked_at: Option<String>,
    created_at: String,
    total_amount_iqd_snapshot: Option<i64>,
    check_type_name_ar_snapshot: Option<String>,
    check_type_name_en_snapshot: Option<String>,
    doctor_name_snapshot: Option<String>,
    void_reason: Option<String>,
}

impl VisitSummaryRow {
    fn into_domain(self) -> AppResult<VisitSummary> {
        Ok(VisitSummary {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            status: self.status,
            locked_at: parse_dt_opt(self.locked_at.as_deref())?,
            created_at: parse_dt(&self.created_at)?,
            total_amount_iqd: self.total_amount_iqd_snapshot,
            check_type_name_ar: self.check_type_name_ar_snapshot,
            check_type_name_en: self.check_type_name_en_snapshot,
            doctor_name: self.doctor_name_snapshot,
            void_reason: self.void_reason,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PatientStatsRow {
    total_visits: i64,
    total_spent_iqd: i64,
    last_visit_at: Option<String>,
    draft_count: i64,
    voided_count: i64,
}
