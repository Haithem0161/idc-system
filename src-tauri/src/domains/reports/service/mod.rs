//! Application service for the reports bounded context.
//!
//! Reports are read-only: this module assembles the view models for the
//! frontend without writing to any business table. The single exception is
//! the `daily_close` IPC, which emits one audit_log row per run (§7.18).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::reports::domain::entities::{
    CheckTypeDailyRow, DailyClose, DashboardKpis, DashboardTops, DateRange, DoctorDailyRow,
    DoctorDrilldown, DoctorEarningsRow, OperatorDailyRow, OperatorDrilldown, OperatorEarningsRow,
    OperatorShiftRow, TrendMatrix, VisitRow, VisitsReport, VisitsReportFilters,
    VisitsReportGroupBy, VisitsReportTotals,
};
use crate::domains::reports::domain::entities::{FrozenClose, FrozenCloseNewInput};
use crate::domains::reports::domain::repositories::{FrozenCloseRepo, ReportsReadModel};
use crate::domains::reports::domain::services::input_hash::DailyCloseHashInput;
use crate::domains::reports::domain::services::money_trend::{trend_cell, TrendInputs};
use crate::domains::reports::domain::services::{
    baghdad_offset_seconds, compute_input_hash, local_day_utc_range, write_doctor_earnings_csv,
    write_operator_earnings_csv, write_visits_csv,
};
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::encode_audit_payload;
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

/// Maximum range the v1 desktop client serves locally (§7.16: ranges beyond
/// this clamp to 90 days unless an explicit "Authoritative" toggle on Daily
/// Close routes to the server endpoint).
pub const MAX_LOCAL_RANGE_DAYS: i64 = 90;

#[derive(Clone)]
pub struct ReportsServiceConfig {
    pub pool: SqlitePool,
    pub read_model: Arc<dyn ReportsReadModel>,
    pub frozen_close_repo: Arc<dyn FrozenCloseRepo>,
    pub audit_repo: Arc<dyn AuditRepo>,
    pub outbox_repo: Arc<dyn OutboxRepo>,
    pub device_id: String,
}

#[derive(Clone)]
pub struct ReportsService {
    pool: SqlitePool,
    read: Arc<dyn ReportsReadModel>,
    frozen: Arc<dyn FrozenCloseRepo>,
    audit: Arc<dyn AuditRepo>,
    outbox: Arc<dyn OutboxRepo>,
    device_id: String,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct RangeArgs {
    pub from_utc: DateTime<Utc>,
    pub to_utc: DateTime<Utc>,
}

impl From<RangeArgs> for DateRange {
    fn from(r: RangeArgs) -> Self {
        Self {
            from_utc: r.from_utc,
            to_utc: r.to_utc,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub path: PathBuf,
}

/// JSON push payload for a frozen daily close. Field names mirror the
/// sync-server `DailyCloseSyncRecord` / Prisma model exactly.
#[derive(Debug, Serialize)]
pub struct FrozenClosePushPayload {
    pub id: String,
    pub target_date: String,
    pub tz_offset: String,
    pub input_hash: String,
    pub total_revenue_iqd: i64,
    pub total_collected_iqd: i64,
    pub total_discount_iqd: i64,
    pub total_doctor_cuts_iqd: i64,
    pub total_operator_cuts_iqd: i64,
    pub total_inventory_consumption_value_iqd: i64,
    pub net_iqd: i64,
    pub locked_count: i64,
    pub voided_count: i64,
    pub voided_value_iqd: i64,
    pub signed_by_user_id: String,
    pub signed_by_name: String,
    pub signed_at: String,
    pub reopened_at: Option<String>,
    pub reopened_by_user_id: Option<String>,
    pub reopen_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub version: i64,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

impl From<&FrozenClose> for FrozenClosePushPayload {
    fn from(c: &FrozenClose) -> Self {
        Self {
            id: c.id.to_string(),
            target_date: c.target_date.format("%Y-%m-%d").to_string(),
            tz_offset: c.tz_offset.clone(),
            input_hash: c.input_hash.clone(),
            total_revenue_iqd: c.total_revenue_iqd,
            total_collected_iqd: c.total_collected_iqd,
            total_discount_iqd: c.total_discount_iqd,
            total_doctor_cuts_iqd: c.total_doctor_cuts_iqd,
            total_operator_cuts_iqd: c.total_operator_cuts_iqd,
            total_inventory_consumption_value_iqd: c.total_inventory_consumption_value_iqd,
            net_iqd: c.net_iqd,
            locked_count: c.locked_count,
            voided_count: c.voided_count,
            voided_value_iqd: c.voided_value_iqd,
            signed_by_user_id: c.signed_by_user_id.to_string(),
            signed_by_name: c.signed_by_name.clone(),
            signed_at: c.signed_at.to_rfc3339(),
            reopened_at: c.reopened_at.map(|d| d.to_rfc3339()),
            reopened_by_user_id: c.reopened_by_user_id.map(|u| u.to_string()),
            reopen_reason: c.reopen_reason.clone(),
            created_at: c.created_at.to_rfc3339(),
            updated_at: c.updated_at.to_rfc3339(),
            deleted_at: None,
            version: c.version,
            origin_device_id: c.origin_device_id.clone(),
            entity_id: c.entity_id.clone(),
        }
    }
}

impl ReportsService {
    pub fn new(cfg: ReportsServiceConfig) -> Self {
        Self {
            pool: cfg.pool,
            read: cfg.read_model,
            frozen: cfg.frozen_close_repo,
            audit: cfg.audit_repo,
            outbox: cfg.outbox_repo,
            device_id: cfg.device_id,
        }
    }

    pub fn require_role(role: UserRole, allowed: &[UserRole]) -> AppResult<()> {
        if allowed.contains(&role) {
            Ok(())
        } else {
            Err(AppError::Validation(format!(
                "this report requires one of: {:?}",
                allowed
            )))
        }
    }

    /// Phase-07 §7.17. Every reports IPC opens with this gate.
    pub fn require_reports_role(role: UserRole) -> AppResult<()> {
        Self::require_role(role, &[UserRole::Accountant, UserRole::Superadmin])
    }

    fn clamp_range_or_error(r: DateRange) -> AppResult<DateRange> {
        if r.to_utc <= r.from_utc {
            return Err(AppError::Validation("range to must be after from".into()));
        }
        let span = r.to_utc.signed_duration_since(r.from_utc);
        if span > Duration::days(MAX_LOCAL_RANGE_DAYS) {
            // §7.16 clamp to the last 90 days of the requested window.
            let from = r.to_utc - Duration::days(MAX_LOCAL_RANGE_DAYS);
            return Ok(DateRange {
                from_utc: from,
                to_utc: r.to_utc,
            });
        }
        Ok(r)
    }

    // ---- Dashboard --------------------------------------------------------

    #[instrument(skip(self))]
    pub async fn dashboard_kpis(
        &self,
        entity_id: &str,
        range: DateRange,
        include_voided: bool,
    ) -> AppResult<DashboardKpis> {
        let range = Self::clamp_range_or_error(range)?;

        let agg = self
            .read
            .aggregate_visits(entity_id, range.from_utc, range.to_utc, include_voided)
            .await?;
        let inv_value = self
            .read
            .inventory_consumption_value(entity_id, range.from_utc, range.to_utc)
            .await?;
        let net = agg
            .revenue_iqd
            .saturating_sub(agg.doctor_cut_iqd)
            .saturating_sub(agg.operator_cut_iqd)
            .saturating_sub(inv_value);

        let today_offset_secs = baghdad_offset_seconds();
        let today_naive =
            (Utc::now() + chrono::Duration::seconds(today_offset_secs as i64)).date_naive();
        let yesterday = today_naive - chrono::Duration::days(1);
        let (today_from, today_to) = local_day_utc_range(today_naive, today_offset_secs);
        let (y_from, y_to) = local_day_utc_range(yesterday, today_offset_secs);

        let today_vs_yesterday = self
            .trend_matrix(
                entity_id,
                today_from,
                today_to,
                y_from,
                y_to,
                include_voided,
            )
            .await?;

        // Week comparisons: use the last completed 7-day windows. Current
        // week = [today-7d, today). Prior week = [today-14d, today-7d).
        let week_to = today_to;
        let week_from = today_to - chrono::Duration::days(7);
        let prior_week_to = week_from;
        let prior_week_from = week_from - chrono::Duration::days(7);
        let week_vs_last = self
            .trend_matrix(
                entity_id,
                week_from,
                week_to,
                prior_week_from,
                prior_week_to,
                include_voided,
            )
            .await?;

        let month_to = today_to;
        let month_from = today_to - chrono::Duration::days(30);
        let prior_month_to = month_from;
        let prior_month_from = month_from - chrono::Duration::days(30);
        let month_vs_last = self
            .trend_matrix(
                entity_id,
                month_from,
                month_to,
                prior_month_from,
                prior_month_to,
                include_voided,
            )
            .await?;

        Ok(DashboardKpis {
            range_from: range.from_utc,
            range_to: range.to_utc,
            revenue_iqd: agg.revenue_iqd,
            doctor_cuts_iqd: agg.doctor_cut_iqd,
            operator_cuts_iqd: agg.operator_cut_iqd,
            inventory_consumption_value_iqd: inv_value,
            net_iqd: net,
            trend_today_vs_yesterday: today_vs_yesterday,
            trend_week_vs_last_week: week_vs_last,
            trend_month_vs_last_month: month_vs_last,
        })
    }

    async fn trend_matrix(
        &self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        prior_from: DateTime<Utc>,
        prior_to: DateTime<Utc>,
        include_voided: bool,
    ) -> AppResult<TrendMatrix> {
        let current = self
            .read
            .aggregate_visits(entity_id, from, to, include_voided)
            .await?;
        let prior = self
            .read
            .aggregate_visits(entity_id, prior_from, prior_to, include_voided)
            .await?;
        let current_inv = self
            .read
            .inventory_consumption_value(entity_id, from, to)
            .await?;
        let prior_inv = self
            .read
            .inventory_consumption_value(entity_id, prior_from, prior_to)
            .await?;
        let cur_net = current
            .revenue_iqd
            .saturating_sub(current.doctor_cut_iqd)
            .saturating_sub(current.operator_cut_iqd)
            .saturating_sub(current_inv);
        let prior_net = prior
            .revenue_iqd
            .saturating_sub(prior.doctor_cut_iqd)
            .saturating_sub(prior.operator_cut_iqd)
            .saturating_sub(prior_inv);
        Ok(TrendMatrix {
            revenue: trend_cell(TrendInputs {
                current: current.revenue_iqd,
                prior: prior.revenue_iqd,
            }),
            doctor_cuts: trend_cell(TrendInputs {
                current: current.doctor_cut_iqd,
                prior: prior.doctor_cut_iqd,
            }),
            operator_cuts: trend_cell(TrendInputs {
                current: current.operator_cut_iqd,
                prior: prior.operator_cut_iqd,
            }),
            inventory_value: trend_cell(TrendInputs {
                current: current_inv,
                prior: prior_inv,
            }),
            net: trend_cell(TrendInputs {
                current: cur_net,
                prior: prior_net,
            }),
        })
    }

    #[instrument(skip(self))]
    pub async fn dashboard_tops(
        &self,
        entity_id: &str,
        range: DateRange,
        include_voided: bool,
    ) -> AppResult<DashboardTops> {
        let range = Self::clamp_range_or_error(range)?;
        let doctors = self
            .read
            .aggregate_doctor_earnings(entity_id, range.from_utc, range.to_utc, include_voided)
            .await?;
        let operators = self
            .read
            .aggregate_operator_earnings(entity_id, range.from_utc, range.to_utc, include_voided)
            .await?;
        let check_types = self
            .read
            .daily_per_check_type(entity_id, range.from_utc, range.to_utc)
            .await?;
        let top_doctors = doctors.into_iter().take(5).collect();
        let mut top_operators: Vec<OperatorEarningsRow> = operators.into_iter().collect();
        top_operators.sort_by(|a, b| b.visits.cmp(&a.visits));
        let top_operators = top_operators.into_iter().take(5).collect();
        let mut top_check_types: Vec<CheckTypeDailyRow> = check_types.into_iter().collect();
        top_check_types.sort_by(|a, b| b.revenue_iqd.cmp(&a.revenue_iqd));
        let top_check_types = top_check_types.into_iter().take(5).collect();
        Ok(DashboardTops {
            top_doctors,
            top_operators,
            top_check_types,
        })
    }

    // ---- Visits report ----------------------------------------------------

    #[instrument(skip(self, filters))]
    pub async fn visits_report(&self, filters: VisitsReportFilters) -> AppResult<VisitsReport> {
        let filters = self.clamp_filters(filters)?;
        match filters.group_by {
            VisitsReportGroupBy::None => {
                let rows = self.read.list_visit_rows(&filters).await?;
                let totals = sum_visit_rows(&rows);
                Ok(VisitsReport::Rows { rows, totals })
            }
            _ => {
                let groups = self.read.list_visit_groups(&filters).await?;
                let totals = sum_groups(&groups);
                Ok(VisitsReport::Groups { groups, totals })
            }
        }
    }

    fn clamp_filters(&self, mut f: VisitsReportFilters) -> AppResult<VisitsReportFilters> {
        if f.to <= f.from {
            return Err(AppError::Validation("range to must be after from".into()));
        }
        let span = f.to.signed_duration_since(f.from);
        if span > Duration::days(MAX_LOCAL_RANGE_DAYS) {
            f.from = f.to - Duration::days(MAX_LOCAL_RANGE_DAYS);
        }
        Ok(f)
    }

    // ---- Doctor earnings --------------------------------------------------

    #[instrument(skip(self))]
    pub async fn doctor_earnings(
        &self,
        entity_id: &str,
        range: DateRange,
        include_voided: bool,
    ) -> AppResult<Vec<DoctorEarningsRow>> {
        let range = Self::clamp_range_or_error(range)?;
        self.read
            .aggregate_doctor_earnings(entity_id, range.from_utc, range.to_utc, include_voided)
            .await
    }

    #[instrument(skip(self))]
    pub async fn doctor_drilldown(
        &self,
        entity_id: &str,
        doctor_id: Option<Uuid>,
        range: DateRange,
        include_voided: bool,
    ) -> AppResult<DoctorDrilldown> {
        let range = Self::clamp_range_or_error(range)?;
        let earnings = self
            .read
            .aggregate_doctor_earnings(entity_id, range.from_utc, range.to_utc, include_voided)
            .await?;
        let me = earnings
            .into_iter()
            .find(|r| r.doctor_id == doctor_id)
            .unwrap_or_else(|| DoctorEarningsRow {
                doctor_id,
                name: if doctor_id.is_none() {
                    "(house)".into()
                } else {
                    String::new()
                },
                specialty: None,
                visits: 0,
                revenue_iqd: 0,
                doctor_cut_total_iqd: 0,
                avg_cut_per_visit_iqd: 0,
            });
        let per_check = self
            .read
            .doctor_per_check(
                entity_id,
                doctor_id,
                range.from_utc,
                range.to_utc,
                include_voided,
            )
            .await?;
        let source = self
            .read
            .doctor_source_visits(
                entity_id,
                doctor_id,
                range.from_utc,
                range.to_utc,
                include_voided,
            )
            .await?;
        let totals = sum_visit_rows(&source);
        Ok(DoctorDrilldown {
            doctor_id,
            name: me.name,
            specialty: me.specialty,
            per_check,
            source_visits: source,
            totals,
        })
    }

    // ---- Operator earnings ------------------------------------------------

    #[instrument(skip(self))]
    pub async fn operator_earnings(
        &self,
        entity_id: &str,
        range: DateRange,
        include_voided: bool,
    ) -> AppResult<Vec<OperatorEarningsRow>> {
        let range = Self::clamp_range_or_error(range)?;
        self.read
            .aggregate_operator_earnings(entity_id, range.from_utc, range.to_utc, include_voided)
            .await
    }

    #[instrument(skip(self))]
    pub async fn operator_drilldown(
        &self,
        entity_id: &str,
        operator_id: Uuid,
        range: DateRange,
        include_voided: bool,
    ) -> AppResult<OperatorDrilldown> {
        let range = Self::clamp_range_or_error(range)?;
        let earnings = self
            .read
            .aggregate_operator_earnings(entity_id, range.from_utc, range.to_utc, include_voided)
            .await?;
        let me = earnings
            .into_iter()
            .find(|r| r.operator_id == operator_id)
            .unwrap_or(OperatorEarningsRow {
                operator_id,
                name: String::new(),
                visits: 0,
                visits_with_dye: 0,
                operator_cut_total_iqd: 0,
                hours_on_shift_milli: 0,
                avg_cut_per_hour_iqd: 0,
            });
        let shifts: Vec<OperatorShiftRow> = self
            .read
            .operator_shifts_window(entity_id, operator_id, range.from_utc, range.to_utc)
            .await?;
        let total_hours = shifts.iter().map(|s| s.duration_milli.unwrap_or(0)).sum();
        let source = self
            .read
            .operator_source_visits(
                entity_id,
                operator_id,
                range.from_utc,
                range.to_utc,
                include_voided,
            )
            .await?;
        let totals = sum_visit_rows(&source);
        Ok(OperatorDrilldown {
            operator_id,
            name: me.name,
            shifts,
            attributed_visits: source,
            totals,
            total_hours_milli: total_hours,
        })
    }

    // ---- Daily Close ------------------------------------------------------

    /// Daily Close computation (§7.8, §7.9, §7.18, §7.19, §7.20).
    #[instrument(skip(self, settings_snapshot))]
    pub async fn daily_close(
        &self,
        actor_user_id: Uuid,
        entity_id: &str,
        target_date: NaiveDate,
        settings_snapshot: BTreeMap<String, String>,
    ) -> AppResult<DailyClose> {
        let offset_secs = baghdad_offset_seconds();
        let (from, to) = local_day_utc_range(target_date, offset_secs);

        let agg_locked = self
            .read
            .aggregate_visits(entity_id, from, to, false)
            .await?;
        let voided = self.read.voided_aggregate(entity_id, from, to).await?;
        let inv_value = self
            .read
            .inventory_consumption_value(entity_id, from, to)
            .await?;
        let per_doctor: Vec<DoctorDailyRow> =
            self.read.daily_per_doctor(entity_id, from, to).await?;
        let per_operator: Vec<OperatorDailyRow> =
            self.read.daily_per_operator(entity_id, from, to).await?;
        let per_check_type: Vec<CheckTypeDailyRow> =
            self.read.daily_per_check_type(entity_id, from, to).await?;

        let visit_ids = self
            .read
            .daily_visit_ids(entity_id, from, to)
            .await?
            .into_iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>();
        let pending_sync = self.read.outbox_count().await?;

        // Net is against cash ACTUALLY COLLECTED, not billed revenue: a
        // receptionist override (patient paid less) reduces the day's net. The
        // discount given is the gap between billed and collected (>= 0).
        let total_discount_iqd = agg_locked
            .revenue_iqd
            .saturating_sub(agg_locked.collected_iqd);
        let net_iqd = agg_locked
            .collected_iqd
            .saturating_sub(agg_locked.doctor_cut_iqd)
            .saturating_sub(agg_locked.operator_cut_iqd)
            .saturating_sub(inv_value);

        let target_date_str = target_date.format("%Y-%m-%d").to_string();
        let hash_input = DailyCloseHashInput {
            tenant_id: entity_id,
            target_date: &target_date_str,
            tz_offset_secs: offset_secs,
            visit_ids: &visit_ids,
            settings_snapshot,
            voided_count: voided.count,
            locked_count: agg_locked.visits,
            total_revenue_iqd: agg_locked.revenue_iqd,
            total_doctor_cuts_iqd: agg_locked.doctor_cut_iqd,
            total_operator_cuts_iqd: agg_locked.operator_cut_iqd,
        };
        let input_hash = compute_input_hash(&hash_input);

        let close = DailyClose {
            tenant_id: entity_id.to_string(),
            target_date,
            tz_offset: format_offset(offset_secs),
            total_revenue_iqd: agg_locked.revenue_iqd,
            total_collected_iqd: agg_locked.collected_iqd,
            total_discount_iqd,
            total_doctor_cuts_iqd: agg_locked.doctor_cut_iqd,
            total_operator_cuts_iqd: agg_locked.operator_cut_iqd,
            total_inventory_consumption_value_iqd: inv_value,
            net_iqd,
            locked_count: agg_locked.visits,
            voided_count: voided.count,
            voided_value_iqd: voided.value_iqd,
            per_doctor,
            per_operator,
            per_check_type,
            pending_sync,
            provisional: pending_sync > 0,
            input_hash: input_hash.clone(),
            generated_at: Utc::now(),
        };

        // §7.18: emit one audit row per run.
        self.emit_daily_close_audit(actor_user_id, entity_id, target_date, &close)
            .await?;
        Ok(close)
    }

    async fn emit_daily_close_audit(
        &self,
        actor_user_id: Uuid,
        entity_id: &str,
        target_date: NaiveDate,
        close: &DailyClose,
    ) -> AppResult<()> {
        let delta = serde_json::json!({
            "input_hash": close.input_hash,
            "generated_at": close.generated_at.to_rfc3339(),
            "total_revenue_iqd": close.total_revenue_iqd,
            "total_collected_iqd": close.total_collected_iqd,
            "total_discount_iqd": close.total_discount_iqd,
            "locked_count": close.locked_count,
            "voided_count": close.voided_count,
            "pending_sync_count": close.pending_sync,
            "provisional": close.provisional,
        });
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id,
            action: AuditAction::DailyCloseRun,
            entity: "daily_close".into(),
            entity_id: target_date.format("%Y-%m-%d").to_string(),
            delta,
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.into(),
        });
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.audit.append(&mut tx, &audit).await?;
        let audit_payload = encode_audit_payload(&audit)?;
        let outbox_row = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox.enqueue(&mut tx, &outbox_row).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }

    // ---- Sign & freeze (the materialized daily_close entity) --------------

    /// Sign and freeze a reconciled day. Recomputes the close, refuses if the
    /// day still has pending-sync ops (provisional data) or is already frozen,
    /// then materializes a `FrozenClose`, enqueues it for sync, and writes a
    /// `daily_close_sign` audit row -- all in one transaction. From then on the
    /// day is immutable (lock/void reject) until a superadmin reopens it.
    #[instrument(skip(self, settings_snapshot, signer_name))]
    #[allow(clippy::too_many_arguments)]
    pub async fn sign_daily_close(
        &self,
        actor_user_id: Uuid,
        signer_name: String,
        entity_id: &str,
        target_date: NaiveDate,
        settings_snapshot: BTreeMap<String, String>,
    ) -> AppResult<FrozenClose> {
        // Recompute from live data (this also writes the per-run audit row).
        let close = self
            .daily_close(actor_user_id, entity_id, target_date, settings_snapshot)
            .await?;

        // Gate 1: no provisional data -- you cannot freeze a day other devices
        // might still be writing to (PRD §7.2.5).
        if close.provisional {
            return Err(AppError::Validation(format!(
                "cannot freeze a provisional day: {} ops still pending sync",
                close.pending_sync
            )));
        }

        // Gate 2: not already frozen.
        if self
            .frozen
            .find_in_force_for_date(entity_id, target_date)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict("this day is already frozen".into()));
        }

        let now = Utc::now();
        let frozen = FrozenClose::try_new(
            FrozenCloseNewInput {
                target_date,
                tz_offset: close.tz_offset.clone(),
                input_hash: close.input_hash.clone(),
                total_revenue_iqd: close.total_revenue_iqd,
                total_collected_iqd: close.total_collected_iqd,
                total_discount_iqd: close.total_discount_iqd,
                total_doctor_cuts_iqd: close.total_doctor_cuts_iqd,
                total_operator_cuts_iqd: close.total_operator_cuts_iqd,
                total_inventory_consumption_value_iqd: close.total_inventory_consumption_value_iqd,
                net_iqd: close.net_iqd,
                locked_count: close.locked_count,
                voided_count: close.voided_count,
                voided_value_iqd: close.voided_value_iqd,
                signed_by_user_id: actor_user_id,
                signed_by_name: signer_name,
                entity_id: entity_id.to_string(),
                origin_device_id: Some(self.device_id.clone()),
            },
            now,
        )?;

        let payload = serde_json::to_vec(&FrozenClosePushPayload::from(&frozen))?;
        let op = OutboxOp::new("daily_close", frozen.id.to_string(), payload);
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id,
            action: AuditAction::DailyCloseSign,
            entity: "daily_close".into(),
            entity_id: target_date.format("%Y-%m-%d").to_string(),
            delta: serde_json::json!({
                "close_id": frozen.id.to_string(),
                "input_hash": frozen.input_hash,
                "net_iqd": frozen.net_iqd,
                "locked_count": frozen.locked_count,
                "signed_at": frozen.signed_at.to_rfc3339(),
            }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.into(),
        });
        let audit_payload = encode_audit_payload(&audit)?;
        let audit_op = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.frozen.insert(&mut tx, &frozen).await?;
        self.audit.append(&mut tx, &audit).await?;
        self.outbox.enqueue(&mut tx, &op).await?;
        self.outbox.enqueue(&mut tx, &audit_op).await?;
        tx.commit().await.map_err(AppError::from)?;

        Ok(frozen)
    }

    /// Reopen (unfreeze) a frozen day. Superadmin-only (enforced at the command
    /// boundary). Tombstones the in-force close, re-allowing edits for the day,
    /// enqueues the tombstone for sync, and writes a `daily_close_reopen` audit
    /// row. Errors if the day is not currently frozen.
    #[instrument(skip(self, reason))]
    pub async fn reopen_daily_close(
        &self,
        actor_user_id: Uuid,
        entity_id: &str,
        target_date: NaiveDate,
        reason: String,
    ) -> AppResult<FrozenClose> {
        let mut frozen = self
            .frozen
            .find_in_force_for_date(entity_id, target_date)
            .await?
            .ok_or_else(|| AppError::NotFound("no frozen close for this day".into()))?;

        let now = Utc::now();
        frozen.reopen(actor_user_id, reason.clone(), now)?;

        let payload = serde_json::to_vec(&FrozenClosePushPayload::from(&frozen))?;
        let op = OutboxOp::new("daily_close", frozen.id.to_string(), payload);
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id,
            action: AuditAction::DailyCloseReopen,
            entity: "daily_close".into(),
            entity_id: target_date.format("%Y-%m-%d").to_string(),
            delta: serde_json::json!({
                "close_id": frozen.id.to_string(),
                "reason": frozen.reopen_reason,
                "reopened_at": now.to_rfc3339(),
            }),
            ip: None,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.into(),
        });
        let audit_payload = encode_audit_payload(&audit)?;
        let audit_op = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        self.frozen.save_reopen(&mut tx, &frozen).await?;
        self.audit.append(&mut tx, &audit).await?;
        self.outbox.enqueue(&mut tx, &op).await?;
        self.outbox.enqueue(&mut tx, &audit_op).await?;
        tx.commit().await.map_err(AppError::from)?;

        Ok(frozen)
    }

    /// The in-force frozen close for a day, if any (backs the page's frozen
    /// badge + the recomputed-since-freeze discrepancy check).
    #[instrument(skip(self))]
    pub async fn frozen_close_for_date(
        &self,
        entity_id: &str,
        target_date: NaiveDate,
    ) -> AppResult<Option<FrozenClose>> {
        self.frozen
            .find_in_force_for_date(entity_id, target_date)
            .await
    }

    /// All closes (in-force + reopened) in a date range, newest first. Backs the
    /// month overview.
    #[instrument(skip(self))]
    pub async fn list_frozen_closes(
        &self,
        entity_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> AppResult<Vec<FrozenClose>> {
        self.frozen
            .list_in_range(entity_id, from_date, to_date)
            .await
    }

    // ---- Exports ----------------------------------------------------------

    #[instrument(skip(self, filters))]
    pub async fn export_visits_csv(
        &self,
        filters: VisitsReportFilters,
        path: &Path,
    ) -> AppResult<()> {
        let filters = self.clamp_filters(filters)?;
        let rows = self.read.list_visit_rows(&filters).await?;
        let totals = sum_visit_rows(&rows);
        write_visits_csv(&rows, &totals, path)
    }

    #[instrument(skip(self))]
    pub async fn export_doctor_earnings_csv(
        &self,
        entity_id: &str,
        range: DateRange,
        include_voided: bool,
        path: &Path,
    ) -> AppResult<()> {
        let rows = self
            .doctor_earnings(entity_id, range, include_voided)
            .await?;
        write_doctor_earnings_csv(&rows, path)
    }

    #[instrument(skip(self))]
    pub async fn export_operator_earnings_csv(
        &self,
        entity_id: &str,
        range: DateRange,
        include_voided: bool,
        path: &Path,
    ) -> AppResult<()> {
        let rows = self
            .operator_earnings(entity_id, range, include_voided)
            .await?;
        write_operator_earnings_csv(&rows, path)
    }

    /// Render a plain-text PDF artifact (parity with phase-05 receipts).
    /// The contract is path-bound; a future swap to a true PDF crate is a
    /// one-line change.
    #[instrument(skip(self, close))]
    /// Render a daily close to a real PDF at `path`. `clinic_name` is printed
    /// as the masthead when present. Delegates to the infrastructure renderer
    /// (which owns the `printpdf` dependency and file I/O).
    pub fn render_daily_close_pdf(
        &self,
        close: &DailyClose,
        clinic_name: Option<&str>,
        path: &Path,
    ) -> AppResult<()> {
        crate::domains::reports::infrastructure::daily_close_pdf::render(close, clinic_name, path)
    }
}

fn sum_visit_rows(rows: &[VisitRow]) -> VisitsReportTotals {
    let mut t = VisitsReportTotals::default();
    for r in rows {
        t.visits += 1;
        t.revenue_iqd = t.revenue_iqd.saturating_add(r.price_iqd);
        t.doctor_cut_iqd = t.doctor_cut_iqd.saturating_add(r.doctor_cut_iqd);
        t.operator_cut_iqd = t.operator_cut_iqd.saturating_add(r.operator_cut_iqd);
        t.net_iqd = t.net_iqd.saturating_add(r.net_iqd);
    }
    t
}

fn sum_groups(
    groups: &[crate::domains::reports::domain::entities::VisitsReportGroup],
) -> VisitsReportTotals {
    let mut t = VisitsReportTotals::default();
    for g in groups {
        t.visits = t.visits.saturating_add(g.visits);
        t.revenue_iqd = t.revenue_iqd.saturating_add(g.revenue_iqd);
        t.doctor_cut_iqd = t.doctor_cut_iqd.saturating_add(g.doctor_cut_iqd);
        t.operator_cut_iqd = t.operator_cut_iqd.saturating_add(g.operator_cut_iqd);
        t.net_iqd = t.net_iqd.saturating_add(g.net_iqd);
    }
    t
}

fn format_offset(secs: i32) -> String {
    let sign = if secs >= 0 { '+' } else { '-' };
    let abs = secs.abs();
    let h = abs / 3600;
    let m = (abs % 3600) / 60;
    format!("{sign}{:02}:{:02}", h, m)
}
