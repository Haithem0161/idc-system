//! SQLite implementation of `ReportsReadModel`. All money aggregations read
//! from `visits.*_snapshot_iqd` exclusively per PRD §4.1.

use async_trait::async_trait;
#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domains::reports::domain::entities::{
    CheckTypeDailyRow, DoctorDailyRow, DoctorEarningsRow, DoctorPerCheckRow, OperatorDailyRow,
    OperatorEarningsRow, OperatorShiftRow, VisitRow, VisitsReportFilters, VisitsReportGroup,
    VisitsReportGroupBy,
};
use crate::domains::reports::domain::repositories::{
    ReportsReadModel, VisitsAggregate, VoidedAggregate,
};
use crate::error::{AppError, AppResult};

fn dt_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

fn parse_uuid(s: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|e| AppError::Validation(format!("uuid: {e}")))
}

fn parse_uuid_opt(s: Option<String>) -> AppResult<Option<Uuid>> {
    s.map(|x| parse_uuid(&x)).transpose()
}

fn parse_dt_opt(s: Option<String>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(|x| parse_dt(&x)).transpose()
}

#[derive(Clone)]
pub struct SqliteReportsReadModel {
    pool: SqlitePool,
}

impl SqliteReportsReadModel {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Snapshot column list used by every row query for consistency.
const VISIT_ROW_SELECT: &str = "v.id AS visit_id, \
    v.locked_at, \
    v.status, \
    v.dye, \
    v.report, \
    COALESCE(v.patient_name_snapshot, p.name) AS patient_name, \
    COALESCE(v.check_type_name_ar_snapshot, ct.name_ar) AS ct_ar, \
    COALESCE(v.check_type_name_en_snapshot, ct.name_en) AS ct_en, \
    COALESCE(v.check_subtype_name_ar_snapshot, cs.name_ar) AS cs_ar, \
    COALESCE(v.check_subtype_name_en_snapshot, cs.name_en) AS cs_en, \
    v.doctor_name_snapshot AS doctor_name, \
    COALESCE(v.operator_name_snapshot, o.name) AS operator_name, \
    COALESCE(v.price_snapshot_iqd, 0) AS price, \
    COALESCE(v.doctor_cut_snapshot_iqd, 0) AS doc_cut, \
    COALESCE(v.operator_cut_snapshot_iqd, 0) AS op_cut, \
    COALESCE(v.total_amount_iqd_snapshot, 0) AS total_iqd";

const VISIT_ROW_JOINS: &str = "FROM visits v \
    LEFT JOIN patients p ON p.id = v.patient_id \
    LEFT JOIN check_types ct ON ct.id = v.check_type_id \
    LEFT JOIN check_subtypes cs ON cs.id = v.check_subtype_id \
    LEFT JOIN operators o ON o.id = v.operator_id";

fn status_clause(include_voided: bool) -> &'static str {
    if include_voided {
        "(v.status = 'locked' OR v.status = 'voided')"
    } else {
        "v.status = 'locked'"
    }
}

#[derive(sqlx::FromRow)]
struct VisitRowRaw {
    visit_id: String,
    locked_at: Option<String>,
    status: String,
    dye: i64,
    report: i64,
    patient_name: Option<String>,
    ct_ar: Option<String>,
    ct_en: Option<String>,
    cs_ar: Option<String>,
    cs_en: Option<String>,
    doctor_name: Option<String>,
    operator_name: Option<String>,
    price: i64,
    doc_cut: i64,
    op_cut: i64,
    total_iqd: i64,
}

impl VisitRowRaw {
    fn into_domain(self) -> AppResult<VisitRow> {
        let price = self.price;
        let dc = self.doc_cut;
        let oc = self.op_cut;
        Ok(VisitRow {
            visit_id: parse_uuid(&self.visit_id)?,
            locked_at: parse_dt_opt(self.locked_at)?,
            status: self.status,
            dye: self.dye != 0,
            report: self.report != 0,
            patient_name: self.patient_name.unwrap_or_default(),
            check_type_name_ar: self.ct_ar.unwrap_or_default(),
            check_type_name_en: self.ct_en,
            check_subtype_name_ar: self.cs_ar,
            check_subtype_name_en: self.cs_en,
            doctor_name: self.doctor_name,
            operator_name: self.operator_name.unwrap_or_default(),
            price_iqd: price,
            doctor_cut_iqd: dc,
            operator_cut_iqd: oc,
            net_iqd: self.total_iqd.saturating_sub(dc).saturating_sub(oc),
        })
    }
}

#[async_trait]
impl ReportsReadModel for SqliteReportsReadModel {
    async fn aggregate_visits(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<VisitsAggregate> {
        // Dashboard / Daily-close "revenue" = SUM(total_amount_iqd_snapshot)
        // per PRD §7.2.1 ("sum of locked visit totals"). The Visits Report
        // per-row Price column is price_snapshot_iqd; that column's footer
        // is computed at the row layer (sum_visit_rows).
        let sql = format!(
            "SELECT \
                COUNT(*) AS visits, \
                COALESCE(SUM(total_amount_iqd_snapshot), 0) AS revenue, \
                COALESCE(SUM(doctor_cut_snapshot_iqd), 0) AS dc, \
                COALESCE(SUM(operator_cut_snapshot_iqd), 0) AS oc \
             FROM visits v \
             WHERE v.entity_id = ? AND v.deleted_at IS NULL \
               AND {status} \
               AND v.locked_at >= ? AND v.locked_at < ?",
            status = status_clause(include_voided)
        );
        let row: (i64, i64, i64, i64) = sqlx::query_as(&sql)
            .bind(entity_id)
            .bind(dt_str(from))
            .bind(dt_str(to))
            .fetch_one(&self.pool)
            .await?;
        Ok(VisitsAggregate {
            visits: row.0,
            revenue_iqd: row.1,
            doctor_cut_iqd: row.2,
            operator_cut_iqd: row.3,
        })
    }

    async fn list_visit_rows(&self, filters: &VisitsReportFilters) -> AppResult<Vec<VisitRow>> {
        let (sql, binds) = build_visit_rows_query(filters);
        let mut q = sqlx::query_as::<_, VisitRowRaw>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(VisitRowRaw::into_domain).collect()
    }

    async fn list_visit_groups(
        &self,
        filters: &VisitsReportFilters,
    ) -> AppResult<Vec<VisitsReportGroup>> {
        // Build the same WHERE clause as the rows query, then GROUP BY the
        // chosen column. Rust-side post-processing fills the label so we can
        // localize names without round-tripping snapshots.
        let where_block = build_where_clause(filters);
        let group_expr = match filters.group_by {
            VisitsReportGroupBy::None => return Ok(vec![]),
            VisitsReportGroupBy::ByDate => "substr(v.locked_at, 1, 10)".to_string(),
            VisitsReportGroupBy::ByDoctor => "COALESCE(v.doctor_id, '__house__')".to_string(),
            VisitsReportGroupBy::ByOperator => "COALESCE(v.operator_id, '')".to_string(),
            VisitsReportGroupBy::ByCheckType => "v.check_type_id".to_string(),
            VisitsReportGroupBy::BySubtype => "COALESCE(v.check_subtype_id, '')".to_string(),
            VisitsReportGroupBy::ByStatus => "v.status".to_string(),
        };
        let label_expr = match filters.group_by {
            VisitsReportGroupBy::None => "''".to_string(),
            VisitsReportGroupBy::ByDate => "substr(v.locked_at, 1, 10)".to_string(),
            VisitsReportGroupBy::ByDoctor => {
                "COALESCE(v.doctor_name_snapshot, '(house)')".to_string()
            }
            VisitsReportGroupBy::ByOperator => "COALESCE(v.operator_name_snapshot, '')".to_string(),
            VisitsReportGroupBy::ByCheckType => {
                "COALESCE(v.check_type_name_en_snapshot, v.check_type_name_ar_snapshot)".to_string()
            }
            VisitsReportGroupBy::BySubtype => {
                "COALESCE(v.check_subtype_name_en_snapshot, v.check_subtype_name_ar_snapshot, '')"
                    .to_string()
            }
            VisitsReportGroupBy::ByStatus => "v.status".to_string(),
        };
        let sql = format!(
            "SELECT {group_expr} AS group_key, \
                    MIN({label_expr}) AS group_label, \
                    COUNT(*) AS visits, \
                    COALESCE(SUM(total_amount_iqd_snapshot), 0) AS revenue, \
                    COALESCE(SUM(doctor_cut_snapshot_iqd), 0) AS dc, \
                    COALESCE(SUM(operator_cut_snapshot_iqd), 0) AS oc \
             FROM visits v \
             {where_block} \
             GROUP BY {group_expr} \
             ORDER BY revenue DESC, group_label ASC"
        );
        let (_, binds) = build_where_binds(filters);
        let mut q = sqlx::query_as::<_, (String, Option<String>, i64, i64, i64, i64)>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|(k, label, visits, revenue, dc, oc)| VisitsReportGroup {
                key: k.clone(),
                label: label.unwrap_or(k),
                visits,
                revenue_iqd: revenue,
                doctor_cut_iqd: dc,
                operator_cut_iqd: oc,
                net_iqd: revenue.saturating_sub(dc).saturating_sub(oc),
            })
            .collect())
    }

    async fn aggregate_doctor_earnings(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<DoctorEarningsRow>> {
        // House (doctor_id IS NULL) is always returned as a pseudo-row.
        let sql = format!(
            "SELECT v.doctor_id, \
                    COALESCE(v.doctor_name_snapshot, d.name, '') AS name, \
                    d.specialty AS specialty, \
                    COUNT(*) AS visits, \
                    COALESCE(SUM(v.total_amount_iqd_snapshot), 0) AS revenue, \
                    COALESCE(SUM(v.doctor_cut_snapshot_iqd), 0) AS dc \
             FROM visits v \
             LEFT JOIN doctors d ON d.id = v.doctor_id \
             WHERE v.entity_id = ? AND v.deleted_at IS NULL \
               AND {status} \
               AND v.locked_at >= ? AND v.locked_at < ? \
             GROUP BY v.doctor_id, d.specialty \
             ORDER BY revenue DESC, name ASC",
            status = status_clause(include_voided)
        );
        let rows: Vec<(Option<String>, String, Option<String>, i64, i64, i64)> =
            sqlx::query_as(&sql)
                .bind(entity_id)
                .bind(dt_str(from))
                .bind(dt_str(to))
                .fetch_all(&self.pool)
                .await?;
        let mut out: Vec<DoctorEarningsRow> = Vec::with_capacity(rows.len());
        for (doctor_id, name, specialty, visits, revenue, dc) in rows {
            let id = parse_uuid_opt(doctor_id)?;
            let final_name = if id.is_none() && name.is_empty() {
                "(house)".to_string()
            } else if name.is_empty() {
                "(unknown)".to_string()
            } else {
                name
            };
            let avg = if visits > 0 { dc / visits } else { 0 };
            out.push(DoctorEarningsRow {
                doctor_id: id,
                name: final_name,
                specialty,
                visits,
                revenue_iqd: revenue,
                doctor_cut_total_iqd: dc,
                avg_cut_per_visit_iqd: avg,
            });
        }
        Ok(out)
    }

    async fn doctor_per_check(
        &self,
        entity_id: &str,
        doctor_id: Option<Uuid>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<DoctorPerCheckRow>> {
        let doctor_filter = if doctor_id.is_some() {
            "v.doctor_id = ?"
        } else {
            "v.doctor_id IS NULL"
        };
        let sql = format!(
            "SELECT v.check_type_id, \
                    v.check_subtype_id, \
                    COALESCE(v.check_type_name_ar_snapshot, ct.name_ar) AS ct_ar, \
                    COALESCE(v.check_type_name_en_snapshot, ct.name_en) AS ct_en, \
                    COALESCE(v.check_subtype_name_ar_snapshot, cs.name_ar) AS cs_ar, \
                    COALESCE(v.check_subtype_name_en_snapshot, cs.name_en) AS cs_en, \
                    COUNT(*) AS visits, \
                    COALESCE(SUM(v.total_amount_iqd_snapshot), 0) AS revenue, \
                    COALESCE(SUM(v.doctor_cut_snapshot_iqd), 0) AS dc \
             FROM visits v \
             LEFT JOIN check_types ct ON ct.id = v.check_type_id \
             LEFT JOIN check_subtypes cs ON cs.id = v.check_subtype_id \
             WHERE v.entity_id = ? AND v.deleted_at IS NULL \
               AND {status} \
               AND v.locked_at >= ? AND v.locked_at < ? \
               AND {doctor_filter} \
             GROUP BY v.check_type_id, v.check_subtype_id \
             ORDER BY revenue DESC",
            status = status_clause(include_voided)
        );
        let mut q = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                i64,
                i64,
                i64,
            ),
        >(&sql);
        q = q.bind(entity_id).bind(dt_str(from)).bind(dt_str(to));
        if let Some(d) = doctor_id {
            q = q.bind(d.to_string());
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(
                |(ct_id, sub_id, ct_ar, ct_en, cs_ar, cs_en, visits, revenue, dc)| {
                    let avg = if visits > 0 { dc / visits } else { 0 };
                    Ok(DoctorPerCheckRow {
                        check_type_id: parse_uuid(&ct_id)?,
                        check_type_name_ar: ct_ar.unwrap_or_default(),
                        check_type_name_en: ct_en,
                        check_subtype_id: parse_uuid_opt(sub_id)?,
                        check_subtype_name_ar: cs_ar,
                        check_subtype_name_en: cs_en,
                        visits,
                        revenue_iqd: revenue,
                        doctor_cut_iqd: dc,
                        avg_cut_iqd: avg,
                    })
                },
            )
            .collect()
    }

    async fn doctor_source_visits(
        &self,
        entity_id: &str,
        doctor_id: Option<Uuid>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<VisitRow>> {
        let doctor_filter = if doctor_id.is_some() {
            "v.doctor_id = ?"
        } else {
            "v.doctor_id IS NULL"
        };
        let sql = format!(
            "SELECT {VISIT_ROW_SELECT} \
             {VISIT_ROW_JOINS} \
             WHERE v.entity_id = ? AND v.deleted_at IS NULL \
               AND {status} \
               AND v.locked_at >= ? AND v.locked_at < ? \
               AND {doctor_filter} \
             ORDER BY v.locked_at DESC, v.id DESC \
             LIMIT 500",
            status = status_clause(include_voided)
        );
        let mut q = sqlx::query_as::<_, VisitRowRaw>(&sql);
        q = q.bind(entity_id).bind(dt_str(from)).bind(dt_str(to));
        if let Some(d) = doctor_id {
            q = q.bind(d.to_string());
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(VisitRowRaw::into_domain).collect()
    }

    async fn aggregate_operator_earnings(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<OperatorEarningsRow>> {
        // Aggregate per operator + hours-on-shift in a single pass via a
        // LEFT JOIN onto the operator-shifts window aggregate.
        let sql = format!(
            "WITH locked AS ( \
                SELECT v.operator_id, \
                       COALESCE(v.operator_name_snapshot, o.name, '') AS name, \
                       v.dye, \
                       v.operator_cut_snapshot_iqd AS op_cut \
                FROM visits v \
                LEFT JOIN operators o ON o.id = v.operator_id \
                WHERE v.entity_id = ? AND v.deleted_at IS NULL \
                  AND v.operator_id IS NOT NULL \
                  AND {status} \
                  AND v.locked_at >= ? AND v.locked_at < ? \
             ), shift_hours AS ( \
                SELECT operator_id, \
                       SUM( \
                          (julianday(COALESCE(check_out_at, ?)) \
                           - julianday(check_in_at)) * 86400000.0 \
                       ) AS millis \
                FROM operator_shifts \
                WHERE entity_id = ? AND deleted_at IS NULL \
                  AND check_in_at < ? \
                  AND (check_out_at IS NULL OR check_out_at >= ?) \
                GROUP BY operator_id \
             ) \
             SELECT l.operator_id, \
                    MIN(l.name) AS name, \
                    COUNT(*) AS visits, \
                    SUM(CASE WHEN l.dye = 1 THEN 1 ELSE 0 END) AS dye_visits, \
                    COALESCE(SUM(l.op_cut), 0) AS oc_total, \
                    COALESCE(MIN(sh.millis), 0) AS hours_milli \
             FROM locked l \
             LEFT JOIN shift_hours sh ON sh.operator_id = l.operator_id \
             GROUP BY l.operator_id \
             ORDER BY oc_total DESC, name ASC",
            status = status_clause(include_voided)
        );
        let rows: Vec<(String, String, i64, i64, i64, f64)> = sqlx::query_as(&sql)
            .bind(entity_id)
            .bind(dt_str(from))
            .bind(dt_str(to))
            .bind(dt_str(to)) // shift_hours upper-bound for open shifts
            .bind(entity_id)
            .bind(dt_str(to))
            .bind(dt_str(from))
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (operator_id, name, visits, dye_visits, oc_total, hours_milli) in rows {
            let id = parse_uuid(&operator_id)?;
            let milli = hours_milli as i64;
            let avg_per_hour = if milli > 0 {
                oc_total
                    .saturating_mul(3_600_000)
                    .checked_div(milli)
                    .unwrap_or(0)
            } else {
                0
            };
            out.push(OperatorEarningsRow {
                operator_id: id,
                name,
                visits,
                visits_with_dye: dye_visits,
                operator_cut_total_iqd: oc_total,
                hours_on_shift_milli: milli,
                avg_cut_per_hour_iqd: avg_per_hour,
            });
        }
        Ok(out)
    }

    async fn operator_shifts_window(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<OperatorShiftRow>> {
        // Lines run = locked visits in [check_in, check_out|to). Compute via
        // a correlated subquery so each shift gets its own count.
        let sql = "SELECT s.id, s.check_in_at, s.check_out_at, \
                          (SELECT COUNT(*) FROM visits v2 \
                             WHERE v2.entity_id = s.entity_id \
                               AND v2.operator_id = s.operator_id \
                               AND v2.deleted_at IS NULL \
                               AND v2.status = 'locked' \
                               AND v2.locked_at >= s.check_in_at \
                               AND v2.locked_at < COALESCE(s.check_out_at, ?)) AS lines_run, \
                          COALESCE((SELECT SUM(v2.operator_cut_snapshot_iqd) FROM visits v2 \
                             WHERE v2.entity_id = s.entity_id \
                               AND v2.operator_id = s.operator_id \
                               AND v2.deleted_at IS NULL \
                               AND v2.status = 'locked' \
                               AND v2.locked_at >= s.check_in_at \
                               AND v2.locked_at < COALESCE(s.check_out_at, ?)), 0) AS cut \
                   FROM operator_shifts s \
                   WHERE s.entity_id = ? AND s.operator_id = ? AND s.deleted_at IS NULL \
                     AND s.check_in_at < ? \
                     AND (s.check_out_at IS NULL OR s.check_out_at >= ?) \
                   ORDER BY s.check_in_at DESC";
        let rows: Vec<(String, String, Option<String>, i64, i64)> = sqlx::query_as(sql)
            .bind(dt_str(to))
            .bind(dt_str(to))
            .bind(entity_id)
            .bind(operator_id.to_string())
            .bind(dt_str(to))
            .bind(dt_str(from))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(id, check_in, check_out, lines, cut)| {
                let check_in_at = parse_dt(&check_in)?;
                let check_out_at = parse_dt_opt(check_out)?;
                let duration_milli = check_out_at.map(|c| (c - check_in_at).num_milliseconds());
                Ok(OperatorShiftRow {
                    shift_id: parse_uuid(&id)?,
                    check_in_at,
                    check_out_at,
                    duration_milli,
                    lines_run: lines,
                    cut_earned_iqd: cut,
                })
            })
            .collect()
    }

    async fn operator_source_visits(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<Vec<VisitRow>> {
        let sql = format!(
            "SELECT {VISIT_ROW_SELECT} \
             {VISIT_ROW_JOINS} \
             WHERE v.entity_id = ? AND v.deleted_at IS NULL \
               AND {status} \
               AND v.locked_at >= ? AND v.locked_at < ? \
               AND v.operator_id = ? \
             ORDER BY v.locked_at DESC, v.id DESC \
             LIMIT 500",
            status = status_clause(include_voided)
        );
        let rows: Vec<VisitRowRaw> = sqlx::query_as(&sql)
            .bind(entity_id)
            .bind(dt_str(from))
            .bind(dt_str(to))
            .bind(operator_id.to_string())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(VisitRowRaw::into_domain).collect()
    }

    async fn daily_per_doctor(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<DoctorDailyRow>> {
        let sql = "SELECT v.doctor_id, \
                          COALESCE(v.doctor_name_snapshot, d.name, '(house)') AS name, \
                          COUNT(*) AS visits, \
                          COALESCE(SUM(v.total_amount_iqd_snapshot), 0) AS revenue, \
                          COALESCE(SUM(v.doctor_cut_snapshot_iqd), 0) AS dc \
                   FROM visits v \
                   LEFT JOIN doctors d ON d.id = v.doctor_id \
                   WHERE v.entity_id = ? AND v.deleted_at IS NULL AND v.status = 'locked' \
                     AND v.locked_at >= ? AND v.locked_at < ? \
                   GROUP BY v.doctor_id \
                   ORDER BY revenue DESC";
        let rows: Vec<(Option<String>, String, i64, i64, i64)> = sqlx::query_as(sql)
            .bind(entity_id)
            .bind(dt_str(from))
            .bind(dt_str(to))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(doctor_id, name, visits, revenue, dc)| {
                Ok(DoctorDailyRow {
                    doctor_id: parse_uuid_opt(doctor_id)?,
                    name,
                    visits,
                    revenue_iqd: revenue,
                    doctor_cut_iqd: dc,
                })
            })
            .collect()
    }

    async fn daily_per_operator(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<OperatorDailyRow>> {
        let sql = "SELECT v.operator_id, \
                          COALESCE(v.operator_name_snapshot, o.name, '') AS name, \
                          COUNT(*) AS visits, \
                          SUM(CASE WHEN v.dye = 1 THEN 1 ELSE 0 END) AS dye_visits, \
                          COALESCE(SUM(v.operator_cut_snapshot_iqd), 0) AS oc, \
                          COALESCE(( \
                            SELECT SUM( \
                              (julianday(COALESCE(s.check_out_at, ?)) - julianday(s.check_in_at)) \
                              * 86400000.0 \
                            ) \
                            FROM operator_shifts s \
                            WHERE s.entity_id = v.entity_id \
                              AND s.operator_id = v.operator_id \
                              AND s.deleted_at IS NULL \
                              AND s.check_in_at < ? \
                              AND (s.check_out_at IS NULL OR s.check_out_at >= ?) \
                          ), 0) AS hours_milli \
                   FROM visits v \
                   LEFT JOIN operators o ON o.id = v.operator_id \
                   WHERE v.entity_id = ? AND v.deleted_at IS NULL AND v.status = 'locked' \
                     AND v.operator_id IS NOT NULL \
                     AND v.locked_at >= ? AND v.locked_at < ? \
                   GROUP BY v.operator_id \
                   ORDER BY oc DESC";
        let rows: Vec<(String, String, i64, i64, i64, f64)> = sqlx::query_as(sql)
            .bind(dt_str(to))
            .bind(dt_str(to))
            .bind(dt_str(from))
            .bind(entity_id)
            .bind(dt_str(from))
            .bind(dt_str(to))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(op_id, name, visits, dye, oc, hours_milli)| {
                Ok(OperatorDailyRow {
                    operator_id: parse_uuid(&op_id)?,
                    name,
                    visits,
                    dye_visits: dye,
                    operator_cut_iqd: oc,
                    hours_on_shift_milli: hours_milli as i64,
                })
            })
            .collect()
    }

    async fn daily_per_check_type(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<CheckTypeDailyRow>> {
        let sql = "SELECT v.check_type_id, \
                          COALESCE(v.check_type_name_ar_snapshot, ct.name_ar, '') AS ar, \
                          COALESCE(v.check_type_name_en_snapshot, ct.name_en) AS en, \
                          COUNT(*) AS visits, \
                          COALESCE(SUM(v.total_amount_iqd_snapshot), 0) AS revenue, \
                          COALESCE(SUM(v.doctor_cut_snapshot_iqd), 0) AS dc, \
                          COALESCE(SUM(v.operator_cut_snapshot_iqd), 0) AS oc \
                   FROM visits v \
                   LEFT JOIN check_types ct ON ct.id = v.check_type_id \
                   WHERE v.entity_id = ? AND v.deleted_at IS NULL AND v.status = 'locked' \
                     AND v.locked_at >= ? AND v.locked_at < ? \
                   GROUP BY v.check_type_id \
                   ORDER BY revenue DESC";
        let rows: Vec<(String, String, Option<String>, i64, i64, i64, i64)> = sqlx::query_as(sql)
            .bind(entity_id)
            .bind(dt_str(from))
            .bind(dt_str(to))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(ct_id, ar, en, visits, revenue, dc, oc)| {
                Ok(CheckTypeDailyRow {
                    check_type_id: parse_uuid(&ct_id)?,
                    name_ar: ar,
                    name_en: en,
                    visits,
                    revenue_iqd: revenue,
                    doctor_cut_iqd: dc,
                    operator_cut_iqd: oc,
                })
            })
            .collect()
    }

    async fn inventory_consumption_value(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<i64> {
        // consume_visit deltas are negative; -SUM(delta) yields the
        // consumption quantity over the window. The value field is unit-cost
        // = 1 IQD per consumed unit in v1 (Horizon-1 adds a per-item cost).
        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(-delta) FROM inventory_adjustments \
             WHERE entity_id = ? AND deleted_at IS NULL \
               AND reason = 'consume_visit' \
               AND created_at >= ? AND created_at < ?",
        )
        .bind(entity_id)
        .bind(dt_str(from))
        .bind(dt_str(to))
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0.unwrap_or(0))
    }

    async fn outbox_count(&self) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    async fn voided_aggregate(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<VoidedAggregate> {
        // Voided rows are still bracketed by their original locked_at; this
        // means the report shows the void's monetary value against the day
        // the visit was originally locked, per PRD §8.4 step 2.
        let row: (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*) AS c, COALESCE(SUM(total_amount_iqd_snapshot), 0) AS v \
             FROM visits \
             WHERE entity_id = ? AND deleted_at IS NULL AND status = 'voided' \
               AND locked_at >= ? AND locked_at < ?",
        )
        .bind(entity_id)
        .bind(dt_str(from))
        .bind(dt_str(to))
        .fetch_one(&self.pool)
        .await?;
        Ok(VoidedAggregate {
            count: row.0,
            value_iqd: row.1,
        })
    }

    async fn daily_visit_ids(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> AppResult<Vec<Uuid>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM visits \
             WHERE entity_id = ? AND deleted_at IS NULL \
               AND (status = 'locked' OR status = 'voided') \
               AND locked_at >= ? AND locked_at < ? \
             ORDER BY id ASC",
        )
        .bind(entity_id)
        .bind(dt_str(from))
        .bind(dt_str(to))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|(s,)| parse_uuid(&s)).collect()
    }
}

// ---- query builders -------------------------------------------------------

fn build_where_clause(filters: &VisitsReportFilters) -> String {
    let (clause, _binds) = build_where_binds(filters);
    clause
}

fn build_where_binds(filters: &VisitsReportFilters) -> (String, Vec<String>) {
    let mut binds: Vec<String> = Vec::new();
    let mut where_block = "WHERE v.entity_id = ? AND v.deleted_at IS NULL".to_string();
    binds.push(filters.entity_id.clone());

    // Status set: derived from `include_voided` if not explicit.
    if filters.statuses.is_empty() {
        if filters.include_voided {
            where_block.push_str(" AND (v.status = 'locked' OR v.status = 'voided')");
        } else {
            where_block.push_str(" AND v.status = 'locked'");
        }
    } else {
        let placeholders = filters
            .statuses
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        where_block.push_str(&format!(" AND v.status IN ({placeholders})"));
        for s in &filters.statuses {
            binds.push(s.clone());
        }
    }

    where_block.push_str(" AND v.locked_at >= ? AND v.locked_at < ?");
    binds.push(filters.from.to_rfc3339());
    binds.push(filters.to.to_rfc3339());

    if !filters.check_type_ids.is_empty() {
        let placeholders = filters
            .check_type_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        where_block.push_str(&format!(" AND v.check_type_id IN ({placeholders})"));
        for id in &filters.check_type_ids {
            binds.push(id.to_string());
        }
    }
    if !filters.subtype_ids.is_empty() {
        let placeholders = filters
            .subtype_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        where_block.push_str(&format!(" AND v.check_subtype_id IN ({placeholders})"));
        for id in &filters.subtype_ids {
            binds.push(id.to_string());
        }
    }
    if !filters.doctor_ids.is_empty() || filters.include_house {
        let mut clauses: Vec<String> = Vec::new();
        if !filters.doctor_ids.is_empty() {
            let placeholders = filters
                .doctor_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            clauses.push(format!("v.doctor_id IN ({placeholders})"));
            for id in &filters.doctor_ids {
                binds.push(id.to_string());
            }
        }
        if filters.include_house {
            clauses.push("v.doctor_id IS NULL".to_string());
        }
        if !clauses.is_empty() {
            where_block.push_str(&format!(" AND ({})", clauses.join(" OR ")));
        }
    }
    if !filters.operator_ids.is_empty() {
        let placeholders = filters
            .operator_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        where_block.push_str(&format!(" AND v.operator_id IN ({placeholders})"));
        for id in &filters.operator_ids {
            binds.push(id.to_string());
        }
    }
    if let Some(b) = filters.dye {
        where_block.push_str(" AND v.dye = ?");
        binds.push(if b { "1".into() } else { "0".into() });
    }
    if let Some(b) = filters.report {
        where_block.push_str(" AND v.report = ?");
        binds.push(if b { "1".into() } else { "0".into() });
    }

    (where_block, binds)
}

fn build_visit_rows_query(filters: &VisitsReportFilters) -> (String, Vec<String>) {
    let (where_block, binds) = build_where_binds(filters);
    let limit = filters.limit.unwrap_or(2000).clamp(1, 10_000);
    let sql = format!(
        "SELECT {VISIT_ROW_SELECT} {VISIT_ROW_JOINS} {where_block} \
         ORDER BY v.locked_at DESC, v.id DESC \
         LIMIT {limit}"
    );
    (sql, binds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_clause_locked_default() {
        let f = VisitsReportFilters {
            from: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap(),
            entity_id: "t1".into(),
            ..Default::default()
        };
        let (sql, binds) = build_where_binds(&f);
        assert!(sql.contains("v.status = 'locked'"));
        assert_eq!(binds[0], "t1");
    }

    #[test]
    fn include_voided_flag() {
        let f = VisitsReportFilters {
            from: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap(),
            include_voided: true,
            entity_id: "t1".into(),
            ..Default::default()
        };
        let (sql, _) = build_where_binds(&f);
        assert!(sql.contains("'locked' OR v.status = 'voided'"));
    }
}
