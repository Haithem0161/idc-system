//! SQLite implementation of `VisitRepo`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::visits::domain::entities::{Visit, VisitSnapshots, VisitStatus};
use crate::domains::visits::domain::repositories::{VisitRepo, WorkspaceFilters};
use crate::error::{AppError, AppResult};

fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

fn parse_dt_opt(s: Option<&str>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(parse_dt).transpose()
}

fn parse_uuid_opt(s: Option<&str>) -> AppResult<Option<Uuid>> {
    s.map(|x| Uuid::parse_str(x).map_err(|e| AppError::Validation(format!("uuid: {e}"))))
        .transpose()
}

fn dt_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn dt_opt_str(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(|d| d.to_rfc3339())
}

#[derive(sqlx::FromRow)]
struct VisitRow {
    id: String,
    patient_id: String,
    status: String,
    receptionist_user_id: String,
    check_type_id: String,
    check_subtype_id: Option<String>,
    doctor_id: Option<String>,
    operator_id: Option<String>,
    dye: i64,
    report: i64,
    locked_at: Option<String>,
    voided_at: Option<String>,
    voided_by_user_id: Option<String>,
    void_reason: Option<String>,
    price_snapshot_iqd: Option<i64>,
    dye_cost_snapshot_iqd: Option<i64>,
    report_cost_snapshot_iqd: Option<i64>,
    doctor_cut_snapshot_iqd: Option<i64>,
    operator_cut_snapshot_iqd: Option<i64>,
    internal_pct_snapshot: Option<i64>,
    total_amount_iqd_snapshot: Option<i64>,
    patient_name_snapshot: Option<String>,
    doctor_name_snapshot: Option<String>,
    operator_name_snapshot: Option<String>,
    check_type_name_ar_snapshot: Option<String>,
    check_type_name_en_snapshot: Option<String>,
    check_subtype_name_ar_snapshot: Option<String>,
    check_subtype_name_en_snapshot: Option<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl VisitRow {
    fn into_domain(self) -> AppResult<Visit> {
        let status = VisitStatus::parse(&self.status)
            .ok_or_else(|| AppError::Validation(format!("status: {}", self.status)))?;
        let snapshots = if status == VisitStatus::Locked {
            Some(VisitSnapshots {
                price_iqd: self.price_snapshot_iqd.unwrap_or(0),
                dye_cost_iqd: self.dye_cost_snapshot_iqd.unwrap_or(0),
                report_cost_iqd: self.report_cost_snapshot_iqd.unwrap_or(0),
                doctor_cut_iqd: self.doctor_cut_snapshot_iqd.unwrap_or(0),
                operator_cut_iqd: self.operator_cut_snapshot_iqd.unwrap_or(0),
                internal_pct: self.internal_pct_snapshot,
                total_amount_iqd: self.total_amount_iqd_snapshot.unwrap_or(0),
                patient_name: self.patient_name_snapshot.clone().unwrap_or_default(),
                doctor_name: self.doctor_name_snapshot.clone(),
                operator_name: self.operator_name_snapshot.clone().unwrap_or_default(),
                check_type_name_ar: self.check_type_name_ar_snapshot.clone().unwrap_or_default(),
                check_type_name_en: self.check_type_name_en_snapshot.clone(),
                check_subtype_name_ar: self.check_subtype_name_ar_snapshot.clone(),
                check_subtype_name_en: self.check_subtype_name_en_snapshot.clone(),
            })
        } else if status == VisitStatus::Voided {
            // Snapshots persist across the void transition.
            Some(VisitSnapshots {
                price_iqd: self.price_snapshot_iqd.unwrap_or(0),
                dye_cost_iqd: self.dye_cost_snapshot_iqd.unwrap_or(0),
                report_cost_iqd: self.report_cost_snapshot_iqd.unwrap_or(0),
                doctor_cut_iqd: self.doctor_cut_snapshot_iqd.unwrap_or(0),
                operator_cut_iqd: self.operator_cut_snapshot_iqd.unwrap_or(0),
                internal_pct: self.internal_pct_snapshot,
                total_amount_iqd: self.total_amount_iqd_snapshot.unwrap_or(0),
                patient_name: self.patient_name_snapshot.clone().unwrap_or_default(),
                doctor_name: self.doctor_name_snapshot.clone(),
                operator_name: self.operator_name_snapshot.clone().unwrap_or_default(),
                check_type_name_ar: self.check_type_name_ar_snapshot.clone().unwrap_or_default(),
                check_type_name_en: self.check_type_name_en_snapshot.clone(),
                check_subtype_name_ar: self.check_subtype_name_ar_snapshot.clone(),
                check_subtype_name_en: self.check_subtype_name_en_snapshot.clone(),
            })
        } else {
            None
        };
        Ok(Visit {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            patient_id: Uuid::parse_str(&self.patient_id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            status,
            receptionist_user_id: Uuid::parse_str(&self.receptionist_user_id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            check_type_id: Uuid::parse_str(&self.check_type_id)
                .map_err(|e| AppError::Validation(format!("uuid: {e}")))?,
            check_subtype_id: parse_uuid_opt(self.check_subtype_id.as_deref())?,
            doctor_id: parse_uuid_opt(self.doctor_id.as_deref())?,
            operator_id: parse_uuid_opt(self.operator_id.as_deref())?,
            dye: self.dye != 0,
            report: self.report != 0,
            locked_at: parse_dt_opt(self.locked_at.as_deref())?,
            voided_at: parse_dt_opt(self.voided_at.as_deref())?,
            voided_by_user_id: parse_uuid_opt(self.voided_by_user_id.as_deref())?,
            void_reason: self.void_reason,
            snapshots,
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

const COLUMNS: &str = "id, patient_id, status, receptionist_user_id, check_type_id, \
                       check_subtype_id, doctor_id, operator_id, dye, report, locked_at, \
                       voided_at, voided_by_user_id, void_reason, price_snapshot_iqd, \
                       dye_cost_snapshot_iqd, report_cost_snapshot_iqd, doctor_cut_snapshot_iqd, \
                       operator_cut_snapshot_iqd, internal_pct_snapshot, total_amount_iqd_snapshot, \
                       patient_name_snapshot, doctor_name_snapshot, operator_name_snapshot, \
                       check_type_name_ar_snapshot, check_type_name_en_snapshot, \
                       check_subtype_name_ar_snapshot, check_subtype_name_en_snapshot, \
                       created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                       origin_device_id, entity_id";

#[derive(Clone)]
pub struct SqliteVisitRepo {
    pool: SqlitePool,
}

impl SqliteVisitRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VisitRepo for SqliteVisitRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, v: &Visit) -> AppResult<()> {
        let snap = v.snapshots.as_ref();
        let sql = format!(
            "INSERT INTO visits ({COLUMNS}) VALUES (\
                ?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?\
             ) ON CONFLICT(id) DO UPDATE SET \
               status = excluded.status, \
               patient_id = excluded.patient_id, \
               check_subtype_id = excluded.check_subtype_id, \
               doctor_id = excluded.doctor_id, \
               operator_id = excluded.operator_id, \
               dye = excluded.dye, \
               report = excluded.report, \
               locked_at = excluded.locked_at, \
               voided_at = excluded.voided_at, \
               voided_by_user_id = excluded.voided_by_user_id, \
               void_reason = excluded.void_reason, \
               price_snapshot_iqd = excluded.price_snapshot_iqd, \
               dye_cost_snapshot_iqd = excluded.dye_cost_snapshot_iqd, \
               report_cost_snapshot_iqd = excluded.report_cost_snapshot_iqd, \
               doctor_cut_snapshot_iqd = excluded.doctor_cut_snapshot_iqd, \
               operator_cut_snapshot_iqd = excluded.operator_cut_snapshot_iqd, \
               internal_pct_snapshot = excluded.internal_pct_snapshot, \
               total_amount_iqd_snapshot = excluded.total_amount_iqd_snapshot, \
               patient_name_snapshot = excluded.patient_name_snapshot, \
               doctor_name_snapshot = excluded.doctor_name_snapshot, \
               operator_name_snapshot = excluded.operator_name_snapshot, \
               check_type_name_ar_snapshot = excluded.check_type_name_ar_snapshot, \
               check_type_name_en_snapshot = excluded.check_type_name_en_snapshot, \
               check_subtype_name_ar_snapshot = excluded.check_subtype_name_ar_snapshot, \
               check_subtype_name_en_snapshot = excluded.check_subtype_name_en_snapshot, \
               updated_at = excluded.updated_at, \
               deleted_at = excluded.deleted_at, \
               version = excluded.version, \
               dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at"
        );
        sqlx::query(&sql)
            .bind(v.id.to_string())
            .bind(v.patient_id.to_string())
            .bind(v.status.as_str())
            .bind(v.receptionist_user_id.to_string())
            .bind(v.check_type_id.to_string())
            .bind(v.check_subtype_id.map(|u| u.to_string()))
            .bind(v.doctor_id.map(|u| u.to_string()))
            .bind(v.operator_id.map(|u| u.to_string()))
            .bind(v.dye as i64)
            .bind(v.report as i64)
            .bind(dt_opt_str(v.locked_at))
            .bind(dt_opt_str(v.voided_at))
            .bind(v.voided_by_user_id.map(|u| u.to_string()))
            .bind(v.void_reason.as_deref())
            .bind(snap.map(|s| s.price_iqd))
            .bind(snap.map(|s| s.dye_cost_iqd))
            .bind(snap.map(|s| s.report_cost_iqd))
            .bind(snap.map(|s| s.doctor_cut_iqd))
            .bind(snap.map(|s| s.operator_cut_iqd))
            .bind(snap.and_then(|s| s.internal_pct))
            .bind(snap.map(|s| s.total_amount_iqd))
            .bind(snap.map(|s| s.patient_name.clone()))
            .bind(snap.and_then(|s| s.doctor_name.clone()))
            .bind(snap.map(|s| s.operator_name.clone()))
            .bind(snap.map(|s| s.check_type_name_ar.clone()))
            .bind(snap.and_then(|s| s.check_type_name_en.clone()))
            .bind(snap.and_then(|s| s.check_subtype_name_ar.clone()))
            .bind(snap.and_then(|s| s.check_subtype_name_en.clone()))
            .bind(dt_str(v.created_at))
            .bind(dt_str(v.updated_at))
            .bind(dt_opt_str(v.deleted_at))
            .bind(v.version)
            .bind(v.dirty as i64)
            .bind(dt_opt_str(v.last_synced_at))
            .bind(v.origin_device_id.as_deref())
            .bind(&v.entity_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Visit>> {
        let sql = format!("SELECT {COLUMNS} FROM visits WHERE id = ?");
        let row: Option<VisitRow> = sqlx::query_as::<_, VisitRow>(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(VisitRow::into_domain).transpose()
    }

    async fn get_by_id_tx(&self, tx: &mut Tx<'_>, id: Uuid) -> AppResult<Option<Visit>> {
        let sql = format!("SELECT {COLUMNS} FROM visits WHERE id = ?");
        let row: Option<VisitRow> = sqlx::query_as::<_, VisitRow>(&sql)
            .bind(id.to_string())
            .fetch_optional(&mut **tx)
            .await?;
        row.map(VisitRow::into_domain).transpose()
    }

    async fn list_today_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> AppResult<Vec<Visit>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM visits \
             WHERE entity_id = ? AND check_type_id = ? AND deleted_at IS NULL \
               AND created_at >= ? AND created_at < ? \
             ORDER BY created_at DESC"
        );
        let rows = sqlx::query_as::<_, VisitRow>(&sql)
            .bind(entity_id)
            .bind(check_type_id.to_string())
            .bind(dt_str(day_start))
            .bind(dt_str(day_end))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(VisitRow::into_domain).collect()
    }

    async fn list_drafts_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
    ) -> AppResult<Vec<Visit>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM visits \
             WHERE entity_id = ? AND check_type_id = ? \
               AND status = 'draft' AND deleted_at IS NULL \
             ORDER BY created_at DESC"
        );
        let rows = sqlx::query_as::<_, VisitRow>(&sql)
            .bind(entity_id)
            .bind(check_type_id.to_string())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(VisitRow::into_domain).collect()
    }

    async fn list_workspace(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        filters: &WorkspaceFilters,
        limit: i64,
    ) -> AppResult<Vec<Visit>> {
        let mut sql = format!(
            "SELECT {COLUMNS} FROM visits \
             WHERE entity_id = ? AND check_type_id = ? AND deleted_at IS NULL"
        );
        let mut binds: Vec<String> = vec![entity_id.to_string(), check_type_id.to_string()];

        if !filters.statuses.is_empty() {
            let placeholders = filters
                .statuses
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND status IN ({placeholders})"));
            for s in &filters.statuses {
                binds.push(s.as_str().into());
            }
        }
        if !filters.doctor_ids.is_empty() {
            let placeholders = filters
                .doctor_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND doctor_id IN ({placeholders})"));
            for d in &filters.doctor_ids {
                binds.push(d.to_string());
            }
        }
        if !filters.subtype_ids.is_empty() {
            let placeholders = filters
                .subtype_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND check_subtype_id IN ({placeholders})"));
            for s in &filters.subtype_ids {
                binds.push(s.to_string());
            }
        }
        if let Some(from) = filters.from {
            sql.push_str(" AND created_at >= ?");
            binds.push(dt_str(from));
        }
        if let Some(to) = filters.to {
            sql.push_str(" AND created_at < ?");
            binds.push(dt_str(to));
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT ?");
        let mut q = sqlx::query_as::<_, VisitRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q.bind(limit).fetch_all(&self.pool).await?;
        rows.into_iter().map(VisitRow::into_domain).collect()
    }

    async fn count_today_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM visits \
             WHERE entity_id = ? AND check_type_id = ? AND deleted_at IS NULL \
               AND created_at >= ? AND created_at < ?",
        )
        .bind(entity_id)
        .bind(check_type_id.to_string())
        .bind(dt_str(day_start))
        .bind(dt_str(day_end))
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    async fn lines_run_today_by_operator(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM visits \
             WHERE entity_id = ? AND operator_id = ? AND deleted_at IS NULL \
               AND status = 'locked' \
               AND locked_at >= ? AND locked_at < ?",
        )
        .bind(entity_id)
        .bind(operator_id.to_string())
        .bind(dt_str(day_start))
        .bind(dt_str(day_end))
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}
