# Phase 8: Audit, Conflict Resolver & Polish

**Goal:** Ship the `/audit` page, the `/sync/conflicts` resolver UI on top of the Phase-1 mechanism, the server `/audit/query` endpoint, the daily vacuum job for audit pruning, and final RTL/i18n/performance/soak verification against PRD §1.3 success metrics.

**Surfaces:** All
**Dependencies:** Phase 07
**Complexity:** M

## §1 Local Schema Changes (Tauri SQLite)

No new tables. `audit_log` already created in Phase 1.

Migration file: `src-tauri/migrations/008_polish.sql` (no DDL; reserved for any soak-driven indexes; ships as a no-op placeholder unless needed).

### Modified tables

None.

### New enums

None (audit `action` values, full union per phase-01 §7.8: `create`, `update`, `soft_delete`, `lock`, `void`, `clock_in`, `clock_out`, `password_change`, `login`, `logout`, `conflict_resolve`, `vacuum`. Added in this phase: `conflict_resolve` (resolver writes) and `vacuum` (the audit vacuum's self-audit row per §7.3). `audit_log.action` is a TEXT column with no CHECK; the closed enum is enforced by `AuditAction::from_str` in Rust and by server TypeBox validators on `/sync/push`).

## §2 Server Schema Changes (Prisma / Postgres)

No new models.

### Modified models

None.

### New enums

None.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/audit` | `src/pages/audit/index.tsx` | Filter chips + result table with expandable JSON delta (PRD §7.5). |
| `/sync/conflicts` | `src/pages/sync/conflicts.tsx` | Resolver list with side-by-side local vs server payloads (PRD §10.8). |

Components:

| Component | File | Purpose |
|-|-|-|
| `<AuditFilters>` | `src/components/audit/audit-filters.tsx` | Actor / Action / Entity / From..To / free-text. |
| `<AuditTable>` | `src/components/audit/audit-table.tsx` | Result rows; click expands delta JSON inline. |
| `<DeltaViewer>` | `src/components/audit/delta-viewer.tsx` | Renders `{ field: { from, to } }` as a colored two-column diff. |
| `<ServerBackedBadge>` | `src/components/audit/server-backed-badge.tsx` | "querying server" pill when crossing 90-day boundary. |
| `<ConflictList>` | `src/components/sync/conflict-list.tsx` | One row per parked conflict. |
| `<ConflictResolverPanel>` | `src/components/sync/conflict-resolver-panel.tsx` | Side-by-side viewer; choose local / server / manual merge. |
| `<MergeEditor>` | `src/components/sync/merge-editor.tsx` | Per-field merge for `visits` and `settings`. |

Zustand stores: none new (reuses `useSyncStatusStore` from Phase 1).

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useAuditQuery(filters)` | `['audit','query', filters]` | Local + remote fallback based on range. |
| `useConflictsList` | `['sync','conflicts']` | Reuses placeholder from Phase 1; now wired to real data. |
| `useConflictResolve` | mutation | `sync::resolve_conflict`. |

Zod schemas:

| Schema | File |
|-|-|
| `AuditFilterSchema` | `src/lib/schemas/audit.ts` |
| `AuditRowSchema` | `src/lib/schemas/audit.ts` |
| `ConflictResolutionSchema` | `src/lib/schemas/sync.ts` (extended from Phase 1) |

### Tauri / Rust

Domain entity: `AuditEntry` from Phase 1; no changes.

```rust
pub struct AuditQueryService<'a> { /* local audit repo, remote http client */ }
impl<'a> AuditQueryService<'a> {
  pub async fn query(&self, filters: AuditFilter) -> Result<AuditPage, AppError> {
    if self.crosses_local_retention(&filters) {
      self.remote.query(filters).await
    } else {
      self.local.list(filters).await
    }
  }
}

pub struct AuditVacuumJob<'a> { /* local audit repo */ }
impl<'a> AuditVacuumJob<'a> {
  pub async fn run(&self) -> Result<VacuumResult, AppError> {
    /* Soft-delete audit rows where at < (now - 90 days) AND dirty = 0 */
  }
}
```

Repository trait extended:

```rust
#[async_trait]
pub trait AuditRepo {
  async fn append(&self, tx: &mut Tx, entry: AuditEntry) -> Result<(), AppError>;
  async fn list(&self, filter: AuditFilter, page: Page) -> Result<Vec<AuditEntry>, AppError>;
  async fn oldest_at(&self) -> Result<Option<DateTime<Utc>>, AppError>;
  async fn vacuum_older_than(&self, cutoff: DateTime<Utc>) -> Result<usize, AppError>;
}
```

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `audit::query` | `AuditFilter` | `AuditPage` | Local + remote fallback. |
| `audit::vacuum_now` | none | `VacuumResult` | Manual trigger; the scheduled run is invoked from `lib.rs::setup` daily. |
| `sync::list_conflicts` | (already from Phase 1) | `Conflict[]` | Now backed by real `ConflictParked` data via remote. |
| `sync::resolve_conflict` | (already from Phase 1) | `()` | Wired to UI in this phase. |

The vacuum runs daily as a Tokio task scheduled from `lib.rs::setup`; it sleeps until 03:00 local each day and then invokes `AuditVacuumJob::run`.

Register the new commands in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Entity class:

```ts
class AuditQuery {
  static fromQuery(q: AuditFilterQuery): AuditQuery { /* parse + validate */ }
}
```

Repository interface:

```ts
interface AuditQueryRepository {
  query(filter: AuditFilter, tenantId: string): Promise<{ rows: AuditLog[]; nextCursor: string | null }>;
}
```

Prisma repo notes: uses `LIMIT 200` per page; cursor on `(at, id)` lex; tenant filter via `entityIdTenant`.

TypeBox schemas:

| Schema | Purpose |
|-|-|
| `AuditQuerySchema` | Query-string filters. |
| `AuditQueryResponseSchema` | Row list + nextCursor. |

Route table:

| Method | Path | Description |
|-|-|-|
| `GET` | `/audit/query` | Server-side audit search (admin-only); paged; full-text on entity + actor. |

The conflict resolver endpoint `/sync/conflicts/:opId/resolve` was created in Phase 1; this phase only consumes it via the UI.

## §4 Business Logic

### Frontend

`<AuditFilters>`:

1. Actor combobox over `users`.
2. Action enum chips: `create | update | soft_delete | lock | void | clock_in | clock_out | password_change | login | logout | conflict_resolve | vacuum` (12-value union per phase-01 §7.8).
3. Entity dropdown over the 15 entity table names.
4. Date range picker.
5. Free-text input applied to `delta` (server-side full-text deferred to Horizon-1; v1 falls back to substring match on the JSON).

`<AuditTable>` row interaction:

1. Click expands `<DeltaViewer>` inline.
2. The viewer renders side-by-side `from` / `to` columns; identical fields collapsed.

`<ConflictResolverPanel>` flow per PRD §10.8:

1. Lists parked conflicts from `useConflictsList`.
2. Selecting one shows local vs server payloads via `<DeltaViewer>`.
3. Actions: "Keep local", "Keep server", "Merge".
4. Merge opens `<MergeEditor>` (one column per field; pick local or server, or edit manually).
5. Submit dispatches `sync::resolve_conflict`; on success removes the row.

### Tauri / Rust

`AuditQueryService::query`:

1. Build local SQL from filter.
2. If filter `from < (now - 90 days)`, route to remote.
3. Else execute local SELECT with the entity / action / date / actor predicates, plus `INSTR(delta, :free_text)` for free-text fallback.
4. Return paginated.

`AuditVacuumJob::run`:

1. `cutoff = now - 90 days`.
2. `count = AuditRepo::vacuum_older_than(cutoff)`.
3. Soft-delete only; the rows remain on the server because they were pushed (`dirty=0`) before the vacuum window applies.
4. Audit the vacuum itself as a `soft_delete` action with delta `{ count }` against `entity='audit_log'`, `entity_id='vacuum'`.

`SyncEngine::handle_conflict_response`:

1. When `/sync/push` returns `{ conflicts: [...] }`, store each parked conflict in a local `Conflict` cache (in-memory; the server is authoritative).
2. Emit `sync:conflict` events for the UI.
3. On `sync::resolve_conflict`, POST `/sync/conflicts/:opId/resolve`; on 200, remove the outbox row that originally caused the conflict (already retained for resolver-driven retry).

### Sync Server

`AuditQueryService.query`:

1. Validate query schema.
2. Run `prisma.auditLog.findMany` with `where: { entityIdTenant, AND: filters }` and `orderBy: [{ at: 'desc' }, { id: 'desc' }]`.
3. Return rows + nextCursor.

The endpoint is admin-only; the JWT plugin asserts `role = 'superadmin'` else 403.

`ConflictResolveService` (already in Phase 1) is now exercised end-to-end:

1. The Tauri client lists conflicts via a new server endpoint or by emitting them via the push response; for v1, the resolver UI reads from the in-memory cache populated by `sync:conflict` events.

### Sync Semantics

| Entity | Policy | Idempotency | Notes |
|-|-|-|-|
| `audit_log` | `additive-only` | `op_id` | Continues from Phase 1; vacuum only soft-deletes locally. |

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

No changes.

### Audit trigger additions

None.

### Local SQLite indexes

The existing `audit_log_*` indexes from Phase 1 suffice. Implementation may add a `audit_log_action` index if benchmark warrants it.

### Tauri capabilities

No new scopes.

### Plugin registrations

None new.

### Fastify plugins / BullMQ queues

- Optional: a BullMQ queue for daily audit reporting can be introduced here. v1 does NOT introduce BullMQ; the audit vacuum is a Tokio task in the desktop app only.

### Soak Harness

A test binary `src-tauri/tests/soak/eight_hour_offline.rs` simulates 8 hours of offline operation:

1. Disable network access in the test harness.
2. Generate synthetic visit creates / locks / shifts / adjustments at a rate matching PRD §1.3 (assume 100 visits / 8h).
3. After 8 hours simulated time, re-enable network.
4. Assert all rows arrive on the server within 5 minutes (sync engine drain).
5. Assert zero outbox rows remain.

### Performance Verification

The phase ships a benchmark harness `src-tauri/benches/`:

| Benchmark | Target | Source |
|-|-|-|
| Lock end-to-end | p95 < 30s | PRD §1.3 |
| Sync replication after reconnect | p95 < 5s | PRD §1.3 |
| Audit query (90-day window) | p95 < 500ms local | new |

### What this phase does NOT touch

- No new domain entities.
- No new sync contracts beyond `conflict_resolve` audit action.
- No PACS / clinical / multi-branch (Horizon-2).
- No signed `daily_close` (Horizon-1).

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover audit query filters, vacuum cutoff, conflict resolver wiring, soak-harness state machine.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`:
   1. Navigate to `/audit`; apply filters; verify delta expansion renders correctly.
   2. Trigger a `manual` conflict (edit same setting key on two devices); verify the conflict appears at `/sync/conflicts`; resolve via "Keep server"; verify the conflict disappears and the local row updates.
   3. Repeat with a "Manual merge" resolution.
5. `cd sync-server && pnpm test`: `/audit/query` happy + auth-denied; conflict resolve happy path.
6. Run the soak harness with simulated 8h offline; assert pass.
7. Run the performance benches; assert all targets met.
8. i18n lint: `pnpm lint:i18n` (script added in this phase) walks every JSX/TSX file outside `src/i18n/locales/` and fails on any Arabic or English literal string. Initial run must report zero violations.
9. RTL final sweep: take screenshots of every page in both `ar` and `en` directions; visual diff; assert zero regression.
10. Audit vacuum: insert audit rows dated 100 days ago into a test DB; trigger `audit::vacuum_now`; assert rows older than 90 days with `dirty=0` are soft-deleted; assert rows with `dirty=1` are retained.
11. Receipt print success telemetry: assert `> 99%` of lock events emit `receipt_print_success` per PRD §1.3.
12. End-to-end story: superadmin logs in; configures settings, doctors, operators, items; receptionist clocks in operators; creates and locks a visit; prints receipt; accountant runs the daily close; superadmin views the audit log and resolves a synthetic conflict. All steps pass without console errors or unhandled rejections.
13. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 13 gaps incorporated below._

### 7.1 `AuditRepo::vacuum_older_than` dirty=0 predicate in API
- **Gap:** HIGH | Missing Retention Policy | PRD §10.4
- §4 prose says the vacuum job skips dirty rows but the repo trait signature accepts only `cutoff`; callers can pass wrong cutoff and prune dirty rows.
- **Resolution:** Update trait signature to encode the predicate at the type level:
  ```rust
  trait AuditRepo {
      async fn vacuum_unsynced_safe(&self, cutoff: DateTime<Utc>) -> Result<u64, AuditError>;
  }
  ```
  Implementation runs `DELETE FROM audit_log WHERE at < ? AND dirty = 0 AND deleted_at IS NULL` (or soft-delete, see §7.3 below). Removes the foot-gun.

### 7.2 Vacuum missed-run handling
- **Gap:** MEDIUM | Missing Vacuum Job | PRD §10.4
- "Sleeps until 03:00 local each day" - no spec for missed runs when the laptop is closed past 03:00 nor for retry on failure.
- **Resolution:** Add to §4 `AuditVacuumJob`:
  - On app start, check `sync_state.last_audit_vacuum_at` (new column added here). If > 24h ago, run vacuum immediately.
  - On scheduled wakeup, run vacuum then update the cursor.
  - On error, log via `tracing` and retry after 1h (single retry; then wait for the next 24h tick).

### 7.3 Vacuum self-audit row convention
- **Gap:** LOW | Missing Business Rule | PRD §10.4
- Phase-08 §4 says the vacuum writes a self-audit row with `entity = 'vacuum'`; that's not a real row id.
- **Resolution:** Use `entity = 'audit_log'`, `entity_id = '00000000-0000-0000-0000-000000000000'` (zero UUID sentinel for system events), `action = 'vacuum'`, `before_json = null`, `after_json = { deleted_count, cutoff }`. Document the sentinel convention in §1.

### 7.4 Cross-boundary merged pagination for `audit::query`
- **Gap:** MEDIUM | Missing IPC Command | PRD §10.4
- `audit::query` routes locally OR remotely based on filters; users searching across the 90-day cliff see disjoint pages.
- **Resolution:** Update `audit::query` to a merge-paginator when `from < (now - 90d)` AND `to > (now - 90d)`:
  1. Issue local query for `[max(from, now-90d), to]`.
  2. Issue server query for `[from, min(to, now-90d - 1ms)]`.
  3. Merge results by `(at DESC, id DESC)` and apply the cursor on the merged stream.
  Cursor encoding: `{ at: ISO, id: UUID, source: 'local' | 'server' }`. Page boundary records carry a "Crossed local retention boundary" divider for the UI.

### 7.5 `/audit/query` cursor + sort stability
- **Gap:** MEDIUM | Missing Route | PRD §10.4
- Server `/audit/query` is GET-only with no cursor encoding spec; ties on `at` between two tenants are unstable.
- **Resolution:** §3.Server route spec extends:
  > Sort: `at DESC, id DESC` (composite for stability). Cursor: base64url-encoded `{ at, id }`. Response: `{ rows: AuditRow[], next_cursor: string | null }`. Max page size: 100; default 50.

### 7.6 `AuditQuerySchema` field enumeration
- **Gap:** MEDIUM | Missing Validation | PRD §7.5
- §3.Server schemas list `AuditQuerySchema` without enumerating fields.
- **Resolution:** Document in §3.Server schemas:
  ```ts
  AuditQuerySchema = Type.Object({
      from: Type.String({ format: 'date-time' }),
      to:   Type.String({ format: 'date-time' }),
      actor:    Type.Optional(Type.String({ format: 'uuid' })),    // user id
      action:   Type.Optional(Type.Union([Type.Literal('create'), ... 12 values])),
      entity:   Type.Optional(Type.Union([Type.Literal('users'), ... 15 values])),
      text:     Type.Optional(Type.String({ minLength: 2, maxLength: 100 })),
      cursor:   Type.Optional(Type.String()),
      limit:    Type.Optional(Type.Integer({ minimum: 1, maximum: 100, default: 50 })),
  })
  ```
  Same TypeBox refinement used by `<AuditPage>` query form.

### 7.7 Audit row drill-down navigation
- **Gap:** MEDIUM | Missing UI Element | PRD §7.5
- `<AuditTable>` renders inline `<DeltaViewer>` but no row → entity-detail navigation.
- **Resolution:** Add to `<AuditTable>` row in §3.Frontend table:
  > Each row's `Entity` cell is a `<Link>` to the source row's detail page when one exists (`users` → `/admin/users/:id`, `doctors` → `/admin/doctors/:id`, `visits` → `/reception/visits/:id`, ...). For entities without a detail route (`settings`, `audit_log`), the link is omitted. Routing map declared in `src/lib/audit/entity-routes.ts`.

### 7.8 Audit entity dropdown 15-name enumeration
- **Gap:** LOW | Missing Behavior | PRD §7.5
- "Entity dropdown over the 15 entity table names" - the list is not enumerated.
- **Resolution:** Document the dropdown values in §3.Frontend:
  > Options: `users, settings, check_types, check_subtypes, doctors, doctor_check_pricing, operators, operator_specialties, operator_shifts, patients, visits, inventory_items, inventory_consumption_map, inventory_adjustments, audit_log` (15 items in the order they appear in PRD §6.1). Bilingual labels live in `i18n/audit:entity.<name>`.

### 7.9 i18n lint specification
- **Gap:** HIGH | Missing Sweep | PRD §10.6
- §6 verification step 8 declares `pnpm lint:i18n` but the implementation is not specified.
- **Resolution:** Add to §5 Infrastructure Updates:
  > `pnpm lint:i18n` is implemented as `node tools/lint-i18n.mjs`. The script:
  > 1. Walks every `.tsx`/`.ts` file under `src/` excluding `src/i18n/locales/`.
  > 2. Uses `@babel/parser` to AST-parse and visits `JSXText` and string-literal arguments of `aria-label`, `title`, `placeholder`, `alt`.
  > 3. Fails on any node whose value contains characters in `[؀-ۿ]` (Arabic) or `[A-Za-z]{4,}` outside an `t(...)` / `<Trans>` call.
  > 4. Allowlist file at `tools/i18n-allowlist.txt` for legitimate literals (e.g., regex patterns, debug strings).
  Added to `pnpm lint` aggregate in this phase.

### 7.10 Pre-phase-08 i18n enforcement during development
- **Gap:** MEDIUM | Missing Sweep | PRD §10.6
- Phase-03 verification grep'd for Arabic; phase-08 enforces fully. Phases 02-07 have no in-flight enforcement.
- **Resolution:** Add to §5: a `pre-commit` hook (Husky + lint-staged) is installed in phase-01 §7 follow-up via a `.husky/pre-commit` script that runs `pnpm lint:i18n` on staged `.tsx`/`.ts` files. Until the script exists (this phase), phases 02-07 verification steps grep for Arabic literals locally. Document the migration here.

### 7.11 `/sync/conflicts` listing endpoint
- **Gap:** MEDIUM | Missing Route | PRD §10.8
- `<ConflictResolverPanel>` reads from "in-memory cache populated by `sync:conflict` events"; if the app restarts before resolving, the resolver shows nothing.
- **Resolution:** Add to §3.Server routes table:
  ```
  | GET    | /sync/conflicts            | Lists ConflictParked rows for the tenant (paginated by parked_at DESC). |
  ```
  TypeBox `ConflictListResponseSchema = Type.Array(ConflictParkedSchema)`. The endpoint returns ONLY unresolved conflicts (`WHERE resolvedAt IS NULL AND entityIdTenant = :tenant ORDER BY createdAt DESC LIMIT 100`); a separate `GET /sync/conflicts/history?from=&to=` is reserved for Horizon-1. Tauri side: the existing IPC `sync::list_conflicts(limit?: u32, offset?: u32, include_resolved?: bool) -> Vec<ConflictParked>` (declared in phase-01 §3 with placeholder return type `Conflict[]`) is amended here: return type promoted to `Vec<ConflictParked>`, default args list unresolved-only, `include_resolved=true` calls the future history endpoint (stubbed in v1 to return empty). The phase-01 stub return type is superseded. `<ConflictResolverPanel>` mounts → calls the IPC → renders. On app start in `<AppShell>`, the panel preloads once.

### 7.12 ARIA label key creation
- **Gap:** MEDIUM | Missing A11y Requirement | PRD §10.7
- PRD §10.7 says aria-label keys live in `i18n/common`. No phase declares the keys.
- **Resolution:** Add to §5: catalogue the icon-button aria-labels under `src/i18n/locales/{ar,en}/common.json` namespace `a11y.icons.*` (close, expand, collapse, search, filter, sort_asc, sort_desc, print, void, edit, delete, retry, refresh). Final sweep verifies each icon-only button has `aria-label={t('a11y.icons.<name>')}`.

### 7.13 A11y final-sweep verification
- **Gap:** HIGH | Missing A11y Requirement | PRD §10.7
- §6 verification step 9 covers RTL screenshots; no accessibility audit.
- **Resolution:** Append to §6 verification:
  > 14. `pnpm a11y` (script seeded in phase-01 §7.11) walks every page in the app (login, no-access, lock, reception/*, accounting/*, inventory/*, admin/*, audit, sync/conflicts). Assert zero serious or critical `axe-core` violations. Color contrast: assert all text/background pairs meet WCAG 2.1 AA (4.5:1 normal, 3:1 large).

### 7.14 SyncPill click-to-resolver wiring
- **Gap:** HIGH | Missing Handshake | phase-01 §7.5
- Phase-01 §7.5 deferred `<SyncPill>` onClick to phase-08. Phase-08 had no receipt.
- **Resolution:** Add to §3 Frontend: amend `<SyncPill>` (declared in phase-01 §3) to set `onClick={() => navigate('/sync/conflicts')}` whenever `status === 'error'` OR `outboxCount > 0` OR the pending-conflict badge is non-zero. Add a hover tooltip i18n-keyed `sync.pill.tooltip_view_conflicts`. Keyboard: `Enter`/`Space` activate the same navigation. The route already exists from §3.Frontend `/sync/conflicts`.

### 7.15 `<AuditTable>` Pending-sync column receipt
- **Gap:** LOW | Missing Handshake | phase-05 §7.29
- Phase-05 §7.29 says `<AuditTable>` adds a Pending-sync column rendering `<DirtyDot dirty={row.dirty === 1} />`. Phase-08 had no receipt.
- **Resolution:** Amend §3 Frontend `<AuditTable>` row to include the Pending-sync column. `audit::query` response carries `dirty: boolean` on every row. Column header i18n key `audit.columns.pending_sync`. Sortable but not filterable. Receipt confirms the shared `<DirtyDot>` from phase-05 §7.29 is consumed here.

### 7.16 Soak harness per-metric acceptance criteria
- **Gap:** HIGH | Missing Soak Criterion | PRD §1.3
- §5 soak harness asserts qualitative outcomes ("all rows arrive within 5 min") without quantitative thresholds.
- **Resolution:** Add to §5 Performance Verification block these acceptance criteria, asserted by the soak test:
  - Sync push throughput target: ≥ 50 ops/sec sustained over the 5-min drain after a simulated 8h offline window.
  - Outbox steady-state depth during 8h offline run: ≤ 800 rows (100 visits × 8 rows each).
  - p95 lock latency during sustained load: < 30s (PRD §1.3; sourced from `metrics_events` table in phase-01 §7.28).
  - Memory growth over 8h soak: < 50 MB (leak check).
  - Audit-vacuum after soak completes within 10s for 90-day rowset.
  - Sync conflict false-merge rate: 0 (verified by asserting no `metrics_events.kind='sync_conflict'` rows have `payload_json.auto_resolved = true`).
  Soak driver lives in `src-tauri/tests/soak/main.rs` and emits a Markdown report to `target/soak-report.md`.

### 7.17 Telemetry: `/metrics`, `/healthz` enrichment, `diagnostics::summary`
- **Gap:** MEDIUM | Missing Telemetry | PRD §1.3, §10.8
- PRD §1.3 names success metrics. No phase declared how they are surfaced.
- **Resolution:**
  - Server: add `GET /metrics` (Prometheus exposition format, gated by `X-Internal-Token` env-controlled; no JWT) exposing `sync_push_duration_seconds` histogram, `sync_conflict_total` counter, `outbox_depth_gauge` (per-tenant), `audit_query_duration_seconds`. Add to §3 server routes table.
  - Extend `/healthz` JSON to `{ status: 'ok', db: 'ok'|'fail', redis: 'ok'|'fail', migrationsApplied: bool, version: string }`.
  - Tauri: add IPC `diagnostics::summary | () | { lock_latency_p95_ms, outbox_depth, last_sync_at, conflict_count_7d, receipt_print_success_rate_30d } | Reads from metrics_events (phase-01 §7.28).`
  - Frontend: `<UserMenu>` "Diagnostics" entry opens a small modal rendering the summary. `<ConflictResolverPanel>` header surfaces a 7-day rolling counter (`conflicts opened`, `conflicts resolved`, `oldest unresolved age`).

### 7.18 RTL chevron/icon lint script
- **Gap:** MEDIUM | Missing A11y | PRD §10.6
- PRD §10.6 requires chevrons/arrows mirror via `rtl:rotate-180`. No phase enforces this.
- **Resolution:** Add to §5 Infrastructure: `pnpm lint:rtl` AST-scans every `.tsx` for lucide icons in the set `{ ChevronLeft, ChevronRight, ArrowLeft, ArrowRight, MoveLeft, MoveRight, ChevronFirst, ChevronLast }` used without an explicit `rtl:rotate-180` className or wrapper from `@/lib/rtl/icons.ts` (new module exporting `<DirectionalChevron direction="forward" />` etc.). Initial run must report zero violations. The script is invoked from the aggregate `pnpm lint` and from `.husky/pre-commit` (phase-01 §7.29).

### 7.19 `sync_state.last_audit_vacuum_at` column ALTER
- **Gap:** MEDIUM | Missing Schema | §7.2
- §7.2 reads `sync_state.last_audit_vacuum_at` "new column added here" but §1 declared "no DDL beyond placeholder".
- **Resolution:** Add to §1 the migration `008_polish.sql`:
  ```sql
  ALTER TABLE sync_state ADD COLUMN last_audit_vacuum_at TEXT NULL;
  ```
  Drop §1's "no-op placeholder" language. The §7.2 vacuum-missed-run check reads this column and updates it on each successful vacuum.

### 7.20 `handle.crumb` cross-phase declarations
- **Gap:** LOW | Missing Handshake | phase-01 §7.13
- Phase-01 §7.13 `<Breadcrumbs>` reads `handle.crumb` per route but no phase enumerated which routes export it.
- **Resolution:** Catalogue here (cross-reference to owning phases): every admin/reception/accounting/inventory detail route exports `handle: { crumb: ({ data }) => ... }`. Phase-03 §7.28 owns admin/* and inventory/*. Phase-05 §3 owns reception/* and visits/*. Phase-07 §7.27 owns accounting/*. Audit/sync routes (`/audit`, `/sync/conflicts`) export static crumbs via i18n keys `breadcrumbs.audit` and `breadcrumbs.sync_conflicts`.

### 7.21 `metrics_events` vacuum extension
- **Gap:** MEDIUM | Missing Vacuum Job | phase-01 §7.28; Pass-3 GAP-A-3
- Phase-01 §7.28 stated `metrics_events` is "30-day retention via the same vacuum that prunes `audit_log`". §4 `AuditVacuumJob::run` operates exclusively on `audit_log`; no code path touches `metrics_events`. Retention rule documented but executor missing.
- **Resolution:** Extend §4 `AuditVacuumJob::run`:
  - New repo trait `MetricsRepo::vacuum_older_than(cutoff: DateTime<Utc>) -> Result<u64, AppError>` (in `src-tauri/src/domains/metrics/repositories/`); SQL `DELETE FROM metrics_events WHERE at < ?` (hard delete; metrics are local-only and non-syncable per phase-01 §7.28).
  - Job step list updated: (1) `cutoff_audit = now - 90d`, (2) `vacuum_unsynced_safe(audit_log, cutoff_audit)`, (3) `cutoff_metrics = now - 30d`, (4) `vacuum_older_than(metrics_events, cutoff_metrics)`, (5) write a single `vacuum` audit row with delta `{ audit_purged, metrics_purged, cutoffs }`, (6) update `sync_state.last_audit_vacuum_at`.
  - Update §6 verification step 10: insert `metrics_events` rows >30d old; assert post-vacuum count is 0; assert `audit_log` rows >90d are deleted but rows <90d are untouched.

### 7.22 Resolver mid-flight idempotency
- **Gap:** MEDIUM | Missing State Recovery | §7.11; Pass-3 GAP-A-5
- §7.11 added `GET /sync/conflicts` for restart durability but the resolve flow itself (`POST /sync/conflicts/:opId/resolve`) is not specified for mid-flight failure: if the network drops AFTER the server commits the resolution and BEFORE the 200 returns, the client retries and the server may double-write.
- **Resolution:** Append to §3 Sync Server flow for the resolve endpoint:
  - Server step 1: wrap in the same `ProcessedOp` idempotency that gates `/sync/push` (phase-01 §4 SyncPushService step 1.i). If `ProcessedOp.has(resolve_op_id)` -> return cached response (200 with same body).
  - Client (`sync::resolve_conflict`): generate a stable resolve-op-id `sha256(opId|choice|merged_canonical_json)` so retries collide.
  - Server step 2: short-circuit if `ConflictParked.resolvedAt IS NOT NULL` AND the cached resolution matches the new request; if a DIFFERENT resolution is presented, return `409 ALREADY_RESOLVED` with the prior resolution body.
  - Frontend `<ConflictResolverPanel>` on 409: toast `errors:sync.already_resolved` ("This conflict was already resolved on another device. Refresh."), reload list.
  - Add §6 verification: simulate network drop after-commit-before-ack; assert second click does not double-apply.

### 7.23 `/audit` and `/sync/conflicts` route role gates
- **Gap:** HIGH | Missing Role Guard | PRD §7.5; Pass-3 GAP-E-9
- Server `/audit/query` is admin-only; the frontend `/audit` and `/sync/conflicts` routes have no `<RequireRole>` wrappers. Non-superadmin URL navigation renders the page and only fails on first IPC call.
- **Resolution:** Append to §3 Frontend routing block: "Both `/audit` and `/sync/conflicts` are wrapped in `<RequireRole roles={['superadmin']}>` (component from phase-02 §7.8). Non-matching role redirects to `/no-access`. `<UserMenu>` hides both links based on the same role check."

### 7.24 Audit `entity_id_prefix` substring filter
- **Gap:** MEDIUM | Missing UI Element | PRD §7.5 line 1889; Pass-3 GAP-E-17
- PRD §7.5 enumerates filters: actor, action, entity, entity_id substring, date range, free-text. §4 `<AuditFilters>` declares: actor combobox, action chips, entity dropdown, date range, free-text -- but free-text is `delta`-only per §4 step 5; entity_id substring is missing.
- **Resolution:** Add to §3 Frontend components: `<EntityIdSubstringInput>` inside `<AuditFilters>`, placeholder "first 8 chars of entity_id" (i18n `audit.filters.entity_id_prefix.placeholder`). Extend `AuditFilterSchema` (Zod + TypeBox in §7.6) with `entity_id_prefix?: string` (min 4, max 36). IPC `audit::query` and server `/audit/query` consume the field; the SQL adds `AND entity_id LIKE :prefix || '%'`.

### 7.25 `<ServerBackedBadge>` placement in `<AuditTable>`
- **Gap:** LOW | Missing UI Element | PRD §7.5 line 1891; Pass-3 GAP-E-18
- PRD §7.5 says "UI surfaces 'querying server' pill when crossing the boundary." `<ServerBackedBadge>` is declared but placement is unspecified.
- **Resolution:** Extend the §3 Frontend `<ServerBackedBadge>` row: rendered in the `<AuditTable>` header row when the `audit::query` response includes `mode: 'server' | 'merged'` (returned by §7.4 cross-boundary paginator). Per-row dividers ("Crossed local retention boundary") render as a sticky row separator inside `<AuditTable>` per §7.4.

### 7.26 Final TENANT_MODELS aggregation note
- **Gap:** LOW | Documentation | Pass-3 GAP-F-6; PRD §9.1
- §5 says "TENANT_MODELS additions: No changes" (correct -- the additions happened in earlier phases) but no phase aggregates the final list.
- **Resolution:** Append to §5: "Final TENANT_MODELS at v0.1.0 ship: 15 entries -- `audit_log, users, settings, check_types, check_subtypes, doctors, doctor_check_pricing, operators, operator_specialties, inventory_items, inventory_consumption_map, operator_shifts, patients, visits, inventory_adjustments` -- matching PRD §9.1. `outbox`, `sync_state`, `metrics_events` are local-only and intentionally excluded; `processed_ops`, `sync_cursors`, `conflict_parked`, `refresh_tokens` are server-only sync-engine tables and intentionally excluded."
