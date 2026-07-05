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
    pub mandoub_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    pub dalal: bool,
    pub discount: bool,
    pub locked_at: Option<DateTime<Utc>>,
    pub voided_at: Option<DateTime<Utc>>,
    pub voided_by_user_id: Option<Uuid>,
    pub void_reason: Option<String>,
    pub price_snapshot_iqd: Option<i64>,
    pub dye_cost_snapshot_iqd: Option<i64>,
    pub report_amount_snapshot_iqd: Option<i64>,
    pub report_pct_snapshot: Option<i64>,
    pub reporting_doctor_name_snapshot: Option<String>,
    pub doctor_cut_snapshot_iqd: Option<i64>,
    pub operator_cut_snapshot_iqd: Option<i64>,
    pub mandoub_cut_snapshot_iqd: Option<i64>,
    pub mandoub_name_snapshot: Option<String>,
    pub internal_pct_snapshot: Option<i64>,
    pub total_amount_iqd_snapshot: Option<i64>,
    pub amount_paid_override_iqd: Option<i64>,
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
            mandoub_id: v.mandoub_id,
            dye: v.dye,
            report: v.report,
            dalal: v.dalal,
            discount: v.discount,
            locked_at: v.locked_at,
            voided_at: v.voided_at,
            voided_by_user_id: v.voided_by_user_id,
            void_reason: v.void_reason.clone(),
            price_snapshot_iqd: snap.map(|s| s.price_iqd),
            dye_cost_snapshot_iqd: snap.map(|s| s.dye_cost_iqd),
            report_amount_snapshot_iqd: snap.map(|s| s.report_amount_iqd),
            report_pct_snapshot: snap.and_then(|s| s.report_pct),
            reporting_doctor_name_snapshot: snap.and_then(|s| s.reporting_doctor_name.clone()),
            doctor_cut_snapshot_iqd: snap.map(|s| s.doctor_cut_iqd),
            operator_cut_snapshot_iqd: snap.map(|s| s.operator_cut_iqd),
            // مندوب cut/name ride together and only when a مندوب is referenced
            // (name Some). Both None otherwise, mirroring the persisted columns.
            mandoub_cut_snapshot_iqd: snap
                .and_then(|s| s.mandoub_name.as_ref().map(|_| s.mandoub_cut_iqd)),
            mandoub_name_snapshot: snap.and_then(|s| s.mandoub_name.clone()),
            internal_pct_snapshot: snap.and_then(|s| s.internal_pct),
            total_amount_iqd_snapshot: snap.map(|s| s.total_amount_iqd),
            amount_paid_override_iqd: snap.and_then(|s| s.amount_paid_override_iqd),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::visits::domain::entities::{
        AdjustmentNewInput, AdjustmentReason, InventoryAdjustment, VisitCreateDraftInput,
        VisitSnapshots,
    };

    fn snap_house(price: i64) -> VisitSnapshots {
        VisitSnapshots {
            price_iqd: price,
            dye_cost_iqd: 0,
            report_amount_iqd: 0,
            report_pct: None,
            reporting_doctor_name: None,
            doctor_cut_iqd: price * 40 / 100,
            operator_cut_iqd: 5_000,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            internal_pct: Some(40),
            total_amount_iqd: price,
            amount_paid_override_iqd: None,
            patient_name: "Pat".into(),
            doctor_name: None,
            operator_name: "Op".into(),
            check_type_name_ar: "اختبار".into(),
            check_type_name_en: Some("Test".into()),
            check_subtype_name_ar: None,
            check_subtype_name_en: None,
        }
    }

    fn draft() -> Visit {
        Visit::create_draft(VisitCreateDraftInput {
            patient_id: Uuid::now_v7(),
            receptionist_user_id: Uuid::now_v7(),
            check_type_id: Uuid::now_v7(),
            check_subtype_id: None,
            doctor_id: None,
            mandoub_id: None,
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            price_override_iqd: None,
            entity_id: "t".into(),
            origin_device_id: Some("dev".into()),
        })
        .unwrap()
    }

    #[test]
    fn visit_push_payload_round_trip_via_messagepack_preserves_status_and_id() {
        let v = draft();
        let payload = VisitPushPayload::from(&v);
        let bytes = rmp_serde::to_vec_named(&payload).unwrap();
        let decoded: VisitPushPayload = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(decoded.id, v.id);
        assert_eq!(decoded.status, "draft");
        assert_eq!(decoded.entity_id, v.entity_id);
        assert!(decoded.locked_at.is_none());
        assert!(decoded.total_amount_iqd_snapshot.is_none());
    }

    #[test]
    fn visit_push_payload_for_locked_visit_carries_all_seven_name_snapshots() {
        let v = draft();
        let snap = snap_house(50_000);
        let locked = v
            .lock(Uuid::now_v7(), snap.clone(), chrono::Utc::now())
            .unwrap();
        let payload = VisitPushPayload::from(&locked);
        assert_eq!(payload.status, "locked");
        assert_eq!(
            payload.patient_name_snapshot.as_deref(),
            Some(snap.patient_name.as_str())
        );
        assert_eq!(
            payload.operator_name_snapshot.as_deref(),
            Some(snap.operator_name.as_str())
        );
        assert_eq!(
            payload.check_type_name_ar_snapshot.as_deref(),
            Some(snap.check_type_name_ar.as_str())
        );
        assert_eq!(
            payload.total_amount_iqd_snapshot,
            Some(snap.total_amount_iqd)
        );
        assert_eq!(payload.internal_pct_snapshot, Some(40));
        // doctor null in house mode -> doctor_name_snapshot None.
        assert!(payload.doctor_name_snapshot.is_none());
        // No override on this visit -> the wire field is None.
        assert!(payload.amount_paid_override_iqd.is_none());
    }

    #[test]
    fn visit_push_payload_carries_amount_paid_override_through_round_trip() {
        let v = draft();
        let mut snap = snap_house(50_000);
        snap.amount_paid_override_iqd = Some(30_000);
        let locked = v.lock(Uuid::now_v7(), snap, chrono::Utc::now()).unwrap();
        let payload = VisitPushPayload::from(&locked);
        assert_eq!(payload.amount_paid_override_iqd, Some(30_000));
        // Survives the MessagePack wire encoding used by the outbox.
        let bytes = rmp_serde::to_vec_named(&payload).unwrap();
        let decoded: VisitPushPayload = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(decoded.amount_paid_override_iqd, Some(30_000));
        // The billed total is unchanged by the override.
        assert_eq!(decoded.total_amount_iqd_snapshot, Some(50_000));
    }

    #[test]
    fn visit_push_payload_for_voided_carries_void_reason_trimmed() {
        let v = draft();
        let locked = v
            .lock(Uuid::now_v7(), snap_house(50_000), chrono::Utc::now())
            .unwrap();
        let voided = locked
            .void(
                "  valid reason  ".into(),
                Uuid::now_v7(),
                chrono::Utc::now(),
            )
            .unwrap();
        let payload = VisitPushPayload::from(&voided);
        assert_eq!(payload.status, "voided");
        assert_eq!(payload.void_reason.as_deref(), Some("valid reason"));
        assert!(payload.voided_at.is_some());
        assert!(payload.voided_by_user_id.is_some());
    }

    #[test]
    fn visit_push_payload_json_key_set_is_stable() {
        let v = draft();
        let payload = VisitPushPayload::from(&v);
        let json = serde_json::to_value(&payload).unwrap();
        let obj = json.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
        keys.sort();
        let expected = [
            "amount_paid_override_iqd",
            "check_subtype_id",
            "check_subtype_name_ar_snapshot",
            "check_subtype_name_en_snapshot",
            "check_type_id",
            "check_type_name_ar_snapshot",
            "check_type_name_en_snapshot",
            "created_at",
            "dalal",
            "deleted_at",
            "discount",
            "doctor_cut_snapshot_iqd",
            "doctor_id",
            "doctor_name_snapshot",
            "dye",
            "dye_cost_snapshot_iqd",
            "entity_id",
            "id",
            "internal_pct_snapshot",
            "locked_at",
            "mandoub_cut_snapshot_iqd",
            "mandoub_id",
            "mandoub_name_snapshot",
            "operator_cut_snapshot_iqd",
            "operator_id",
            "operator_name_snapshot",
            "origin_device_id",
            "patient_id",
            "patient_name_snapshot",
            "price_snapshot_iqd",
            "receptionist_user_id",
            "report",
            "report_amount_snapshot_iqd",
            "report_pct_snapshot",
            "reporting_doctor_name_snapshot",
            "status",
            "total_amount_iqd_snapshot",
            "updated_at",
            "version",
            "void_reason",
            "voided_at",
            "voided_by_user_id",
        ];
        assert_eq!(keys, expected);
    }

    #[test]
    fn adjustment_push_payload_round_trip_via_messagepack() {
        let adj = InventoryAdjustment::try_new(AdjustmentNewInput {
            item_id: Uuid::now_v7(),
            delta: -3,
            reason: AdjustmentReason::ConsumeVisit,
            visit_id: Some(Uuid::now_v7()),
            note: Some("test".into()),
            by_user_id: Uuid::now_v7(),
            entity_id: "t".into(),
            origin_device_id: Some("dev".into()),
        })
        .unwrap();
        let p = InventoryAdjustmentPushPayload::from(&adj);
        let bytes = rmp_serde::to_vec_named(&p).unwrap();
        let decoded: InventoryAdjustmentPushPayload = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(decoded.id, adj.id);
        assert_eq!(decoded.delta, -3);
        assert_eq!(decoded.reason, "consume_visit");
        assert_eq!(decoded.visit_id, adj.visit_id);
    }

    #[test]
    fn adjustment_push_payload_json_keys_stable() {
        let adj = InventoryAdjustment::try_new(AdjustmentNewInput {
            item_id: Uuid::now_v7(),
            delta: 5,
            reason: AdjustmentReason::Receive,
            visit_id: None,
            note: None,
            by_user_id: Uuid::now_v7(),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        let p = InventoryAdjustmentPushPayload::from(&adj);
        let json = serde_json::to_value(&p).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        keys.sort();
        let expected = [
            "by_user_id",
            "created_at",
            "deleted_at",
            "delta",
            "entity_id",
            "id",
            "item_id",
            "note",
            "origin_device_id",
            "reason",
            "updated_at",
            "version",
            "visit_id",
        ];
        assert_eq!(keys, expected);
    }
}
