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

    /// §7.12: hex digest of SHA-256 is 64 chars and only lowercase hex.
    #[test]
    fn hash_is_64_lowercase_hex_chars() {
        let ids: Vec<String> = vec![];
        let settings = BTreeMap::new();
        let h = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings,
            voided_count: 0,
            locked_count: 0,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    /// A new visit changes the hash even when totals would coincidentally
    /// match -- the visit_ids slice contributes (Pass-1 §1.1).
    #[test]
    fn hash_changes_when_a_new_visit_id_is_added() {
        let ids_a = vec!["v1".to_string()];
        let ids_b = vec!["v1".to_string(), "v2".to_string()];
        let settings = BTreeMap::new();
        let h_a = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids_a,
            settings_snapshot: settings.clone(),
            voided_count: 0,
            locked_count: 1,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        let h_b = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids_b,
            settings_snapshot: settings,
            voided_count: 0,
            locked_count: 1,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        assert_ne!(h_a, h_b);
    }

    /// Two tenants with identical aggregates still hash to different values
    /// (the tenant_id is folded in -- prevents tenant cross-talk on the
    /// signed-close horizon).
    #[test]
    fn hash_differs_across_tenants_with_identical_aggregates() {
        let ids: Vec<String> = vec!["v1".into()];
        let settings = BTreeMap::new();
        let h_a = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "tenant-a",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings.clone(),
            voided_count: 0,
            locked_count: 1,
            total_revenue_iqd: 100,
            total_doctor_cuts_iqd: 30,
            total_operator_cuts_iqd: 10,
        });
        let h_b = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "tenant-b",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings,
            voided_count: 0,
            locked_count: 1,
            total_revenue_iqd: 100,
            total_doctor_cuts_iqd: 30,
            total_operator_cuts_iqd: 10,
        });
        assert_ne!(h_a, h_b);
    }

    /// A settings tweak (e.g., dye_cost_iqd bump) changes the hash. The
    /// daily-close key needs to invalidate when the pricing rule changes.
    #[test]
    fn hash_changes_when_settings_snapshot_diverges() {
        let ids: Vec<String> = vec![];
        let mut settings_a = BTreeMap::new();
        settings_a.insert("dye_cost_iqd".into(), "2000".into());
        let mut settings_b = BTreeMap::new();
        settings_b.insert("dye_cost_iqd".into(), "2500".into());
        let h_a = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings_a,
            voided_count: 0,
            locked_count: 0,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        let h_b = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: settings_b,
            voided_count: 0,
            locked_count: 0,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        assert_ne!(h_a, h_b);
    }

    /// BTreeMap iteration order is deterministic, so settings keyed in any
    /// order hash identically.
    #[test]
    fn hash_is_independent_of_settings_insertion_order() {
        let ids: Vec<String> = vec![];
        let mut m1 = BTreeMap::new();
        m1.insert("a".to_string(), "1".to_string());
        m1.insert("b".to_string(), "2".to_string());
        let mut m2 = BTreeMap::new();
        m2.insert("b".to_string(), "2".to_string());
        m2.insert("a".to_string(), "1".to_string());
        let h1 = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: m1,
            voided_count: 0,
            locked_count: 0,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        let h2 = compute_input_hash(&DailyCloseHashInput {
            tenant_id: "t",
            target_date: "2026-05-12",
            tz_offset_secs: 10800,
            visit_ids: &ids,
            settings_snapshot: m2,
            voided_count: 0,
            locked_count: 0,
            total_revenue_iqd: 0,
            total_doctor_cuts_iqd: 0,
            total_operator_cuts_iqd: 0,
        });
        assert_eq!(h1, h2);
    }
}
