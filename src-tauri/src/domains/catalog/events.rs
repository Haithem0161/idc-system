//! Catalog domain events emitted via Tauri's app handle (§7.27, §7.35).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tracing::warn;
use uuid::Uuid;

pub const PRICING_CHANGED: &str = "catalog:pricing_changed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingChangeKind {
    CheckType,
    CheckSubtype,
    DoctorPricing,
    Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingChangedPayload {
    pub kind: PricingChangeKind,
    pub changed_entity_id: Uuid,
    pub check_type_id: Option<Uuid>,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub changed_at: DateTime<Utc>,
}

pub fn emit_pricing_changed<R: tauri::Runtime>(app: &AppHandle<R>, payload: PricingChangedPayload) {
    if let Err(e) = app.emit(PRICING_CHANGED, &payload) {
        warn!(error = %e, "failed to emit catalog:pricing_changed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(kind: PricingChangeKind) -> PricingChangedPayload {
        PricingChangedPayload {
            kind,
            changed_entity_id: Uuid::now_v7(),
            check_type_id: None,
            check_subtype_id: None,
            doctor_id: None,
            changed_at: Utc::now(),
        }
    }

    #[test]
    fn pricing_change_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&PricingChangeKind::CheckType).unwrap(),
            "\"check_type\""
        );
        assert_eq!(
            serde_json::to_string(&PricingChangeKind::CheckSubtype).unwrap(),
            "\"check_subtype\""
        );
        assert_eq!(
            serde_json::to_string(&PricingChangeKind::DoctorPricing).unwrap(),
            "\"doctor_pricing\""
        );
        assert_eq!(
            serde_json::to_string(&PricingChangeKind::Settings).unwrap(),
            "\"settings\""
        );
    }

    #[test]
    fn pricing_change_kind_deserializes_snake_case() {
        let p: PricingChangeKind = serde_json::from_str("\"check_type\"").unwrap();
        let s: PricingChangeKind = serde_json::from_str("\"check_subtype\"").unwrap();
        let d: PricingChangeKind = serde_json::from_str("\"doctor_pricing\"").unwrap();
        let g: PricingChangeKind = serde_json::from_str("\"settings\"").unwrap();
        assert_eq!(p, PricingChangeKind::CheckType);
        assert_eq!(s, PricingChangeKind::CheckSubtype);
        assert_eq!(d, PricingChangeKind::DoctorPricing);
        assert_eq!(g, PricingChangeKind::Settings);
    }

    #[test]
    fn pricing_changed_payload_round_trips_through_json() {
        let p = PricingChangedPayload {
            kind: PricingChangeKind::DoctorPricing,
            changed_entity_id: Uuid::now_v7(),
            check_type_id: Some(Uuid::now_v7()),
            check_subtype_id: Some(Uuid::now_v7()),
            doctor_id: Some(Uuid::now_v7()),
            changed_at: Utc::now(),
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: PricingChangedPayload = serde_json::from_str(&s).unwrap();
        assert_eq!(back.kind, p.kind);
        assert_eq!(back.changed_entity_id, p.changed_entity_id);
        assert_eq!(back.check_type_id, p.check_type_id);
        assert_eq!(back.check_subtype_id, p.check_subtype_id);
        assert_eq!(back.doctor_id, p.doctor_id);
    }

    #[test]
    fn pricing_changed_topic_name_is_canonical() {
        assert_eq!(PRICING_CHANGED, "catalog:pricing_changed");
    }

    #[test]
    fn pricing_changed_kind_includes_all_4_variants_via_payload() {
        for k in [
            PricingChangeKind::CheckType,
            PricingChangeKind::CheckSubtype,
            PricingChangeKind::DoctorPricing,
            PricingChangeKind::Settings,
        ] {
            let p = payload(k);
            assert_eq!(p.kind, k);
        }
    }
}
