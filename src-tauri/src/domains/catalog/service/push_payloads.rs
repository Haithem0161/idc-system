//! JSON push payloads for sync server. Mirrors the field names declared in
//! the sync-server memory store / Prisma models.

use serde::Serialize;

use crate::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem,
    Operator, OperatorSpecialty,
};

#[derive(Serialize)]
pub struct CheckTypePushPayload {
    pub id: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub has_subtypes: bool,
    pub base_price_iqd: Option<i64>,
    pub dye_supported: bool,
    pub report_supported: bool,
    pub sort_order: i64,
    pub is_active: bool,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&CheckType> for CheckTypePushPayload {
    fn from(ct: &CheckType) -> Self {
        Self {
            id: ct.id.to_string(),
            name_ar: ct.name_ar.clone(),
            name_en: ct.name_en.clone(),
            has_subtypes: ct.has_subtypes,
            base_price_iqd: ct.base_price_iqd,
            dye_supported: ct.dye_supported,
            report_supported: ct.report_supported,
            sort_order: ct.sort_order,
            is_active: ct.is_active,
            entity_id: ct.entity_id.clone(),
            version: ct.version,
            updated_at: ct.updated_at.to_rfc3339(),
            deleted_at: ct.deleted_at.map(|d| d.to_rfc3339()),
            origin_device_id: ct.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct CheckSubtypePushPayload {
    pub id: String,
    pub check_type_id: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub price_iqd: i64,
    pub sort_order: i64,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&CheckSubtype> for CheckSubtypePushPayload {
    fn from(s: &CheckSubtype) -> Self {
        Self {
            id: s.id.to_string(),
            check_type_id: s.check_type_id.to_string(),
            name_ar: s.name_ar.clone(),
            name_en: s.name_en.clone(),
            price_iqd: s.price_iqd,
            sort_order: s.sort_order,
            entity_id: s.entity_id.clone(),
            version: s.version,
            updated_at: s.updated_at.to_rfc3339(),
            deleted_at: s.deleted_at.map(|d| d.to_rfc3339()),
            origin_device_id: s.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct DoctorPushPayload {
    pub id: String,
    pub name: String,
    pub specialty: Option<String>,
    pub phone: Option<String>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&Doctor> for DoctorPushPayload {
    fn from(d: &Doctor) -> Self {
        Self {
            id: d.id.to_string(),
            name: d.name.clone(),
            specialty: d.specialty.clone(),
            phone: d.phone.clone(),
            is_active: d.is_active,
            notes: d.notes.clone(),
            entity_id: d.entity_id.clone(),
            version: d.version,
            updated_at: d.updated_at.to_rfc3339(),
            deleted_at: d.deleted_at.map(|t| t.to_rfc3339()),
            origin_device_id: d.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct DoctorPricingPushPayload {
    pub id: String,
    pub doctor_id: String,
    pub check_type_id: String,
    pub check_subtype_id: Option<String>,
    pub price_override_iqd: Option<i64>,
    pub cut_kind: &'static str,
    pub cut_value: i64,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&DoctorCheckPricing> for DoctorPricingPushPayload {
    fn from(p: &DoctorCheckPricing) -> Self {
        Self {
            id: p.id.to_string(),
            doctor_id: p.doctor_id.to_string(),
            check_type_id: p.check_type_id.to_string(),
            check_subtype_id: p.check_subtype_id.map(|s| s.to_string()),
            price_override_iqd: p.price_override_iqd,
            cut_kind: p.cut_kind.as_str(),
            cut_value: p.cut_value,
            entity_id: p.entity_id.clone(),
            version: p.version,
            updated_at: p.updated_at.to_rfc3339(),
            deleted_at: p.deleted_at.map(|t| t.to_rfc3339()),
            origin_device_id: p.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct OperatorPushPayload {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub base_cut_per_check_iqd: i64,
    pub is_active: bool,
    pub notes: Option<String>,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&Operator> for OperatorPushPayload {
    fn from(o: &Operator) -> Self {
        Self {
            id: o.id.to_string(),
            name: o.name.clone(),
            phone: o.phone.clone(),
            base_cut_per_check_iqd: o.base_cut_per_check_iqd,
            is_active: o.is_active,
            notes: o.notes.clone(),
            entity_id: o.entity_id.clone(),
            version: o.version,
            updated_at: o.updated_at.to_rfc3339(),
            deleted_at: o.deleted_at.map(|t| t.to_rfc3339()),
            origin_device_id: o.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct OperatorSpecialtyPushPayload {
    pub id: String,
    pub operator_id: String,
    pub check_type_id: String,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&OperatorSpecialty> for OperatorSpecialtyPushPayload {
    fn from(s: &OperatorSpecialty) -> Self {
        Self {
            id: s.id.to_string(),
            operator_id: s.operator_id.to_string(),
            check_type_id: s.check_type_id.to_string(),
            entity_id: s.entity_id.clone(),
            version: s.version,
            updated_at: s.updated_at.to_rfc3339(),
            deleted_at: s.deleted_at.map(|t| t.to_rfc3339()),
            origin_device_id: s.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct InventoryItemPushPayload {
    pub id: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub unit: String,
    pub quantity_on_hand: i64,
    pub low_stock_threshold: i64,
    pub is_active: bool,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&InventoryItem> for InventoryItemPushPayload {
    fn from(i: &InventoryItem) -> Self {
        Self {
            id: i.id.to_string(),
            name_ar: i.name_ar.clone(),
            name_en: i.name_en.clone(),
            unit: i.unit.clone(),
            quantity_on_hand: i.quantity_on_hand,
            low_stock_threshold: i.low_stock_threshold,
            is_active: i.is_active,
            entity_id: i.entity_id.clone(),
            version: i.version,
            updated_at: i.updated_at.to_rfc3339(),
            deleted_at: i.deleted_at.map(|t| t.to_rfc3339()),
            origin_device_id: i.origin_device_id.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct ConsumptionPushPayload {
    pub id: String,
    pub check_type_id: String,
    pub check_subtype_id: Option<String>,
    pub item_id: String,
    pub quantity_per_check: i64,
    pub on_dye_only: bool,
    pub entity_id: String,
    pub version: i64,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub origin_device_id: Option<String>,
}

impl From<&InventoryConsumptionMap> for ConsumptionPushPayload {
    fn from(c: &InventoryConsumptionMap) -> Self {
        Self {
            id: c.id.to_string(),
            check_type_id: c.check_type_id.to_string(),
            check_subtype_id: c.check_subtype_id.map(|s| s.to_string()),
            item_id: c.item_id.to_string(),
            quantity_per_check: c.quantity_per_check,
            on_dye_only: c.on_dye_only,
            entity_id: c.entity_id.clone(),
            version: c.version,
            updated_at: c.updated_at.to_rfc3339(),
            deleted_at: c.deleted_at.map(|t| t.to_rfc3339()),
            origin_device_id: c.origin_device_id.clone(),
        }
    }
}
