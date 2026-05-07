---
paths:
  - "src/**"
  - "src-tauri/**"
  - "**/sync/**"
  - "**/migrations/**"
---

# Offline-First Rules

The IDC system is **offline-first**: the Tauri app is the source of truth for the user's day-to-day workflow. The Fastify server exists for **sync** and **backup** -- the desktop app must function fully without network for the entire feature surface.

## Non-Negotiable Invariants

1. **Every read goes to local SQLite.** No screen ever blocks on the network for a read. If a feature needs server data we don't have locally, it's a phase-level decision (cache it, or label the feature "online-only" in the PRD).
2. **Every write commits locally first.** UI confirms success the moment the local transaction commits. The sync engine ships the change later.
3. **No write reaches the server without an `op_id`.** Every mutation carries a client-generated UUID so the server can dedupe retries. The server returns the same response for repeats.
4. **No client invents IDs the server later overrides.** Use UUID v7 (time-sortable) generated client-side; the server accepts those IDs as canonical.
5. **Deletes are tombstones.** Local rows are soft-deleted (`deleted_at`); the sync engine pushes the tombstone; the server keeps the row for the retention window.
6. **Schema migrations are idempotent and append-only on the client.** A user with an old DB must be able to upgrade without data loss.
7. **Every entity that syncs declares a conflict-resolution policy in its phase file.** No undeclared policies.

## Local Schema Conventions

Every syncable table has these columns:

| Column | Type | Purpose |
|-|-|-|
| `id` | `TEXT PRIMARY KEY` | UUID v7 generated client-side. |
| `created_at` | `TEXT NOT NULL` | RFC3339 UTC, set on insert. |
| `updated_at` | `TEXT NOT NULL` | RFC3339 UTC, set on every update (used for LWW). |
| `deleted_at` | `TEXT NULL` | Tombstone marker; non-null hides the row. |
| `version` | `INTEGER NOT NULL DEFAULT 0` | Monotonic per-row counter, bumped on every local mutation. |
| `dirty` | `INTEGER NOT NULL DEFAULT 1` | 1 = needs push, 0 = synced. |
| `last_synced_at` | `TEXT NULL` | Set after a successful push or pull. |
| `origin_device_id` | `TEXT NULL` | Device that created the row (debugging + LWW tiebreak). |

Plus an `outbox` table:

```sql
CREATE TABLE outbox (
  op_id TEXT PRIMARY KEY,         -- UUID v7
  entity TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  op TEXT NOT NULL,               -- 'upsert' | 'delete'
  payload BLOB NOT NULL,          -- MessagePack-encoded snapshot
  created_at TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TEXT NOT NULL,
  last_error TEXT NULL
);
CREATE INDEX outbox_next_attempt ON outbox(next_attempt_at) WHERE attempts < 10;
```

## The Sync Engine

The engine runs as a Tokio task in the Tauri app. Lifecycle:

1. **Boot.** Load device ID + last-pull cursor from `sync_state`. Subscribe to network status.
2. **Push loop.** Drain `outbox` in batches (capped, e.g., 50 ops). Each batch goes to `POST /sync/push` with `If-Match: <cursor>` semantics. On 200, mark rows clean and delete outbox entries. On 409 (conflict), invoke the conflict resolver. On 5xx, exponential backoff (`next_attempt_at`).
3. **Pull loop.** Periodically `GET /sync/pull?since=<cursor>` and apply the returned changes. Apply each change in a transaction, comparing `version` and `updated_at`. Update the cursor only after the transaction commits.
4. **Realtime (optional).** Subscribe to a server-sent event stream so newly pushed rows from other devices arrive sooner. The engine still treats the polled pull as the source of truth.
5. **Shutdown.** Cancel via `CancellationToken`, finish the in-flight HTTP request, persist the cursor.

The engine emits `tauri::Event`s for the UI: `sync:status` (`idle | pushing | pulling | offline | error`), `sync:progress`, `sync:conflict`. The frontend reflects these in a status indicator.

## Conflict Resolution

Every entity declares one of:

| Policy | Use when | Resolution |
|-|-|-|
| `last-write-wins` | Independent fields, low collision risk (e.g., user prefs). | Higher `updated_at` wins; tiebreak by `origin_device_id` lexicographic. |
| `field-merge` | Disjoint fields likely to be edited concurrently (e.g., contact: phone here, address there). | Merge per-field by the field's own `updated_at`. Requires per-field timestamps -- store as JSON column or sidecar table. |
| `additive-only` | Append-only logs (audit, comments). | Both writes survive; ordering by `created_at`. |
| `manual` | Domain-critical (financial documents, records that must reconcile). | Server returns 409 with both versions; UI surfaces a resolver screen; user picks. |

The phase file MUST name the policy when an entity becomes syncable. Changing a policy later is a breaking change and requires a migration phase.

## Network and Auth

- The sync engine treats `401` as "refresh and retry once". After a second 401, it surfaces a "session expired" event and pauses pushes (queued ops are preserved).
- The engine NEVER logs payload contents at `info!` level (PII). Use `debug!` and gate it behind a feature flag.
- All sync HTTP carries a `X-Device-Id` header and a `X-App-Version` header. The server may reject incompatible app versions with 426; the UI must prompt for upgrade.

## Testing

Every syncable feature MUST have these tests at minimum:

1. **Offline create -> reconnect -> server has it** (push smoke test).
2. **Server change -> client pull -> local row exists** (pull smoke test).
3. **Concurrent edit -> reconnect both clients -> declared policy is enforced** (conflict test).
4. **Delete -> reconnect -> tombstone propagates -> other clients hide the row** (delete test).
5. **Crash mid-push -> reboot -> outbox replays without duplication** (idempotency test).

These tests live in `src-tauri/tests/sync/` (Rust integration) and `tests/sync.spec.ts` (Playwright/Vitest E2E if applicable).

## Common Pitfalls

- **Generating IDs on the server only.** Breaks offline writes -- the local row would have no canonical ID until sync. Always client-side IDs.
- **`updated_at` from `Date.now()` on the client.** Clock skew between devices makes LWW unreliable. Use `updated_at` from the local clock for ordering against THIS device's writes, but treat the server's stamp as canonical when it returns one.
- **Forgetting to bump `version` on every mutation.** Pull-side merge will miss updates.
- **Holding a SQLite write transaction across an HTTP call.** Commit local first, then dispatch network work.
- **Letting the outbox grow unbounded.** Cap retries (e.g., 10) and surface "stuck" ops in the sync status UI for manual resolution.
- **Pulling without a cursor.** Re-pulling the entire dataset on every connect kills the server. The cursor must be persisted in `sync_state` and updated atomically with the applied changes.
