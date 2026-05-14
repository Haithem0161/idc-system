//! Catalog value objects.

use serde::{Deserialize, Serialize};

/// Pricing cut model used by `doctor_check_pricing.cut_kind` (PRD §6.1.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CutKind {
    Pct,
    Fixed,
}

impl CutKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pct => "pct",
            Self::Fixed => "fixed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pct" => Some(Self::Pct),
            "fixed" => Some(Self::Fixed),
            _ => None,
        }
    }
}

impl std::fmt::Display for CutKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_and_parse_round_trip() {
        for variant in [CutKind::Pct, CutKind::Fixed] {
            assert_eq!(CutKind::parse(variant.as_str()), Some(variant));
        }
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", CutKind::Pct), "pct");
        assert_eq!(format!("{}", CutKind::Fixed), "fixed");
    }

    #[test]
    fn parse_rejects_unknown_and_case_variants() {
        assert!(CutKind::parse("").is_none());
        assert!(CutKind::parse("PCT").is_none());
        assert!(CutKind::parse(" pct").is_none());
        assert!(CutKind::parse("percent").is_none());
        assert!(CutKind::parse("fixed_amount").is_none());
    }

    #[test]
    fn serializes_as_lowercase_string() {
        let pct = serde_json::to_string(&CutKind::Pct).unwrap();
        let fixed = serde_json::to_string(&CutKind::Fixed).unwrap();
        assert_eq!(pct, "\"pct\"");
        assert_eq!(fixed, "\"fixed\"");
    }

    #[test]
    fn deserializes_from_lowercase_string() {
        let p: CutKind = serde_json::from_str("\"pct\"").unwrap();
        let f: CutKind = serde_json::from_str("\"fixed\"").unwrap();
        assert_eq!(p, CutKind::Pct);
        assert_eq!(f, CutKind::Fixed);
    }

    #[test]
    fn rejects_uppercase_or_unknown_on_deserialize() {
        assert!(serde_json::from_str::<CutKind>("\"PCT\"").is_err());
        assert!(serde_json::from_str::<CutKind>("\"percent\"").is_err());
    }

    #[test]
    fn equality_and_copy_semantics() {
        let a = CutKind::Pct;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(CutKind::Pct, CutKind::Fixed);
    }
}
