# Phase 9: Audit Page, FTS Polish & Vacuum

**Goal:** Land the global audit search page (deep filters, server fallback for queries beyond local 90-day retention), polish the conflict-resolver UI introduced from Phase 5 onward, and ship the daily vacuum job that prunes local audit rows.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 8.
**Complexity:** M
**PRD references:** §7.5 (Audit page), §10.4 (Audit), §10.8 (Offline UX — conflict resolver).
**Decisions consumed:** D-015 (90-day local retention; vacuum), D-024 (PII redaction).

---

## Section 1: Local Schema Changes (Tauri SQLite)

### Migration `017_sync_conflicts.sql`

```sql
-- Local-only. Records inbound conflicts the engine surfaces for the user to resolve.
CREATE TABLE IF NOT EXISTS sync_conflicts (
  id              TEXT PRIMARY KEY,                         -- UUID v7 (local)
  op_id           TEXT NOT NULL,                            -- the outbox op that conflicted
  entity          TEXT NOT NULL,
  entity_id       TEXT NOT NULL,
  policy          TEXT NOT NULL CHECK (policy IN ('manual')),
  local_payload   BLOB NOT NULL,                            -- MessagePack
  server_payload  BLOB NOT NULL,
  surfaced_at     TEXT NOT NULL,
  resolved_at     TEXT NULL,
  resolved_by     TEXT NULL REFERENCES users(id),
  resolution      TEXT NULL CHECK (resolution IN ('local','server','merge')),
  merged_payload  BLOB NULL
);
CREATE INDEX sync_conflicts_unresolved ON sync_conflicts(surfaced_at) WHERE resolved_at IS NULL;
```

### Migration `018_audit_fts.sql` (optional FTS over `delta` JSON)

```sql
-- Phase 9 introduces optional FTS5 over the audit_log delta JSON for local search.
-- Heavy; only built if `settings.audit_local_fts = '1'`. The migration creates the table empty;
-- a one-off backfill command populates it.
CREATE VIRTUAL TABLE IF NOT EXISTS audit_log_fts USING fts5(
  audit_id UNINDEXED,
  actor,
  entity,
  delta_text,
  tokenize = 'unicode61 remove_diacritics 2'
);
```

(Server-side `pg_trgm` was set up in P2's `init-custom-sql.sql`. P9 turns on the index that uses it.)

---

## Section 2: Server Schema Changes (Prisma / Postgres)

No new Prisma models.

### `init-custom-sql.sql` extensions (appended)

```sql
CREATE INDEX IF NOT EXISTS audit_log_delta_trgm ON audit_log USING gin ((delta::text) gin_trgm_ops);
CREATE INDEX IF NOT EXISTS audit_log_entity_id_trgm ON audit_log USING gin (entity_id gin_trgm_ops);
```

These power the server-side audit search beyond the desktop's 90-day local retention.

---

## Section 3: DDD Implementation

### Frontend (React)

#### Audit page (`/audit`) — full implementation

The placeholder from P1 lights up.

Layout (PRD §7.5):
```
+----------------------------------------------------------------------+
| Filters: [Actor v]  [Action v]  [Entity v]  [From..To]  [Search]    |
| +------------------------------------------------------------------+ |
| | At                  | Actor   | Action | Entity     | Entity ID | |
| ...                                                                  |
| +------------------------------------------------------------------+ |
| Clicking a row expands the JSON delta inline.                        |
+----------------------------------------------------------------------+
```

Filters:
- **Actor**: combobox over `users` (from local cache); supports clear.
- **Action**: enum picker (`create`, `update`, `soft_delete`, `lock`, `void`, `clock_in`, `clock_out`, `password_change`).
- **Entity**: dropdown of entity table names.
- **From..To**: date range picker.
- **Search**: full-text on `delta`. Local FTS5 if `audit_local_fts = '1'`; otherwise routes to server.

When the date range exceeds local 90-day retention OR `audit_local_fts = '0'`, the UI shows a "querying server" pill and calls `GET /audit/query`.

#### Conflict-resolver page (`/sync/conflicts`)

| Path | File | Description |
|-|-|-|
| `/sync/conflicts` | `src/pages/sync/conflicts.tsx` | List of unresolved conflicts. |
| `/sync/conflicts/:id` | `src/pages/sync/conflict-detail.tsx` | Side-by-side local vs server; pick local / server / merge. |

The sync pill's `error` state click target now opens this page directly.

#### Per-row pending-sync indicator

Tables across the app (visits, audit log, inventory) read the `dirty` column on each row and render a small dot when `dirty = 1`. Tooltip: "Pending sync".

#### React Query hooks
- `useAuditSearch(filter, cursor)`.
- `useUnresolvedConflicts()` — invalidates when a `sync:conflict` Tauri event fires.
- `useResolveConflict()` — mutation.

#### Zod schemas
`audit-filter.ts`, `conflict.ts`.

#### i18n
`audit.json` namespace (~60 keys).

### Tauri/Rust

#### `AuditQueryService`

File: `src-tauri/src/domains/audit/services/audit_query_service.rs`. Methods:
- `local_search(filter) -> Vec<AuditRow>` — uses `audit_log_fts` if enabled, else structured filters only.
- `is_within_local_window(filter) -> bool` — returns false if range extends past `now - 90d`.
- `server_search(filter, token) -> Vec<AuditRow>` — calls `SyncClient::audit_query`.

#### `VacuumJob`

File: `src-tauri/src/services/vacuum.rs`. Tokio interval task started by `AppState`:
1. Every 24h (and once at app startup), run:
   ```sql
   UPDATE audit_log
   SET deleted_at = ?, updated_at = ?, version = version + 1, dirty = 1
   WHERE deleted_at IS NULL
     AND at < ?                     -- 90 days ago
     AND dirty = 0;                 -- never delete unsynced rows
   ```
2. After the soft-delete pass, run `DELETE FROM audit_log WHERE deleted_at IS NOT NULL AND deleted_at < ? AND dirty = 0` for rows older than 180 days, with a row-count cap to avoid long pauses.
3. Run `PRAGMA wal_checkpoint(TRUNCATE)` quarterly.

The `dirty = 0` guard ensures audit rows that haven't shipped to the server yet are never pruned.

#### `ConflictResolver`

File: `src-tauri/src/sync/conflict_resolver.rs`. When `SyncEngine::pusher_loop` receives a 409, write a `sync_conflicts` row with the local + server payloads; emit `sync:conflict` event.

#### Tauri commands

| Command | Args | Returns |
|-|-|-|
| `audit_search` | `{ filter: AuditFilter, cursor?: String, limit: i64 }` | paged `Vec<AuditRow>` |
| `audit_search_remote` | same | same (forces server path) |
| `audit_backfill_fts` | `()` | `BackfillReport` (one-off; populates `audit_log_fts` from existing rows) |
| `sync_conflicts_list` | `()` | `Vec<ConflictRow>` |
| `sync_conflicts_resolve` | `{ id: Uuid, choice: ConflictChoice, merged_payload?: Bytes }` | `()` |
| `vacuum_run_now` | `()` | `VacuumReport` (debug helper) |

6 IPC commands.

### Sync Server (Fastify)

| Method | Path | Description |
|-|-|-|
| `GET` | `/audit/query` | `?actor=&action=&entity=&from=&to=&q=&cursor=`. Returns `{ items, nextCursor }`. Uses `pg_trgm` GIN index for free-text on `delta::text`. Auth required. |

1 route. Plus enrichment of the existing `/sync/conflicts/:opId/resolve` (P2 stub) with a full implementation that records the resolution and applies the chosen payload.

---

## Section 4: Business Logic

### Audit search dispatch

```rust
pub async fn search(filter: AuditFilter) -> Result<Vec<AuditRow>, AppError> {
    if Self::is_within_local_window(&filter) && Self::has_local_fts_or_no_text_search(&filter) {
        Self::local_search(filter).await
    } else {
        Self::server_search(filter).await
    }
}
```

UI surfaces a small "server-backed" pill when the second branch is taken.

### Vacuum semantics

Local audit retention = 90 days (D-015). Soft-delete first (allows recovery via tooling), hard-delete after 180 days. Both gated on `dirty = 0` (never lose unsynced data).

### Conflict resolver

The resolver UI calls `sync_conflicts_resolve`, which:
1. Reads the conflict row.
2. Applies the chosen payload as a regular outbox op (with `version > server.version` to force the next push).
3. Marks the conflict row resolved.
4. The next `pusher_loop` tick ships it.

PII redaction in displayed conflict deltas: the resolver UI calls a `redactPii(json)` utility that masks `password`, `password_hash`, `token`, and any field listed in `redactedFields` of the locale-specific schema (Q-004 follow-up).

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
None.

### Audit triggers
None.

### Local SQLite indexes
- `sync_conflicts_unresolved`.

### Tauri capabilities
None new.

### New Tauri plugins
None.

### Server cron
The vacuum-equivalent on the server is a **no-op** (server retention is indefinite per D-015). The materialized-view refresh from P7 is the only cron job.

---

## Section 6: Verification

1. Lint / build / test pass.
2. **Audit search local.** Filter by actor + action + entity + date in the last 30d → results in <300ms. Click a row → JSON delta expands inline.
3. **Audit search server fallback.** Filter date range covering 4 months back → "querying server" pill → results from `/audit/query`.
4. **Free-text search.** Search `"void_reason":` returns all void rows; matches via `pg_trgm` on the server.
5. **Vacuum.** Inject 200 audit rows older than 90 days; run `vacuum_run_now`; rows soft-deleted; query the audit page and confirm they're absent locally but still queryable via server fallback.
6. **Vacuum safety.** Inject an audit row with `dirty = 1` and `at` 100d ago; run vacuum; row NOT deleted.
7. **Conflict resolver.** Force a `manual` conflict on `visits` (edit same fields offline on two devices); reconnect both. Conflict surfaces on second push; resolver page shows side-by-side; pick `local` → outbox bumps version; ships; loser's prior local change appears in audit log.
8. **PII redaction.** Conflict resolver and audit row expansions never display password hashes or refresh tokens.
9. **i18n + RTL** on audit + conflicts pages.
10. **Pre-push composite.**

### What this phase does NOT verify
- Backup / restore (P10).
- BullMQ-based vacuum (deferred — `setInterval` is fine for v1).

### Summary update
Bump `status.md` row 9 to `Completed`. Add `/audit`, `/sync/conflicts`, `/sync/conflicts/:id` routes; `useAuditSearch`, `useUnresolvedConflicts`, `useResolveConflict`; `audit.json` namespace to `frontend-summary.md`. Note vacuum job under conventions.

---

## Section 7: PRD Gap Additions (Pass-V+)

### 7.1 `with_audit` lint rule lands here — MEDIUM
**Gap:** P1 §6 verification 7 says "lint rule lands in Phase 9" but P9 didn't reference it. P10 §7.1 hedges "land it in Phase 10 if it didn't make P9". This leaves the PRD §1.3 audit-coverage success metric unowned.
**Category:** Missing Verification.
**Remediation:** Ship a Clippy custom rule (or a `cargo-deny`-style check) that flags any `sqlx::query!` or `sqlx::query_as!` write invocation (`INSERT`, `UPDATE`, `DELETE`) outside `src-tauri/src/services/with_audit.rs` or `#[cfg(test)]` modules unless the call site is annotated `#[allow(audit_required)]`. Runs as part of `cargo clippy --all-targets -- -D warnings`. Add a unit test that intentionally violates the rule and asserts the lint fires.

### 7.2 `sync_conflicts.policy` CHECK widening — LOW
**Gap:** P9 §1 hardcodes `policy CHECK (policy IN ('manual'))` but is brittle if a future entity uses `field-merge`.
**Category:** Missing Setup.
**Remediation:** Drop the CHECK constraint. The column is informational; the engine's policy-dispatch lookup is the source of truth.

### 7.3 Migrating in-memory conflicts on first-boot — LOW
**Gap:** P1 §7.3 introduced an in-memory conflict queue. On first P9 boot, the queue must drain to the new `sync_conflicts` table or rows are lost.
**Category:** Missing Integration.
**Remediation:** In P9, ship a one-off `MigrationRunner::migrate_in_memory_conflicts(state, &pool)` that runs after migrations apply: drains `state.sync_engine.conflicts` `VecDeque` into `sync_conflicts` rows. Logged at `info!`. After this completes, the deque is dropped from `SyncEngineHandle`.
