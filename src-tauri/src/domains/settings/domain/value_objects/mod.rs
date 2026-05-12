//! Value objects for the settings bounded context.

use serde::{Deserialize, Serialize};

/// Typed value of a setting. Mirrors the SQLite `value_type` CHECK.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "valueType", content = "value")]
pub enum SettingValue {
    Int(i64),
    Decimal(String),
    Text(String),
    Bool(bool),
}

impl SettingValue {
    pub fn value_type(&self) -> &'static str {
        match self {
            Self::Int(_) => "int",
            Self::Decimal(_) => "decimal",
            Self::Text(_) => "text",
            Self::Bool(_) => "bool",
        }
    }

    pub fn as_storage(&self) -> String {
        match self {
            Self::Int(n) => n.to_string(),
            Self::Decimal(s) => s.clone(),
            Self::Text(s) => s.clone(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
        }
    }

    pub fn parse(value_type: &str, raw: &str) -> Option<Self> {
        match value_type {
            "int" => raw.parse::<i64>().ok().map(Self::Int),
            "decimal" => Some(Self::Decimal(raw.to_string())),
            "text" => Some(Self::Text(raw.to_string())),
            "bool" => match raw {
                "true" | "1" => Some(Self::Bool(true)),
                "false" | "0" => Some(Self::Bool(false)),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Keys whose deletion is forbidden by PRD §6.1.11 invariant 3.
pub const REQUIRED_KEYS: &[&str] = &[
    "dye_cost_iqd",
    "report_cost_iqd",
    "internal_doctor_pct",
    "idle_lock_minutes",
    "arabic_numerals",
    "clinic_display_name_ar",
    "clinic_display_name_en",
    "currency_symbol",
    "thermal_width",
    "thermal_printer_name",
];

pub fn is_required_key(key: &str) -> bool {
    REQUIRED_KEYS.contains(&key)
}
