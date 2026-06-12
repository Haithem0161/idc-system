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

    /// Bare JSON scalar for the in-memory settings cache. NOTE: the derived
    /// `Serialize` is a tagged object (`{"valueType":..,"value":..}`), whose
    /// `.as_i64()/.as_str()/.as_bool()` are all `None`. Cache consumers
    /// (`money_settings`, `receipt_options`) expect a bare scalar, so the
    /// cache must store THIS, not `serde_json::to_value(self)`.
    pub fn to_cache_json(&self) -> serde_json::Value {
        match self {
            Self::Int(n) => serde_json::json!(n),
            Self::Decimal(s) => serde_json::json!(s),
            Self::Text(s) => serde_json::json!(s),
            Self::Bool(b) => serde_json::json!(b),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_type_returns_correct_tag_per_variant() {
        assert_eq!(SettingValue::Int(0).value_type(), "int");
        assert_eq!(SettingValue::Decimal("0".into()).value_type(), "decimal");
        assert_eq!(SettingValue::Text(String::new()).value_type(), "text");
        assert_eq!(SettingValue::Bool(false).value_type(), "bool");
    }

    #[test]
    fn as_storage_serializes_each_variant_to_string() {
        assert_eq!(SettingValue::Int(42).as_storage(), "42");
        assert_eq!(SettingValue::Decimal("3.14".into()).as_storage(), "3.14");
        assert_eq!(SettingValue::Text("hi".into()).as_storage(), "hi");
        assert_eq!(SettingValue::Bool(true).as_storage(), "true");
        assert_eq!(SettingValue::Bool(false).as_storage(), "false");
    }

    #[test]
    fn parse_int_round_trip() {
        let v = SettingValue::parse("int", "42").unwrap();
        assert_eq!(v, SettingValue::Int(42));
        assert!(SettingValue::parse("int", "not-a-number").is_none());
    }

    #[test]
    fn parse_decimal_keeps_string_form() {
        let v = SettingValue::parse("decimal", "12500.75").unwrap();
        assert_eq!(v, SettingValue::Decimal("12500.75".into()));
    }

    #[test]
    fn parse_text_accepts_empty_string() {
        let v = SettingValue::parse("text", "").unwrap();
        assert_eq!(v, SettingValue::Text(String::new()));
    }

    #[test]
    fn parse_bool_accepts_true_false_one_zero_and_rejects_other() {
        assert_eq!(
            SettingValue::parse("bool", "true").unwrap(),
            SettingValue::Bool(true)
        );
        assert_eq!(
            SettingValue::parse("bool", "false").unwrap(),
            SettingValue::Bool(false)
        );
        assert_eq!(
            SettingValue::parse("bool", "1").unwrap(),
            SettingValue::Bool(true)
        );
        assert_eq!(
            SettingValue::parse("bool", "0").unwrap(),
            SettingValue::Bool(false)
        );
        assert!(SettingValue::parse("bool", "yes").is_none());
    }

    #[test]
    fn parse_unknown_value_type_returns_none() {
        assert!(SettingValue::parse("json", "{}").is_none());
    }

    #[test]
    fn required_keys_list_has_exactly_the_ten_v1_keys() {
        assert_eq!(REQUIRED_KEYS.len(), 10);
        for k in [
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
        ] {
            assert!(is_required_key(k), "{k} must be a required key");
        }
    }

    #[test]
    fn is_required_key_rejects_unknown_key() {
        assert!(!is_required_key("horizon_pet_mode"));
        assert!(!is_required_key(""));
    }

    #[test]
    fn to_cache_json_yields_bare_scalars_readable_by_cache_consumers() {
        // money_settings/receipt_options read get_setting(..).as_i64()/.as_str()
        // /.as_bool(); the cache must hold bare scalars, not the tagged enum.
        assert_eq!(
            SettingValue::Int(10_000).to_cache_json().as_i64(),
            Some(10_000)
        );
        assert_eq!(
            SettingValue::Text("IDC".into()).to_cache_json().as_str(),
            Some("IDC")
        );
        assert_eq!(
            SettingValue::Bool(true).to_cache_json().as_bool(),
            Some(true)
        );
        assert_eq!(
            SettingValue::Decimal("1.5".into()).to_cache_json().as_str(),
            Some("1.5")
        );
    }

    #[test]
    fn derived_serialize_is_tagged_and_not_readable_as_scalar() {
        // Documents WHY to_cache_json exists: the derived Serialize is tagged,
        // so as_i64() on it is None -- storing it in the cache zeroes money.
        let tagged = serde_json::to_value(SettingValue::Int(10_000)).unwrap();
        assert!(tagged.is_object());
        assert_eq!(tagged.as_i64(), None);
    }
}
