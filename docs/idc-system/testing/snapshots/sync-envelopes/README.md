# Sync Envelope Snapshots

Canonical wire-format samples for the `/sync/push`, `/sync/pull`, and conflict envelopes. Tests on both surfaces validate that the produced JSON matches these byte-for-byte (after canonicalisation).

| File | Purpose | Validated by |
|-|-|-|
| `push-body.json` | The shape a Tauri client sends to `POST /sync/push`. | `sync-server/test/contract/sync-envelopes.test.ts::PushBodySchema accepts a minimal valid op`; phase-01 §10 snapshot test in `src-tauri/tests/sync_snapshots_phase01.rs`. |
| `push-response.json` | The shape the server returns from `/sync/push`. | `PushResponseSchema` accepts; snapshot test asserts byte-for-byte. |
| `pull-response.json` | The shape `/sync/pull` returns. | `PullResponseSchema` accepts; snapshot test pins. |

## Regeneration policy

Per `.claude/rules/testing.md` §10: regenerating ANY snapshot requires:

1. An explicit `--update-snapshots` flag at the test runner.
2. A human reviewer in the PR.
3. A written justification in the PR body ("Renderer changed because <reason>").

Auto-accepting a snapshot change is forbidden.

## Hash-based canonicalisation

Snapshots are compared via canonical-JSON hash (sorted keys, no insignificant whitespace). The hash function is `serde_json::to_value` -> `serde_json::to_string` (which sorts object keys deterministically when paired with `BTreeMap`). The Rust snapshot test computes the hash of the live envelope and the committed file and asserts equality.
