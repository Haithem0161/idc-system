//! Read-model DTOs for the reports bounded context. Pure data; no I/O.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub net_iqd: i64,
}

/// Totals for the rows in scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisitsReportTotals {
    pub visits: i64,
    pub revenue_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
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

/// Daily close artifact (§7.9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyClose {
    pub tenant_id: String,
    pub target_date: NaiveDate,
    pub tz_offset: String,
    pub total_revenue_iqd: i64,
    pub total_doctor_cuts_iqd: i64,
    pub total_operator_cuts_iqd: i64,
    pub total_inventory_consumption_value_iqd: i64,
    pub net_iqd: i64,
    pub locked_count: i64,
    pub voided_count: i64,
    pub voided_value_iqd: i64,
    pub per_doctor: Vec<DoctorDailyRow>,
    pub per_operator: Vec<OperatorDailyRow>,
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
