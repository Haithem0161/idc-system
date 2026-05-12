//! Sync push wire formats for `visits` and `inventory_adjustments`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domains::visits::domain::entities::{InventoryAdjustment, Visit};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitPushPayload {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub status: String,
    pub receptionist_user_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub operator_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    pub locked_at: Option<DateTime<Utc>>,
    pub voided_at: Option<DateTime<Utc>>,
    pub voided_by_user_id: Option<Uuid>,
    pub void_reason: Option<String>,
    pub price_snapshot_iqd: Option<i64>,
    pub dye_cost_snapshot_iqd: Option<i64>,
    pub report_cost_snapshot_iqd: Option<i64>,
    pub doctor_cut_snapshot_iqd: Option<i64>,
    pub operator_cut_snapshot_iqd: Option<i64>,
    pub internal_pct_snapshot: Option<i64>,
    pub total_amount_iqd_snapshot: Option<i64>,
    pub patient_name_snapshot: Option<String>,
    pub doctor_name_snapshot: Option<String>,
    pub operator_name_snapshot: Option<String>,
    pub check_type_name_ar_snapshot: Option<String>,
    pub check_type_name_en_snapshot: Option<String>,
    pub check_subtype_name_ar_snapshot: Option<String>,
    pub check_subtype_name_en_snapshot: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

impl From<&Visit> for VisitPushPayload {
    fn from(v: &Visit) -> Self {
        let snap = v.snapshots.as_ref();
        Self {
            id: v.id,
            patient_id: v.patient_id,
            status: v.status.as_str().into(),
            receptionist_user_id: v.receptionist_user_id,
            check_type_id: v.check_type_id,
            check_subtype_id: v.check_subtype_id,
            doctor_id: v.doctor_id,
            operator_id: v.operator_id,
            dye: v.dye,
            report: v.report,
            locked_at: v.locked_at,
            voided_at: v.voided_at,
            voided_by_user_id: v.voided_by_user_id,
            void_reason: v.void_reason.clone(),
            price_snapshot_iqd: snap.map(|s| s.price_iqd),
            dye_cost_snapshot_iqd: snap.map(|s| s.dye_cost_iqd),
            report_cost_snapshot_iqd: snap.map(|s| s.report_cost_iqd),
            doctor_cut_snapshot_iqd: snap.map(|s| s.doctor_cut_iqd),
            operator_cut_snapshot_iqd: snap.map(|s| s.operator_cut_iqd),
            internal_pct_snapshot: snap.and_then(|s| s.internal_pct),
            total_amount_iqd_snapshot: snap.map(|s| s.total_amount_iqd),
            patient_name_snapshot: snap.map(|s| s.patient_name.clone()),
            doctor_name_snapshot: snap.and_then(|s| s.doctor_name.clone()),
            operator_name_snapshot: snap.map(|s| s.operator_name.clone()),
            check_type_name_ar_snapshot: snap.map(|s| s.check_type_name_ar.clone()),
            check_type_name_en_snapshot: snap.and_then(|s| s.check_type_name_en.clone()),
            check_subtype_name_ar_snapshot: snap.and_then(|s| s.check_subtype_name_ar.clone()),
            check_subtype_name_en_snapshot: snap.and_then(|s| s.check_subtype_name_en.clone()),
            created_at: v.created_at,
            updated_at: v.updated_at,
            deleted_at: v.deleted_at,
            version: v.version,
            origin_device_id: v.origin_device_id.clone(),
            entity_id: v.entity_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryAdjustmentPushPayload {
    pub id: Uuid,
    pub item_id: Uuid,
    pub delta: i64,
    pub reason: String,
    pub visit_id: Option<Uuid>,
    pub note: Option<String>,
    pub by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

impl From<&InventoryAdjustment> for InventoryAdjustmentPushPayload {
    fn from(a: &InventoryAdjustment) -> Self {
        Self {
            id: a.id,
            item_id: a.item_id,
            delta: a.delta,
            reason: a.reason.as_str().into(),
            visit_id: a.visit_id,
            note: a.note.clone(),
            by_user_id: a.by_user_id,
            created_at: a.created_at,
            updated_at: a.updated_at,
            deleted_at: a.deleted_at,
            version: a.version,
            origin_device_id: a.origin_device_id.clone(),
            entity_id: a.entity_id.clone(),
        }
    }
}
