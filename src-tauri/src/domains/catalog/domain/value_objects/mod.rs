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
