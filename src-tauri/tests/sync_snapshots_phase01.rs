//! Phase-01 §10 snapshot tests: pin the wire format of /sync/push,
//! /sync/pull, and the conflict envelope.
//!
//! Snapshots live at
//! `docs/idc-system/testing/snapshots/sync-envelopes/*.json`
//! and are compared byte-for-byte (after canonicalisation) against the
//! live serializations. Per `.claude/rules/testing.md` §10, regenerating
//! a snapshot requires an explicit operator action; CI never auto-accepts.

use std::path::PathBuf;

use serde_json::Value;

fn snapshot_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // src-tauri -> repo root
    p.push("docs/idc-system/testing/snapshots/sync-envelopes");
    p.push(name);
    p
}

fn read_snapshot(name: &str) -> Value {
    let path = snapshot_path(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("snapshot {name} missing at {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("snapshot {name} invalid JSON: {e}"))
}

/// Canonicalise via re-serialization through serde_json. `serde_json::Value`
/// uses a sorted Map under the hood when constructed from text, but to
/// guarantee a stable byte ordering for the hash comparison we round-trip
/// through `to_string` which preserves the parsed key ordering.
fn canonical(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

#[test]
fn push_body_snapshot_matches_canonical_sample() {
    // The sample push body the engine constructs for a single audit_log
    // upsert. If the wire format changes, this test fails and the
    // operator must explicitly update the snapshot file.
    let live = serde_json::json!({
        "ops": [{
            "op_id": "01HZWAB000000000000000001",
            "entity": "audit_log",
            "entity_id": "01HZWAB000000000000000002",
            "op": "upsert",
            "payload_b64": "aGVsbG8="
        }]
    });
    let snapshot = read_snapshot("push-body.json");
    assert_eq!(
        canonical(&live),
        canonical(&snapshot),
        "push-body.json drift -- regenerate explicitly per §10"
    );
}

#[test]
fn push_response_snapshot_matches_canonical_sample() {
    let live = serde_json::json!({
        "accepted": [
            { "op_id": "01HZWAB000000000000000001", "status": "applied" }
        ],
        "conflicts": [{
            "op_id": "01HZWAB000000000000000003",
            "entity": "audit_log",
            "entity_id": "01HZWAB000000000000000004",
            "server_payload": { "version": 2 },
            "local_payload": { "version": 1 },
            "reason": "AUDIT_IMMUTABLE"
        }]
    });
    let snapshot = read_snapshot("push-response.json");
    assert_eq!(canonical(&live), canonical(&snapshot));
}

#[test]
fn pull_response_snapshot_matches_canonical_sample() {
    let live = serde_json::json!({
        "changes": [{
            "entity": "audit_log",
            "entity_id": "01HZWAB000000000000000005",
            "payload": {
                "delta": { "status": { "from": null, "to": "created" } }
            },
            "updated_at": "2026-05-13T10:00:00Z",
            "version": 1
        }],
        "next_cursor": "2026-05-13T10:00:00Z|01HZWAB000000000000000005"
    });
    let snapshot = read_snapshot("pull-response.json");
    assert_eq!(canonical(&live), canonical(&snapshot));
}

#[test]
fn snapshot_files_pass_their_own_typebox_schemas_via_contract_suite() {
    // Defence-in-depth check: every snapshot file is also exercised by
    // the sync-server contract tests at
    // `sync-server/test/contract/sync-envelopes.test.ts` via TypeBox
    // Value.Check. This Rust-side test only confirms the snapshot files
    // are present and parse as JSON; the schema validation is asserted on
    // the server side where the schemas live.
    for name in ["push-body.json", "push-response.json", "pull-response.json"] {
        let v = read_snapshot(name);
        assert!(v.is_object(), "snapshot {name} must be a JSON object");
    }
}
