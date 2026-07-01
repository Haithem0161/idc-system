//! SQLite implementation of `FrozenCloseRepo`. Additive-only: `insert` uses
//! INSERT OR IGNORE so a retried push/pull never duplicates a signed close.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::reports::domain::entities::FrozenClose;
use crate::domains::reports::domain::repositories::FrozenCloseRepo;
use crate::error::{AppError, AppResult};

fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

fn parse_dt_opt(s: Option<String>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(|x| parse_dt(&x)).transpose()
}

fn parse_uuid(s: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|e| AppError::Validation(format!("uuid: {e}")))
}

fn parse_uuid_opt(s: Option<String>) -> AppResult<Option<Uuid>> {
    s.map(|x| parse_uuid(&x)).transpose()
}

fn parse_date(s: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| AppError::Validation(format!("date: {e}")))
}

const SELECT_COLS: &str = "id, target_date, tz_offset, input_hash, \
    total_revenue_iqd, total_collected_iqd, total_discount_iqd, \
    total_doctor_cuts_iqd, total_operator_cuts_iqd, total_report_iqd, \
    total_mandoub_cuts_iqd, \
    total_inventory_consumption_value_iqd, net_iqd, locked_count, voided_count, \
    voided_value_iqd, signed_by_user_id, signed_by_name, signed_at, \
    reopened_at, reopened_by_user_id, reopen_reason, \
    created_at, updated_at, version, origin_device_id, entity_id";

fn map_row(r: &sqlx::sqlite::SqliteRow) -> AppResult<FrozenClose> {
    Ok(FrozenClose {
        id: parse_uuid(r.get::<String, _>("id").as_str())?,
        target_date: parse_date(r.get::<String, _>("target_date").as_str())?,
        tz_offset: r.get("tz_offset"),
        input_hash: r.get("input_hash"),
        total_revenue_iqd: r.get("total_revenue_iqd"),
        total_collected_iqd: r.get("total_collected_iqd"),
        total_discount_iqd: r.get("total_discount_iqd"),
        total_doctor_cuts_iqd: r.get("total_doctor_cuts_iqd"),
        total_operator_cuts_iqd: r.get("total_operator_cuts_iqd"),
        total_report_iqd: r.get("total_report_iqd"),
        total_mandoub_cuts_iqd: r.get("total_mandoub_cuts_iqd"),
        total_inventory_consumption_value_iqd: r.get("total_inventory_consumption_value_iqd"),
        net_iqd: r.get("net_iqd"),
        locked_count: r.get("locked_count"),
        voided_count: r.get("voided_count"),
        voided_value_iqd: r.get("voided_value_iqd"),
        signed_by_user_id: parse_uuid(r.get::<String, _>("signed_by_user_id").as_str())?,
        signed_by_name: r.get("signed_by_name"),
        signed_at: parse_dt(r.get::<String, _>("signed_at").as_str())?,
        reopened_at: parse_dt_opt(r.get("reopened_at"))?,
        reopened_by_user_id: parse_uuid_opt(r.get("reopened_by_user_id"))?,
        reopen_reason: r.get("reopen_reason"),
        created_at: parse_dt(r.get::<String, _>("created_at").as_str())?,
        updated_at: parse_dt(r.get::<String, _>("updated_at").as_str())?,
        version: r.get("version"),
        origin_device_id: r.get("origin_device_id"),
        entity_id: r.get("entity_id"),
    })
}

#[derive(Clone)]
pub struct SqliteFrozenCloseRepo {
    pool: SqlitePool,
}

impl SqliteFrozenCloseRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FrozenCloseRepo for SqliteFrozenCloseRepo {
    async fn insert(&self, tx: &mut Tx<'_>, c: &FrozenClose) -> AppResult<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO daily_close ( \
                id, target_date, tz_offset, input_hash, \
                total_revenue_iqd, total_collected_iqd, total_discount_iqd, \
                total_doctor_cuts_iqd, total_operator_cuts_iqd, total_report_iqd, \
                total_mandoub_cuts_iqd, \
                total_inventory_consumption_value_iqd, net_iqd, locked_count, \
                voided_count, voided_value_iqd, \
                signed_by_user_id, signed_by_name, signed_at, \
                reopened_at, reopened_by_user_id, reopen_reason, \
                created_at, updated_at, deleted_at, version, dirty, \
                last_synced_at, origin_device_id, entity_id \
             ) VALUES (?,?,?,?, ?,?,?,?,?,?, ?, ?,?,?, ?,?, ?,?,?, ?,?,?, ?,?,NULL,?,1, NULL,?,?)",
        )
        .bind(c.id.to_string())
        .bind(c.target_date.format("%Y-%m-%d").to_string())
        .bind(&c.tz_offset)
        .bind(&c.input_hash)
        .bind(c.total_revenue_iqd)
        .bind(c.total_collected_iqd)
        .bind(c.total_discount_iqd)
        .bind(c.total_doctor_cuts_iqd)
        .bind(c.total_operator_cuts_iqd)
        .bind(c.total_report_iqd)
        .bind(c.total_mandoub_cuts_iqd)
        .bind(c.total_inventory_consumption_value_iqd)
        .bind(c.net_iqd)
        .bind(c.locked_count)
        .bind(c.voided_count)
        .bind(c.voided_value_iqd)
        .bind(c.signed_by_user_id.to_string())
        .bind(&c.signed_by_name)
        .bind(c.signed_at.to_rfc3339())
        .bind(c.reopened_at.map(|d| d.to_rfc3339()))
        .bind(c.reopened_by_user_id.map(|u| u.to_string()))
        .bind(c.reopen_reason.as_deref())
        .bind(c.created_at.to_rfc3339())
        .bind(c.updated_at.to_rfc3339())
        .bind(c.version)
        .bind(c.origin_device_id.as_deref())
        .bind(&c.entity_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    async fn save_reopen(&self, tx: &mut Tx<'_>, c: &FrozenClose) -> AppResult<()> {
        sqlx::query(
            "UPDATE daily_close SET \
                reopened_at = ?, reopened_by_user_id = ?, reopen_reason = ?, \
                updated_at = ?, version = ?, dirty = 1 \
             WHERE id = ?",
        )
        .bind(c.reopened_at.map(|d| d.to_rfc3339()))
        .bind(c.reopened_by_user_id.map(|u| u.to_string()))
        .bind(c.reopen_reason.as_deref())
        .bind(c.updated_at.to_rfc3339())
        .bind(c.version)
        .bind(c.id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    async fn find_in_force_for_date(
        &self,
        entity_id: &str,
        target_date: NaiveDate,
    ) -> AppResult<Option<FrozenClose>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM daily_close \
             WHERE entity_id = ? AND target_date = ? \
               AND reopened_at IS NULL AND deleted_at IS NULL \
             LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(entity_id)
            .bind(target_date.format("%Y-%m-%d").to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)?;
        row.as_ref().map(map_row).transpose()
    }

    async fn find_by_id(&self, entity_id: &str, id: Uuid) -> AppResult<Option<FrozenClose>> {
        let sql =
            format!("SELECT {SELECT_COLS} FROM daily_close WHERE entity_id = ? AND id = ? LIMIT 1");
        let row = sqlx::query(&sql)
            .bind(entity_id)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)?;
        row.as_ref().map(map_row).transpose()
    }

    async fn list_in_range(
        &self,
        entity_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> AppResult<Vec<FrozenClose>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM daily_close \
             WHERE entity_id = ? AND target_date >= ? AND target_date <= ? \
               AND deleted_at IS NULL \
             ORDER BY target_date DESC, signed_at DESC"
        );
        let rows = sqlx::query(&sql)
            .bind(entity_id)
            .bind(from_date.format("%Y-%m-%d").to_string())
            .bind(to_date.format("%Y-%m-%d").to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        rows.iter().map(map_row).collect()
    }
}
