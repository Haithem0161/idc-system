//! Deterministic input-hash for Daily Close artifacts (§7.12). Hashes the
//! canonicalised JSON of the aggregation inputs so v0.2 can use the hash as
//! the freeze key for the signed `daily_close` entity.
//!
//! We avoid the BLAKE3 dep here -- a stable hex digest of a SHA-256 sum over
//! the canonical input is functionally equivalent for the freeze-key purpose
//! and uses the existing transitive `sha2`-equivalent via `argon2`. We can
//! upgrade to BLAKE3 in Horizon-1 without breaking the contract.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct DailyCloseHashInput<'a> {
    pub tenant_id: &'a str,
    pub target_date: &'a str,
    pub tz_offset_secs: i32,
    pub visit_ids: &'a [String],
    pub settings_snapshot: BTreeMap<String, String>,
    pub voided_count: i64,
    pub locked_count: i64,
    pub total_revenue_iqd: i64,
    pub total_doctor_cuts_iqd: i64,
    pub total_operator_cuts_iqd: i64,
}

/// Hex-encoded SHA-256 of canonical JSON. Stable across reruns when the
/// inputs are unchanged.
pub fn compute_input_hash(input: &DailyCloseHashInput<'_>) -> String {
    let json = serde_json::to_string(input).unwrap_or_default();
    hex_sha256(json.as_bytes())
}

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_for_equal_inputs() {
        let ids = vec!["b".into(), "a".into()];
        let mut settings = BTreeMap::new();
        settings.insert("k".into(), "v".into());
        let input = DailyCloseHashInput {
            tenant_id: "t1",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings.clone(),
            voided_count: 0,
            locked_count: 1,
            total_revenue_iqd: 100,
            total_doctor_cuts_iqd: 40,
            total_operator_cuts_iqd: 10,
        };
        let h1 = compute_input_hash(&input);
        let h2 = compute_input_hash(&input);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_differs_when_inputs_change() {
        let ids = vec!["a".into()];
        let settings = BTreeMap::new();
        let a = DailyCloseHashInput {
            tenant_id: "t1",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings.clone(),
            voided_count: 0,
            locked_count: 1,
            total_revenue_iqd: 100,
            total_doctor_cuts_iqd: 40,
            total_operator_cuts_iqd: 10,
        };
        let b = DailyCloseHashInput {
            tenant_id: "t1",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings,
            voided_count: 0,
            locked_count: 2,
            total_revenue_iqd: 200,
            total_doctor_cuts_iqd: 40,
            total_operator_cuts_iqd: 10,
        };
        assert_ne!(compute_input_hash(&a), compute_input_hash(&b));
    }
}
