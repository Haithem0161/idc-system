//! SQLite implementation of `OperatorShiftRepo`.
//!
//! Storage notes:
//! - `created_at` / `updated_at` / `check_in_at` / `check_out_at` /
//!   `deleted_at` stored as RFC3339 TEXT for lexicographic ordering with
//!   millisecond resolution.
//! - `dirty` is materialised as `INTEGER (0|1)`; the application layer is
//!   the sole writer.
//! - The partial unique index `operator_shifts_open` is the second guard
//!   against double-open; the service layer also runs
//!   `has_open_for_operator` before INSERT to surface a `Conflict` error
//!   that's friendlier than the SQLite constraint message.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::shifts::domain::entities::OperatorShift;
use crate::domains::shifts::domain::repositories::{OperatorShiftRepo, OverlapPair};
use crate::error::{AppError, AppResult};

fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

fn parse_dt_opt(s: Option<&str>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(parse_dt).transpose()
}

fn parse_uuid(s: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|e| AppError::Validation(format!("uuid: {e}")))
}

fn parse_uuid_opt(s: Option<&str>) -> AppResult<Option<Uuid>> {
    s.map(parse_uuid).transpose()
}

fn dt_to_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn dt_opt_to_str(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(|d| d.to_rfc3339())
}

#[derive(Clone)]
pub struct SqliteOperatorShiftRepo {
    pool: SqlitePool,
}

impl SqliteOperatorShiftRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OperatorShiftRepo for SqliteOperatorShiftRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, s: &OperatorShift) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO operator_shifts (\
                id, operator_id, check_in_at, check_out_at, check_in_by_user_id, \
                check_out_by_user_id, note, created_at, updated_at, deleted_at, \
                version, dirty, last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               operator_id = excluded.operator_id, \
               check_in_at = excluded.check_in_at, \
               check_out_at = excluded.check_out_at, \
               check_in_by_user_id = excluded.check_in_by_user_id, \
               check_out_by_user_id = excluded.check_out_by_user_id, \
               note = excluded.note, \
               updated_at = excluded.updated_at, \
               deleted_at = excluded.deleted_at, \
               version = excluded.version, \
               dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(s.id.to_string())
        .bind(s.operator_id.to_string())
        .bind(dt_to_str(s.check_in_at))
        .bind(dt_opt_to_str(s.check_out_at))
        .bind(s.check_in_by_user_id.to_string())
        .bind(s.check_out_by_user_id.map(|u| u.to_string()))
        .bind(s.note.as_deref())
        .bind(dt_to_str(s.created_at))
        .bind(dt_to_str(s.updated_at))
        .bind(dt_opt_to_str(s.deleted_at))
        .bind(s.version)
        .bind(s.dirty as i64)
        .bind(dt_opt_to_str(s.last_synced_at))
        .bind(s.origin_device_id.as_deref())
        .bind(&s.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<OperatorShift>> {
        let row: Option<OperatorShiftRow> =
            sqlx::query_as::<_, OperatorShiftRow>("SELECT * FROM operator_shifts WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(OperatorShiftRow::into_domain).transpose()
    }

    async fn list_open(&self, entity_id: &str) -> AppResult<Vec<OperatorShift>> {
        let rows = sqlx::query_as::<_, OperatorShiftRow>(
            "SELECT * FROM operator_shifts \
             WHERE entity_id = ? AND check_out_at IS NULL AND deleted_at IS NULL \
             ORDER BY check_in_at ASC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(OperatorShiftRow::into_domain)
            .collect()
    }

    async fn history_today(
        &self,
        entity_id: &str,
        today_start: DateTime<Utc>,
        today_end: DateTime<Utc>,
    ) -> AppResult<Vec<OperatorShift>> {
        let rows = sqlx::query_as::<_, OperatorShiftRow>(
            "SELECT * FROM operator_shifts \
             WHERE entity_id = ? AND deleted_at IS NULL \
               AND check_in_at >= ? AND check_in_at < ? \
             ORDER BY check_in_at ASC",
        )
        .bind(entity_id)
        .bind(dt_to_str(today_start))
        .bind(dt_to_str(today_end))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(OperatorShiftRow::into_domain)
            .collect()
    }

    async fn has_open_for_operator(
        &self,
        operator_id: Uuid,
        except_id: Option<Uuid>,
    ) -> AppResult<bool> {
        let except_str = except_id.map(|u| u.to_string()).unwrap_or_default();
        let (cnt,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM operator_shifts \
             WHERE operator_id = ? AND check_out_at IS NULL AND deleted_at IS NULL \
               AND (? = '' OR id != ?)",
        )
        .bind(operator_id.to_string())
        .bind(&except_str)
        .bind(&except_str)
        .fetch_one(&self.pool)
        .await?;
        Ok(cnt > 0)
    }

    async fn list_overlaps_for_operator(
        &self,
        operator_id: Uuid,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<OverlapPair>> {
        let rows = sqlx::query_as::<_, OperatorShiftRow>(
            "SELECT * FROM operator_shifts \
             WHERE operator_id = ? AND deleted_at IS NULL \
             ORDER BY check_in_at ASC",
        )
        .bind(operator_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let shifts: Vec<OperatorShift> = rows
            .into_iter()
            .map(OperatorShiftRow::into_domain)
            .collect::<AppResult<_>>()?;
        Ok(compute_overlaps(&shifts, now))
    }

    async fn list_for_operator(
        &self,
        operator_id: Uuid,
        except_id: Option<Uuid>,
    ) -> AppResult<Vec<OperatorShift>> {
        let except_str = except_id.map(|u| u.to_string()).unwrap_or_default();
        let rows = sqlx::query_as::<_, OperatorShiftRow>(
            "SELECT * FROM operator_shifts \
             WHERE operator_id = ? AND deleted_at IS NULL \
               AND (? = '' OR id != ?) \
             ORDER BY check_in_at ASC",
        )
        .bind(operator_id.to_string())
        .bind(&except_str)
        .bind(&except_str)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(OperatorShiftRow::into_domain)
            .collect()
    }

    async fn list_overlaps(
        &self,
        entity_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<OverlapPair>> {
        let rows = sqlx::query_as::<_, OperatorShiftRow>(
            "SELECT * FROM operator_shifts \
             WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY operator_id ASC, check_in_at ASC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        let shifts: Vec<OperatorShift> = rows
            .into_iter()
            .map(OperatorShiftRow::into_domain)
            .collect::<AppResult<_>>()?;
        let mut pairs: Vec<OverlapPair> = Vec::new();
        let mut by_op: std::collections::HashMap<Uuid, Vec<OperatorShift>> =
            std::collections::HashMap::new();
        for s in shifts {
            by_op.entry(s.operator_id).or_default().push(s);
        }
        for (_, list) in by_op {
            pairs.extend(compute_overlaps(&list, now));
        }
        Ok(pairs)
    }
}

/// Pairwise overlap detection on a per-operator slice. O(n^2) but n stays
/// small (per-operator, per-tenant). Open shifts treat `now` as the end.
fn compute_overlaps(shifts: &[OperatorShift], now: DateTime<Utc>) -> Vec<OverlapPair> {
    let mut out: Vec<OverlapPair> = Vec::new();
    for i in 0..shifts.len() {
        let a = &shifts[i];
        let a_end = a.check_out_at.unwrap_or(now);
        for b in shifts.iter().skip(i + 1) {
            let b_end = b.check_out_at.unwrap_or(now);
            // Treat as overlap if intervals share any moment. Adjacent
            // touching (a_end == b.check_in_at) is NOT considered overlap.
            if a.check_in_at < b_end && b.check_in_at < a_end {
                out.push(OverlapPair {
                    left: a.clone(),
                    right: b.clone(),
                });
            }
        }
    }
    out
}

#[derive(sqlx::FromRow)]
struct OperatorShiftRow {
    id: String,
    operator_id: String,
    check_in_at: String,
    check_out_at: Option<String>,
    check_in_by_user_id: String,
    check_out_by_user_id: Option<String>,
    note: Option<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl OperatorShiftRow {
    fn into_domain(self) -> AppResult<OperatorShift> {
        Ok(OperatorShift {
            id: parse_uuid(&self.id)?,
            operator_id: parse_uuid(&self.operator_id)?,
            check_in_at: parse_dt(&self.check_in_at)?,
            check_out_at: parse_dt_opt(self.check_out_at.as_deref())?,
            check_in_by_user_id: parse_uuid(&self.check_in_by_user_id)?,
            check_out_by_user_id: parse_uuid_opt(self.check_out_by_user_id.as_deref())?,
            note: self.note,
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
