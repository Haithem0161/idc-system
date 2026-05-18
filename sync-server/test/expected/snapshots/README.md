# Canonical sync-wire snapshots

Phase-09 §3.3. Each pair `<name>.json` + `<name>.json.sha256` pins a wire-format
sample for one route surface or response. CI runs
`test/contract/canonical-snapshots.test.ts` which:

1. Validates the sample JSON against the route's TypeBox schema (drift on the
   server-side contract fails the test).
2. Re-hashes the sample bytes and compares to the stored SHA-256 (drift in the
   sample file itself fails the test).

Both directions are covered: a schema change without a sample update fails (1);
a sample-only edit without a deliberate hash regen fails (2). Regeneration is
explicit -- edit the JSON, run `node tools/regen-snapshot-hashes.js`, commit
both.

| Snapshot | Schema | Purpose |
|-|-|-|
| `patient-push.json` | `PushBodySchema` | Reception creating a new patient offline; one op, MessagePack `payload_b64`. |
| `visit-push-locked.json` | `PushBodySchema` | Reception locking a finalised visit (phase-07 §7.1). |
| `visit-push-voided.json` | `PushBodySchema` | Accountant voiding a locked visit with `void_reason`. |
| `inventory-adjustment-push.json` | `PushBodySchema` | Inventory manual restock with `applied_at`. |
| `operator-shift-push.json` | `PushBodySchema` | Closed shift with `duration_minutes` (phase-05 §5.2). |
| `operator-shift-pull.json` | `PullResponseSchema` | Other-device shift arriving via pull. |
| `operator-shift-soft-delete.json` | `PushBodySchema` | Soft-delete tombstone for a shift (`tombstone=true`). |
| `visit-pull-row.json` | `PullResponseSchema` | Locked visit arriving via pull. |
| `audit-query-response-mixed-50-row.json` | `AuditQueryResponseSchema` | 50-row page mixing all 14 actions and 15 entities + tri-state `ip`. |
| `conflict-list-response-canonical.json` | `ConflictsListResponseSchema` | 2-row open conflicts: visit version-conflict + patient manual-policy. |
| `conflict-resolve-applied-response.json` | `ResolveResponseSchema` | First resolve commit; `ok=true status='applied'`. |
| `conflict-resolve-already-resolved-response.json` | `ErrorResponseSchema` | Second resolve of the same conflict; 409 with `code='ALREADY_RESOLVED'` and `details.resolvedAt`. |
| `prometheus-exposition-sample.txt` | (plaintext) | `/metrics` exposition: all 10 named metrics + outbox gauge with tenant label. Hash-only pin -- format guarded by Prometheus conventions, not a TypeBox schema. |

## Regeneration

```bash
# Edit the JSON sample.
# Recompute the hash:
node -e 'const fs=require("fs");const buf=fs.readFileSync("FILE.json","utf8").replace(/\n$/,"");process.stdout.write(require("crypto").createHash("sha256").update(buf).digest("hex"))' > FILE.json.sha256
```

The trailing newline of each file is stripped before hashing -- matches the
phase-09 BLOCKER-5 healthz pattern at `test/expected/healthz/`. This means the
working tree can keep its `LF` end-of-file but the hash reflects the canonical
byte sequence.
