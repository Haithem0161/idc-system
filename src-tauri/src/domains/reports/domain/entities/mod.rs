//! Read-model DTOs for the reports bounded context. Pure data; no I/O.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Date range as an absolute closed-open UTC interval. The caller resolves
/// local-day boundaries to UTC via the tz offset declared in phase-07 §7.8
/// (Iraq, fixed UTC+03:00 year-round, no DST).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DateRange {
    pub from_utc: DateTime<Utc>,
    pub to_utc: DateTime<Utc>,
}

/// Grouping for the visits report (§7.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisitsReportGroupBy {
    #[default]
    None,
    ByDate,
    ByDoctor,
    ByOperator,
    ByCheckType,
    BySubtype,
    ByStatus,
}

/// Status set for the visits report. Defaults to locked-only per dashboard
/// toggle (§7.2 - status toggle "locked-only default; include voided").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisitsReportFilters {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub include_voided: bool,
    pub statuses: Vec<String>,
    pub check_type_ids: Vec<Uuid>,
    pub subtype_ids: Vec<Uuid>,
    pub doctor_ids: Vec<Uuid>,
    pub operator_ids: Vec<Uuid>,
    pub include_house: bool,
    pub dye: Option<bool>,
    pub report: Option<bool>,
    pub group_by: VisitsReportGroupBy,
    pub limit: Option<i64>,
    pub entity_id: String,
}

/// One visit row in the report. All money columns come from snapshots so the
/// row is stable across pricing changes (PRD §4.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitRow {
    pub visit_id: Uuid,
    pub locked_at: Option<DateTime<Utc>>,
    pub status: String,
    pub patient_name: String,
    pub check_type_name_ar: String,
    pub check_type_name_en: Option<String>,
    pub check_subtype_name_ar: Option<String>,
    pub check_subtype_name_en: Option<String>,
    pub doctor_name: Option<String>,
    pub operator_name: String,
    pub dye: bool,
    pub report: bool,
    pub price_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
    /// Report carve-out owed to the internal reporting doctor for this visit.
    /// Zero when the visit carries no report. Subtracted from net.
    pub report_amount_iqd: i64,
    /// مندوب (mandoub) carve-out owed to the referring representative for this
    /// visit. Zero when the visit carries no مندوب. Subtracted from net AFTER
    /// the report carve-out; does not change doctor/operator cuts or report.
    pub mandoub_cut_iqd: i64,
    /// Billed total for the visit (price + dye).
    pub total_iqd: i64,
    /// Cash actually collected when the receptionist overrode the billed total
    /// (`None` = paid the billed `total_iqd`). Drives the "overridden" marker in
    /// accounting; `Some(0)` means waived.
    pub amount_paid_override_iqd: Option<i64>,
    /// Net against COLLECTED cash: collected - doctor_cut - operator_cut -
    /// report_amount, where collected = `amount_paid_override_iqd` when set,
    /// else `total_iqd`.
    pub net_iqd: i64,
}

/// Totals for the rows in scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisitsReportTotals {
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
    /// Report carve-out owed to the reporting doctor across the rows.
    /// Subtracted from `net_iqd`.
    pub report_iqd: i64,
    /// مندوب carve-out owed to representatives across the rows.
    /// Subtracted from `net_iqd`.
    pub mandoub_cut_iqd: i64,
    pub net_iqd: i64,
}

/// Aggregated row for `groupBy != none`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitsReportGroup {
    pub key: String,
    pub label: String,
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
    /// Report carve-out owed to the reporting doctor across the group.
    /// Subtracted from `net_iqd`.
    pub report_iqd: i64,
    /// مندوب carve-out owed to representatives across the group.
    /// Subtracted from `net_iqd`.
    pub mandoub_cut_iqd: i64,
    pub net_iqd: i64,
}

/// Tagged-union response shape (§7.14).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum VisitsReport {
    Rows {
        rows: Vec<VisitRow>,
        totals: VisitsReportTotals,
    },
    Groups {
        groups: Vec<VisitsReportGroup>,
        totals: VisitsReportTotals,
    },
}

/// Dashboard KPIs (PRD §7.2.1).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardKpis {
    pub range_from: DateTime<Utc>,
    pub range_to: DateTime<Utc>,
    pub revenue_iqd: i64,
    pub doctor_cuts_iqd: i64,
    pub operator_cuts_iqd: i64,
    /// Reporting-doctor share carved from net across the range.
    pub report_cuts_iqd: i64,
    /// Representative (مندوب) cuts carved from net across the range.
    pub mandoub_cuts_iqd: i64,
    pub inventory_consumption_value_iqd: i64,
    pub net_iqd: i64,
    pub trend_today_vs_yesterday: TrendMatrix,
    pub trend_week_vs_last_week: TrendMatrix,
    pub trend_month_vs_last_month: TrendMatrix,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrendMatrix {
    pub revenue: TrendCell,
    pub doctor_cuts: TrendCell,
    pub operator_cuts: TrendCell,
    pub report_cuts: TrendCell,
    pub mandoub_cuts: TrendCell,
    pub inventory_value: TrendCell,
    pub net: TrendCell,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrendCell {
    pub current_iqd: i64,
    pub prior_iqd: i64,
    pub delta_iqd: i64,
    /// Permille (parts-per-thousand) so the frontend can render as `X.X%`
    /// without floats over the IPC boundary.
    pub delta_permille: i64,
}

/// One row in `<DoctorEarningsTable>` (§7.4). The house pseudo-row has
/// `doctor_id = None` and `name = "house"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorEarningsRow {
    pub doctor_id: Option<Uuid>,
    pub name: String,
    pub specialty: Option<String>,
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_total_iqd: i64,
    pub avg_cut_per_visit_iqd: i64,
}

/// Doctor drilldown: per-check breakdown + source visits (§7.30).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorDrilldown {
    pub doctor_id: Option<Uuid>,
    pub name: String,
    pub specialty: Option<String>,
    pub per_check: Vec<DoctorPerCheckRow>,
    pub source_visits: Vec<VisitRow>,
    pub totals: VisitsReportTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorPerCheckRow {
    pub check_type_id: Uuid,
    pub check_type_name_ar: String,
    pub check_type_name_en: Option<String>,
    pub check_subtype_id: Option<Uuid>,
    pub check_subtype_name_ar: Option<String>,
    pub check_subtype_name_en: Option<String>,
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub avg_cut_iqd: i64,
}

/// Operator earnings row (§7.5). Hours-on-shift is rendered as millis so the
/// frontend can format as hours.fraction without floats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorEarningsRow {
    pub operator_id: Uuid,
    pub name: String,
    pub visits: i64,
    pub visits_with_dye: i64,
    pub operator_cut_total_iqd: i64,
    pub hours_on_shift_milli: i64,
    pub avg_cut_per_hour_iqd: i64,
}

/// Operator drilldown (§7.30).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorDrilldown {
    pub operator_id: Uuid,
    pub name: String,
    pub shifts: Vec<OperatorShiftRow>,
    pub attributed_visits: Vec<VisitRow>,
    pub totals: VisitsReportTotals,
    pub total_hours_milli: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorShiftRow {
    pub shift_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub check_out_at: Option<DateTime<Utc>>,
    pub duration_milli: Option<i64>,
    pub lines_run: i64,
    pub cut_earned_iqd: i64,
}

/// مندوب (representative) earnings row. Mirrors `OperatorEarningsRow` but has no
/// shift-hours dimension: a مندوب is paid a flat per-visit cut (500 or 1000).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandoubEarningsRow {
    pub mandoub_id: Uuid,
    pub name: String,
    pub visits: i64,
    pub mandoub_cut_total_iqd: i64,
    pub avg_cut_per_visit_iqd: i64,
}

/// مندوب drilldown (§7.30, mirror of `OperatorDrilldown` minus shift hours).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandoubDrilldown {
    pub mandoub_id: Uuid,
    pub name: String,
    pub attributed_visits: Vec<VisitRow>,
    pub totals: VisitsReportTotals,
}

/// Daily close artifact (§7.9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyClose {
    pub tenant_id: String,
    pub target_date: NaiveDate,
    pub tz_offset: String,
    /// Billed revenue: SUM of locked visit totals (price + dye).
    pub total_revenue_iqd: i64,
    /// Cash actually collected: SUM of override-where-set, else billed total.
    pub total_collected_iqd: i64,
    /// Discount granted via receptionist overrides: billed - collected (>= 0).
    pub total_discount_iqd: i64,
    pub total_doctor_cuts_iqd: i64,
    pub total_operator_cuts_iqd: i64,
    /// Report carve-out owed to the reporting doctor across the day.
    /// Subtracted from net.
    pub total_report_iqd: i64,
    /// مندوب carve-out owed to representatives across the day.
    /// Subtracted from net AFTER the report carve-out.
    pub total_mandoub_cuts_iqd: i64,
    pub total_inventory_consumption_value_iqd: i64,
    /// Net against COLLECTED cash: collected - doctor cuts - operator cuts -
    /// report - مندوب cuts - inventory value.
    pub net_iqd: i64,
    pub locked_count: i64,
    pub voided_count: i64,
    pub voided_value_iqd: i64,
    pub per_doctor: Vec<DoctorDailyRow>,
    pub per_operator: Vec<OperatorDailyRow>,
    /// Per-representative breakdown of the مندوب carve-out for the day.
    pub per_mandoub: Vec<MandoubDailyRow>,
    pub per_check_type: Vec<CheckTypeDailyRow>,
    pub pending_sync: i64,
    pub provisional: bool,
    pub input_hash: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorDailyRow {
    pub doctor_id: Option<Uuid>,
    pub name: String,
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_iqd: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorDailyRow {
    pub operator_id: Uuid,
    pub name: String,
    pub visits: i64,
    pub dye_visits: i64,
    pub operator_cut_iqd: i64,
    pub hours_on_shift_milli: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandoubDailyRow {
    pub mandoub_id: Uuid,
    pub name: String,
    pub visits: i64,
    pub mandoub_cut_iqd: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckTypeDailyRow {
    pub check_type_id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
}

/// Dashboard "Top 5" cards payload (§7.22).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTops {
    pub top_doctors: Vec<DoctorEarningsRow>,
    pub top_operators: Vec<OperatorEarningsRow>,
    pub top_check_types: Vec<CheckTypeDailyRow>,
}

/// A signed, frozen daily close (the PRD §11.1 / phase-07 §7.12 Horizon-1
/// entity). Materialized when an accountant "signs and freezes" a reconciled
/// day: it captures the totals snapshot plus the `input_hash` freeze key and the
/// signer's attestation, and renders that day immutable until a superadmin
/// reopens it. Additive-only: created once, never mutated in place except the
/// reopen tombstone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrozenClose {
    pub id: Uuid,
    pub target_date: NaiveDate,
    pub tz_offset: String,
    pub input_hash: String,

    pub total_revenue_iqd: i64,
    pub total_collected_iqd: i64,
    pub total_discount_iqd: i64,
    pub total_doctor_cuts_iqd: i64,
    pub total_operator_cuts_iqd: i64,
    /// Reporting-doctor payable for the day. Subtracted from `net_iqd`; stored
    /// as its own line so a reopened/historical frozen close itemizes it.
    pub total_report_iqd: i64,
    /// مندوب (representative) payable for the day. Subtracted from `net_iqd`;
    /// stored as its own line so a reopened/historical frozen close itemizes it.
    pub total_mandoub_cuts_iqd: i64,
    pub total_inventory_consumption_value_iqd: i64,
    pub net_iqd: i64,
    pub locked_count: i64,
    pub voided_count: i64,
    pub voided_value_iqd: i64,

    pub signed_by_user_id: Uuid,
    pub signed_by_name: String,
    pub signed_at: DateTime<Utc>,

    /// Non-null once a superadmin has reopened (unfrozen) this close; the day is
    /// editable again until re-frozen.
    pub reopened_at: Option<DateTime<Utc>>,
    pub reopened_by_user_id: Option<Uuid>,
    pub reopen_reason: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

/// Inputs to materialize a brand-new frozen close from a freshly computed
/// `DailyClose` snapshot.
pub struct FrozenCloseNewInput {
    pub target_date: NaiveDate,
    pub tz_offset: String,
    pub input_hash: String,
    pub total_revenue_iqd: i64,
    pub total_collected_iqd: i64,
    pub total_discount_iqd: i64,
    pub total_doctor_cuts_iqd: i64,
    pub total_operator_cuts_iqd: i64,
    pub total_report_iqd: i64,
    pub total_mandoub_cuts_iqd: i64,
    pub total_inventory_consumption_value_iqd: i64,
    pub net_iqd: i64,
    pub locked_count: i64,
    pub voided_count: i64,
    pub voided_value_iqd: i64,
    pub signed_by_user_id: Uuid,
    pub signed_by_name: String,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

impl FrozenClose {
    /// Build a new (in-force) frozen close, validating invariants. Returns a
    /// validation error if the hash or signer name is blank.
    pub fn try_new(input: FrozenCloseNewInput, now: DateTime<Utc>) -> AppResult<Self> {
        if input.input_hash.trim().is_empty() {
            return Err(AppError::Validation("input_hash must not be empty".into()));
        }
        if input.signed_by_name.trim().is_empty() {
            return Err(AppError::Validation(
                "signed_by_name must not be empty".into(),
            ));
        }
        if input.entity_id.trim().is_empty() {
            return Err(AppError::Validation("entity_id must not be empty".into()));
        }
        Ok(Self {
            id: Uuid::now_v7(),
            target_date: input.target_date,
            tz_offset: input.tz_offset,
            input_hash: input.input_hash,
            total_revenue_iqd: input.total_revenue_iqd,
            total_collected_iqd: input.total_collected_iqd,
            total_discount_iqd: input.total_discount_iqd,
            total_doctor_cuts_iqd: input.total_doctor_cuts_iqd,
            total_operator_cuts_iqd: input.total_operator_cuts_iqd,
            total_report_iqd: input.total_report_iqd,
            total_mandoub_cuts_iqd: input.total_mandoub_cuts_iqd,
            total_inventory_consumption_value_iqd: input.total_inventory_consumption_value_iqd,
            net_iqd: input.net_iqd,
            locked_count: input.locked_count,
            voided_count: input.voided_count,
            voided_value_iqd: input.voided_value_iqd,
            signed_by_user_id: input.signed_by_user_id,
            signed_by_name: input.signed_by_name,
            signed_at: now,
            reopened_at: None,
            reopened_by_user_id: None,
            reopen_reason: None,
            created_at: now,
            updated_at: now,
            version: 1,
            origin_device_id: input.origin_device_id,
            entity_id: input.entity_id,
        })
    }

    /// True while this close is still in force (not reopened).
    pub fn is_in_force(&self) -> bool {
        self.reopened_at.is_none()
    }

    /// Reopen (unfreeze) this close. A superadmin action: records who reopened
    /// it, when, and why, and bumps the version so the tombstone syncs. Rejects
    /// an already-reopened close and a too-short reason.
    pub fn reopen(&mut self, by_user_id: Uuid, reason: String, at: DateTime<Utc>) -> AppResult<()> {
        if self.reopened_at.is_some() {
            return Err(AppError::Validation(
                "daily close is already reopened".into(),
            ));
        }
        if reason.trim().chars().count() < 5 {
            return Err(AppError::Validation(
                "reopen reason must be at least 5 characters".into(),
            ));
        }
        self.reopened_at = Some(at);
        self.reopened_by_user_id = Some(by_user_id);
        self.reopen_reason = Some(reason.trim().to_string());
        self.updated_at = at;
        self.version += 1;
        Ok(())
    }
}

#[cfg(test)]
mod frozen_close_tests {
    use super::*;

    fn sample_input() -> FrozenCloseNewInput {
        FrozenCloseNewInput {
            target_date: NaiveDate::from_ymd_opt(2026, 6, 19).unwrap(),
            tz_offset: "+03:00".into(),
            input_hash: "abc123".into(),
            total_revenue_iqd: 50_000,
            total_collected_iqd: 50_000,
            total_discount_iqd: 0,
            total_doctor_cuts_iqd: 1_500,
            total_operator_cuts_iqd: 4_000,
            total_report_iqd: 0,
            total_mandoub_cuts_iqd: 0,
            total_inventory_consumption_value_iqd: 0,
            net_iqd: 44_500,
            locked_count: 2,
            voided_count: 0,
            voided_value_iqd: 0,
            signed_by_user_id: Uuid::now_v7(),
            signed_by_name: "Karrar".into(),
            entity_id: "tenant-1".into(),
            origin_device_id: Some("device-1".into()),
        }
    }

    #[test]
    fn try_new_builds_an_in_force_close_at_version_1() {
        let now = Utc::now();
        let c = FrozenClose::try_new(sample_input(), now).unwrap();
        assert!(c.is_in_force());
        assert_eq!(c.version, 1);
        assert_eq!(c.signed_at, now);
        assert!(c.reopened_at.is_none());
        assert_eq!(c.net_iqd, 44_500);
    }

    #[test]
    fn try_new_rejects_blank_hash_name_and_tenant() {
        let now = Utc::now();
        let mut bad = sample_input();
        bad.input_hash = "  ".into();
        assert!(FrozenClose::try_new(bad, now).is_err());

        let mut bad = sample_input();
        bad.signed_by_name = "".into();
        assert!(FrozenClose::try_new(bad, now).is_err());

        let mut bad = sample_input();
        bad.entity_id = "".into();
        assert!(FrozenClose::try_new(bad, now).is_err());
    }

    #[test]
    fn reopen_tombstones_bumps_version_and_records_actor() {
        let now = Utc::now();
        let mut c = FrozenClose::try_new(sample_input(), now).unwrap();
        let admin = Uuid::now_v7();
        let later = now + chrono::Duration::minutes(5);
        c.reopen(admin, "wrong day frozen".into(), later).unwrap();
        assert!(!c.is_in_force());
        assert_eq!(c.version, 2);
        assert_eq!(c.reopened_by_user_id, Some(admin));
        assert_eq!(c.reopened_at, Some(later));
        assert_eq!(c.reopen_reason.as_deref(), Some("wrong day frozen"));
    }

    #[test]
    fn reopen_rejects_short_reason_and_double_reopen() {
        let now = Utc::now();
        let mut c = FrozenClose::try_new(sample_input(), now).unwrap();
        let admin = Uuid::now_v7();
        assert!(c.reopen(admin, "no".into(), now).is_err());
        // first valid reopen succeeds
        c.reopen(admin, "valid reason".into(), now).unwrap();
        // second reopen is rejected
        assert!(c.reopen(admin, "another reason".into(), now).is_err());
    }
}
