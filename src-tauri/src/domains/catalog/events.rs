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

pub fn emit_pricing_changed(app: &AppHandle, payload: PricingChangedPayload) {
    if let Err(e) = app.emit(PRICING_CHANGED, &payload) {
        warn!(error = %e, "failed to emit catalog:pricing_changed");
    }
}
