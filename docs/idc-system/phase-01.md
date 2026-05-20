# Phase 1: Foundation & Sync Plumbing

**Goal:** Stand up the offline-first plumbing end-to-end (outbox, sync engine, audit log, tenant scoping, conflict mechanism, app shell, sync-server bootstrap) so that subsequent phases only need to add domain entities and pages.

**Surfaces:** All
**Dependencies:** None
**Complexity:** L

## §1 Local Schema Changes (Tauri SQLite)

Migration file: `src-tauri/migrations/001_foundation.sql` (idempotent; `CREATE TABLE IF NOT EXISTS` where safe).

### outbox

```sql
CREATE TABLE IF NOT EXISTS outbox (
  op_id            TEXT PRIMARY KEY,
  entity           TEXT NOT NULL,
  entity_id        TEXT NOT NULL,
  op               TEXT NOT NULL CHECK (op IN ('upsert','delete')),
  payload          BLOB NOT NULL,
  created_at       TEXT NOT NULL,
  attempts         INTEGER NOT NULL DEFAULT 0,
  next_attempt_at  TEXT NOT NULL,
  last_error       TEXT NULL
);
CREATE INDEX IF NOT EXISTS outbox_next_attempt ON outbox(next_attempt_at) WHERE attempts < 10;
```

### sync_state

```sql
CREATE TABLE IF NOT EXISTS sync_state (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  pull_cursor     TEXT NULL,
  last_pulled_at  TEXT NULL,
  last_pushed_at  TEXT NULL,
  device_id       TEXT NOT NULL
);
```

### audit_log (per PRD §6.1.15)

```sql
CREATE TABLE IF NOT EXISTS audit_log (
  id                TEXT PRIMARY KEY,
  actor_user_id     TEXT NOT NULL,
  action            TEXT NOT NULL,
  entity            TEXT NOT NULL,
  entity_id         TEXT NOT NULL,
  delta             TEXT NOT NULL,
  ip                TEXT NULL,
  device_id         TEXT NOT NULL,
  at                TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id_tenant  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS audit_log_entity ON audit_log(entity, entity_id, at);
CREATE INDEX IF NOT EXISTS audit_log_actor  ON audit_log(actor_user_id, at);
CREATE INDEX IF NOT EXISTS audit_log_at     ON audit_log(at);
```

The FK to `users(id)` is not declared here because `users` lands in Phase 2. The constraint is added in `002_users.sql` via `ALTER TABLE` (SQLite limitation: use a CHECK-supported workaround or rebuild). Phase 2 owns the FK addition.

### Modified tables

None.

### New enums

`outbox.op CHECK IN ('upsert','delete')`.

## §2 Server Schema Changes (Prisma / Postgres)

Edit `sync-server/prisma/schema.prisma`. New models:

```prisma
model AuditLog {
  id              String    @id
  actorUserId     String    @map("actor_user_id")
  action          String
  entity          String
  entityId        String    @map("entity_id")
  delta           Json
  ip              String?
  deviceId        String    @map("device_id")
  at              DateTime  @db.Timestamptz
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityIdTenant  String    @map("entity_id_tenant")

  @@index([entity, entityId, at])
  @@index([actorUserId, at])
  @@index([at])
  @@map("audit_log")
}

model ProcessedOp {
  opId            String    @id @map("op_id")
  entityIdTenant  String    @map("entity_id_tenant")
  responseHash    String    @map("response_hash")
  processedAt     DateTime  @default(now()) @map("processed_at") @db.Timestamptz

  @@index([entityIdTenant, processedAt])
  @@map("processed_ops")
}

model SyncCursor {
  deviceId        String    @id @map("device_id")
  entityIdTenant  String    @map("entity_id_tenant")
  cursor          String
  updatedAt       DateTime  @updatedAt @map("updated_at") @db.Timestamptz

  @@index([entityIdTenant])
  @@map("sync_cursors")
}

model ConflictParked {
  opId            String    @id @map("op_id")
  entityIdTenant  String    @map("entity_id_tenant")
  entity          String
  entityId        String    @map("entity_id")
  localPayload    Json      @map("local_payload")
  serverPayload   Json      @map("server_payload")
  createdAt       DateTime  @default(now()) @map("created_at") @db.Timestamptz
  resolvedAt      DateTime? @map("resolved_at") @db.Timestamptz
  resolvedByUserId String?  @map("resolved_by_user_id")

  @@index([entityIdTenant, resolvedAt])
  @@map("conflicts_parked")
}
```

### New enums

None at server scope; the SQLite `op` CHECK is mirrored as a string union in the TypeBox schema.

### Sync columns

Standard sync columns appear on `AuditLog` only (Phase-1 syncable). The other three models are server-only operational tables and use plain Postgres timestamps.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `*` (no new routes) | n/a | App shell wraps the existing `/` and `/no-access` placeholders; real pages land in Phases 2-8. |

App shell components:

| Component | File | Purpose |
|-|-|-|
| `<AppShell>` | `src/components/shell/app-shell.tsx` | Sidebar (RTL-aware), top bar, status bar, child outlet. |
| `<Sidebar>` | `src/components/shell/sidebar.tsx` | Role-gated nav tree (stub items for Phases 2-8). |
| `<SyncPill>` | `src/components/shell/sync-pill.tsx` | Five states: idle, pushing, pulling, offline, error. |
| `<LanguageToggle>` | `src/components/shell/language-toggle.tsx` | ar / en toggle; persists in `tauri-plugin-store`. |
| `<RtlBoundary>` | `src/components/shell/rtl-boundary.tsx` | Applies `<html dir>` from i18n state. |

Zustand stores added:

| Store | File | State |
|-|-|-|
| `useSyncStatusStore` | `src/stores/sync-status-store.ts` | `{ status, pushPending, pullPending, lastError, conflicts: Conflict[] }`; updated by `sync:*` Tauri events. |
| `useDeviceStore` | `src/stores/device-store.ts` | `{ deviceId, appVersion }`; populated once at boot. |

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useSyncStatus` | `['sync','status']` | Reads current sync status snapshot. |
| `useSyncConflicts` | `['sync','conflicts']` | Lists parked conflicts (placeholder list at Phase 1; resolver UI ships in Phase 8). |

Zod schemas in `src/lib/schemas/`:

| Schema | File | Shape |
|-|-|-|
| `SyncStatusSchema` | `src/lib/schemas/sync.ts` | `z.enum(['idle','pushing','pulling','offline','error'])`. |
| `ConflictSchema` | `src/lib/schemas/sync.ts` | `{ opId, entity, entityId, localPayload, serverPayload, createdAt }`. |
| `DeviceContextSchema` | `src/lib/schemas/device.ts` | `{ deviceId, appVersion }`. |

### Tauri / Rust

Domain entity (in `src-tauri/src/domains/sync/`):

```rust
pub struct OutboxOp {
  pub op_id: Uuid,             // UUID v7
  pub entity: String,
  pub entity_id: String,
  pub op: OutboxAction,        // Upsert | Delete
  pub payload: Vec<u8>,        // MessagePack
  pub created_at: DateTime<Utc>,
  pub attempts: i32,
  pub next_attempt_at: DateTime<Utc>,
  pub last_error: Option<String>,
}
impl OutboxOp {
  pub fn try_new(entity: &str, entity_id: &str, op: OutboxAction, payload: Vec<u8>) -> Result<Self, AppError> { ... }
}

pub enum OutboxAction { Upsert, Delete }

pub struct AuditEntry {
  pub id: Uuid,
  pub actor_user_id: Uuid,
  pub action: AuditAction,    // Create | Update | SoftDelete | Lock | Void | ClockIn | ClockOut | PasswordChange
  pub entity: String,
  pub entity_id: String,
  pub delta: serde_json::Value,
  pub ip: Option<String>,
  pub device_id: String,
  pub at: DateTime<Utc>,
}
impl AuditEntry {
  pub fn try_new(...) -> Result<Self, AppError> { ... }
}
```

Repository traits (in `src-tauri/src/domains/sync/repositories/`):

```rust
#[async_trait]
pub trait OutboxRepo {
  async fn enqueue(&self, tx: &mut Tx, op: OutboxOp) -> Result<(), AppError>;
  async fn next_batch(&self, limit: usize) -> Result<Vec<OutboxOp>, AppError>;
  async fn mark_failure(&self, op_id: Uuid, error: &str, backoff: Duration) -> Result<(), AppError>;
  async fn delete_ack(&self, op_ids: &[Uuid]) -> Result<(), AppError>;
}

#[async_trait]
pub trait AuditRepo {
  async fn append(&self, tx: &mut Tx, entry: AuditEntry) -> Result<(), AppError>;
  async fn list(&self, filter: AuditFilter, page: Page) -> Result<Vec<AuditEntry>, AppError>;
}

#[async_trait]
pub trait SyncStateRepo {
  async fn get(&self) -> Result<SyncState, AppError>;
  async fn put_cursor(&self, cursor: &str) -> Result<(), AppError>;
  async fn ensure_device_id(&self) -> Result<String, AppError>;
}
```

SQLite repositories notes:

- All writes go through `sqlx::Transaction<'_, Sqlite>`. `WAL` journal mode set at pool init.
- Prepared statements cached on `SqlitePool`.
- `OutboxRepo::enqueue` and the domain row write share a single transaction; commit ordering is row first then outbox row, then the transaction commits both atomically.

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `sync::status` | none | `SyncStatusSnapshot` | Current engine state. |
| `sync::trigger_push` | none | `()` | Wakes the push loop. |
| `sync::trigger_pull` | none | `()` | Wakes the pull loop. |
| `sync::list_conflicts` | `{ limit, offset }` | `Conflict[]` | Lists parked conflicts (placeholder list at Phase 1). |
| `sync::resolve_conflict` | `{ opId, choice: 'local'|'server', merged?: Json }` | `()` | Submits a resolution; calls `/sync/conflicts/:opId/resolve`. Used by Phase 8 UI. |
| `device::info` | none | `{ deviceId, appVersion }` | Returns boot-cached device info. |

Tauri events (emitted by `SyncEngine`):

| Event | Payload | Trigger |
|-|-|-|
| `sync:status` | `SyncStatus` | State transitions. |
| `sync:progress` | `{ pushed, total }` | During drain. |
| `sync:conflict` | `Conflict` | New parked conflict received from server. |

Register all commands in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Bootstrap plugins (in `sync-server/src/app/plugins/`):

| Plugin | File | Purpose |
|-|-|-|
| `auth-jwt` | `auth-jwt.ts` | RS256 verifier (key pair stub for Phase 1; full auth flow in Phase 2). |
| `tenant` | `tenant.ts` | Decorates `request.tenantId` from JWT `entityId` claim; injects on every Prisma query. |
| `swagger` | `swagger.ts` | `@fastify/swagger` + `@fastify/swagger-ui` at `/documentation`. |
| `prisma` | `prisma.ts` | Singleton `PrismaClient` decorated on `app`. |
| `error-handler` | `error-handler.ts` | Maps domain errors to HTTP status + TypeBox `Error` schema. |

Entity classes (in `sync-server/src/domains/sync/`):

```ts
class AuditLog {
  constructor(
    readonly id: string,
    readonly actorUserId: string,
    readonly action: string,
    readonly entity: string,
    readonly entityId: string,
    readonly delta: Record<string, { from: unknown; to: unknown }>,
    readonly deviceId: string,
    readonly at: Date,
    readonly entityIdTenant: string,
    readonly ip?: string,
  ) {}
  static create(input: AuditCreateInput): AuditLog { /* validation here */ }
  toResponse(): AuditLogResponse { /* shape for /audit/query */ }
}
```

Repository interface:

```ts
interface AuditLogRepository {
  insertBatch(entries: AuditLog[]): Promise<void>;
  query(params: AuditQueryParams): Promise<{ rows: AuditLog[]; nextCursor: string | null }>;
}

interface ProcessedOpRepository {
  has(opId: string, tenantId: string): Promise<ResponseEntry | null>;
  remember(opId: string, tenantId: string, response: unknown): Promise<void>;
}

interface ConflictParkedRepository {
  park(record: ConflictRecord): Promise<void>;
  resolve(opId: string, resolution: ResolutionInput, userId: string): Promise<void>;
}
```

Prisma repo notes: every query carries `where: { entityIdTenant: request.tenantId }` via the tenant plugin decorator. Audit insertion uses `createMany` with `skipDuplicates: true` keyed on `id`.

TypeBox schemas (in `sync-server/src/schemas/`):

| Schema | Purpose |
|-|-|
| `SyncPushBodySchema` | `{ ops: PushOp[] }` with `op_id`, `entity`, `entity_id`, `op`, `payload`. |
| `SyncPushResponseSchema` | `{ accepted: string[]; conflicts: ConflictResponse[] }`. |
| `SyncPullQuerySchema` | `{ since: string }` (cursor). |
| `SyncPullResponseSchema` | `{ changes: ChangeRow[]; nextCursor: string }`. |
| `ConflictResolveBodySchema` | `{ choice: 'local'|'server'; merged?: any }`. |
| `HealthResponseSchema` | `{ status: 'ok'; version: string }`. |
| `ErrorSchema` | `{ statusCode, error, message }` baseline. |

Route table:

| Method | Path | Description |
|-|-|-|
| `GET` | `/healthz` | Liveness; no auth. |
| `POST` | `/sync/push` | Apply outbox batch; idempotent by `op_id`; returns 200 with `{ accepted, conflicts }`. |
| `GET` | `/sync/pull` | Returns changes after cursor, plus next cursor. |
| `POST` | `/sync/conflicts/:opId/resolve` | Manual resolution submission. |

All non-`/healthz` routes require the JWT plugin and the tenant decorator.

## §4 Business Logic

### Frontend

- `<AppShell>` mounts `<RtlBoundary>` to set `<html dir>` and the Tailwind logical-property flow.
- `<SyncPill>` subscribes to `sync:status` and renders the five states from PRD §10.8 (no resolver yet; pill in `error` is a passive indicator).
- Idle watcher and lock screen are stubs in Phase 1; full implementation in Phase 2.

### Tauri / Rust

`SyncEngine` Tokio task (lifecycle per `.claude/rules/offline-first.md`):

1. **Boot.** Resolve `device_id` (create if missing via `tauri-plugin-os` + `tauri-plugin-store`). Load `sync_state.pull_cursor`. Subscribe to network status.
2. **Push loop step.**
   1. Select up to 50 outbox rows where `next_attempt_at <= now` and `attempts < 10`.
   2. POST `/sync/push` with batch and `X-Device-Id`, `X-App-Version`, `Authorization: Bearer <accessToken>` headers.
   3. On 200, mark accepted `op_ids` complete (delete from outbox); for any `conflicts[]`, emit `sync:conflict` and DO NOT delete the outbox row (left for resolver-driven retry).
   4. On 401 once, refresh and retry. On second 401, emit `auth:expired` and pause loop.
   5. On 5xx, mark failure with exponential backoff (`next_attempt_at = now + min(2^attempts, 60min)`).
3. **Pull loop step.**
   1. GET `/sync/pull?since=<cursor>`.
   2. Apply each change inside a single SQLite tx: parse payload (MessagePack), look up local row, compare `version` and `updated_at`, apply per declared conflict policy.
   3. Persist `nextCursor` to `sync_state.pull_cursor` in the same tx.
4. **Realtime.** Optional SSE stream; if connected, push wake events into the pull loop. Pull remains the source of truth.
5. **Shutdown.** Cancel via `CancellationToken`. Drain in-flight HTTP. Persist cursor.

`AuditWriter::with_audit` helper:

1. Open SQLite transaction.
2. Capture `before` snapshot (read current row if exists).
3. Run caller closure that performs the mutation and returns `after` snapshot.
4. Compute delta `{ field: { from, to } }` for changed fields; omit identical fields.
5. Construct `AuditEntry` and append via `AuditRepo`.
6. Enqueue outbox row for the business entity and for the audit entry.
7. Commit. Return the caller's result.

Bare repository writes outside `with_audit` are forbidden in domain code; this is enforced by a code-review reject and a clippy lint that flags writes outside the helper (future tightening, not blocking Phase 1).

### Sync Server

`SyncPushService::apply(batch, tenantId, deviceId)`:

1. For each `op` in `batch`:
   1. Look up `ProcessedOp` by `op_id`. If hit, return remembered response shape.
   2. Decode payload (MessagePack) and validate against the entity's TypeBox schema (per phase; Phase 1 accepts no domain entities and 422s).
   3. Open Prisma transaction.
   4. Read current row by `(id, entityIdTenant)`.
   5. Apply policy:
      - `last-write-wins`: replace fields if pushed `version > local.version` or (`version =` and pushed `updated_at > local.updated_at`); tiebreak by `originDeviceId` lex.
      - `additive-only`: insert if absent; otherwise no-op.
      - `manual`: if local exists with different content and concurrent `version`, park in `ConflictParked` and respond with conflict; do NOT mutate.
   6. Insert/update row; insert audit row in same tx.
   7. Record `ProcessedOp` with the canonical response.
   8. Commit.
2. Aggregate `{ accepted, conflicts }` and return.

`SyncPullService::changes(tenantId, sinceCursor)`:

1. Pick high-water mark: `(updatedAt, id)` lex cursor.
2. Query each tenant-scoped table with `WHERE updatedAt > sinceCursor AND entityIdTenant = :tenant` ordered by `(updatedAt, id)` ascending; cap per-entity at 500 rows; aggregate to a single change stream.
3. Return `{ changes, nextCursor = max(updatedAt|id) }`.

`ConflictResolveService::resolve(opId, { choice, merged }, userId, tenantId)`:

1. Load `ConflictParked` row.
2. If `choice = 'local'`, replay the local payload as a forced upsert.
3. If `choice = 'server'`, mark the local op as dropped; respond with the canonical server row.
4. If `merged` supplied, validate against entity schema, apply as upsert.
5. Set `resolvedAt`, `resolvedByUserId`.
6. Audit `conflict_resolve` row.

### Sync Semantics in This Phase

- `audit_log` is the only Phase-1 entity that pushes via the outbox. Policy: `additive-only`. Idempotency: `op_id` (UUID v7).
- `outbox`, `sync_state`, `ProcessedOp`, `SyncCursor`, `ConflictParked` are infrastructure; NOT pushed.

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

```ts
export const TENANT_MODELS = ['audit_log'] as const;
```

`ProcessedOp`, `SyncCursor`, `ConflictParked` are excluded from `TENANT_MODELS` because they are operational tables filtered via explicit `entityIdTenant` column rather than via tenant plugin middleware.

### Audit trigger additions

None at this phase. Audit is written explicitly by the service layer via `with_audit`; no DB triggers.

### Local SQLite indexes

- `outbox_next_attempt` (partial, `attempts < 10`).
- `audit_log_entity`, `audit_log_actor`, `audit_log_at` (per PRD §6.1.15).

### Tauri capabilities

Edit `src-tauri/capabilities/default.json` to add:

- `store:default`, `stronghold:default`, `os:default`, `path:default`, `dialog:default`, `log:default`.
- No bare `http:default` capability. Sync HTTP goes via the in-process Rust `reqwest` client; the frontend never makes sync HTTP calls.

### Plugin registrations

In `src-tauri/Cargo.toml` and `src-tauri/src/lib.rs`, add:

- `tauri-plugin-sql`, `tauri-plugin-store`, `tauri-plugin-stronghold`, `tauri-plugin-os`, `tauri-plugin-dialog`, `tauri-plugin-log`.

In `sync-server/`:

- `@fastify/jwt`, `@fastify/swagger`, `@fastify/swagger-ui`, `@fastify/autoload`, `@fastify/sensible`, `@fastify/cors`, `@prisma/client`, `@sinclair/typebox`, `pino`.

### Fastify plugins / BullMQ queues

- BullMQ NOT introduced yet; nothing to schedule until Phase 8 (audit vacuum).

### What this phase does NOT touch

- No new TENANT_MODELS entries beyond `audit_log`.
- No domain entities (no `users`, no `settings`).
- No conflict resolver UI (Phase 8 builds it).
- No background jobs.

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings` passes with zero warnings.
2. `cd src-tauri && cargo test` passes; new tests in `src-tauri/tests/sync/` cover outbox enqueue, push success, push 409 conflict, push 5xx backoff, pull cursor advance, audit append.
3. `pnpm lint && pnpm build` passes.
4. `pnpm tauri dev` boots the app; `<SyncPill>` cycles `idle -> pushing -> idle` against a stubbed server response.
5. `cd sync-server && pnpm test` passes; integration tests cover `/healthz`, `/sync/push` idempotency, `/sync/pull` cursor monotonicity, `/sync/conflicts/:opId/resolve` happy path.
6. Sync round-trip smoke: enqueue an `audit_log` row offline via a Rust integration test; flip network; assert the row arrives at the Postgres `audit_log` table within 5 seconds.
7. Idempotency: push the same `op_id` twice; assert second push returns the cached response and inserts zero new rows.
8. Conflict scenario: post two competing `audit_log` writes with identical `id`; assert `additive-only` policy accepts both (no conflict) and ordering is by `created_at`.
9. Tauri capability lint: `tauri.conf.json` declares only the capabilities listed above; no `http:default` present.
10. Run existing tests: no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 14 gaps incorporated below._

### 7.1 `tauri-plugin-fs` registration and log capability scope
- **Gap:** HIGH | Missing Plugin | PRD §5.1
- The plugin list in §3.Tauri omits `tauri-plugin-fs`; PRD §5.1 requires it for receipt and log writes. The capabilities table also omits `fs:scope` for `$APPDATA/idc-system/logs/**`.
- **Resolution:** Add `tauri-plugin-fs` to `Cargo.toml`, register in `lib.rs::Builder`, and add to `capabilities/main.json`:
  ```json
  { "identifier": "fs:scope", "allow": [{ "path": "$APPDATA/idc-system/logs/**" }] }
  ```
  Receipt-path scope (`$APPDATA/idc-system/receipts/**`) is added in phase-05; log scope is owned here.

### 7.2 `AppState` struct construction
- **Gap:** MEDIUM | Missing Setup | PRD §5.1
- Phase-01 references `AppState` indirectly but never declares its construction. PRD §5.1 mandates the shape `AppState { db_pool, sync_engine_handle, user_context, settings_cache, device_id }`.
- **Resolution:** Add to §3.Tauri:
  ```rust
  // src-tauri/src/state.rs
  pub struct AppState {
      pub db_pool: SqlitePool,
      pub sync_engine: Arc<SyncEngine>,
      pub user_context: RwLock<Option<UserContext>>,    // populated in Phase 2
      pub settings_cache: RwLock<HashMap<String, SettingValue>>, // populated in Phase 2
      pub device_id: Uuid,
  }
  ```
  Wired via `builder.manage(AppState::new(...).await?)` in `lib.rs::run()`.

### 7.3 Pull-cursor ownership reconciled
- **Gap:** MEDIUM | Missing Setup | research.md
- Both `tauri-plugin-store` (PRD §5.1) and the SQLite `sync_state` table (research.md) are described as cursor stores. Ambiguity will cause double-writes.
- **Resolution:** `sync_state.pull_cursor` is authoritative; `tauri-plugin-store` carries only UI prefs (theme, last-opened tab, language toggle). Add this rule to §4.SyncEngine business logic.

### 7.4 Sync pill pending-count badge
- **Gap:** HIGH | Missing UI Element | PRD §10.8
- PRD §10.8 requires the sync pill display the count of unshipped outbox ops; §3.Frontend describes `<SyncPill>` as five-state without a badge.
- **Resolution:** Extend `<SyncPill>` to subscribe to `SyncStatusStore.queuedOps` (count of `outbox WHERE attempts < 10`) and render a numeric badge when > 0. Add IPC command `sync::outbox_count() -> u32`; the store polls it every 2s via `SyncEngine`.

### 7.5 Click-to-resolver wiring deferred to phase-08
- **Gap:** MEDIUM | Missing Behavior | PRD §10.8
- §3.Frontend says "pill in error is a passive indicator." PRD §10.8 says clicking the error state opens the resolver.
- **Resolution:** Phase-01 ships a passive pill; phase-08 wires `onClick={() => navigate('/sync/conflicts')}` once the resolver page exists. Note added to §3.Frontend and roadmap Engines table.

### 7.6 `AuditLog` server Prisma `actor` relation
- **Gap:** HIGH | Missing Server Field | PRD §6.1.15
- The Prisma `AuditLog` block in §2 omits `actor User @relation(fields: [actorUserId], references: [id])`. PRD §6.1.15 declares it. The `User` model is introduced in phase-02, so the relation must be added as a back-reference once `User` exists.
- **Resolution:** Add forward note to §2: "Relation `actor User?` added in phase-02 alongside the `User` model; null-allowed because the actor may be deleted while audit rows persist." Add to phase-02 §2: `auditEntries AuditLog[]` on `User`.

### 7.7 Audit-first write ordering inside `with_audit`
- **Gap:** HIGH | Wrong Order | PRD §4.3
- §4 step 5 of `AuditWriter::with_audit` reads "Construct AuditEntry and append... Enqueue outbox row for the business entity AND for the audit entry. Commit." This places the audit row append after the caller's business write. PRD §4.3 mandates audit-first ordering.
- **Resolution:** Restructure `with_audit` to a two-pass closure pattern:
  1. Open SQLite tx.
  2. Caller's `prepare()` returns `(before_snapshot, after_snapshot, business_writes_fn)`.
  3. AuditWriter inserts the audit row (audit-first).
  4. AuditWriter invokes `business_writes_fn(&mut tx)`.
  5. Enqueue outbox rows.
  6. Commit.
  Document the new step numbering explicitly in §4.

### 7.8 `audit_log.action` enum extensions documented
- **Gap:** MEDIUM | Missing Enum Value | PRD §6.1.15
- PRD §6.1.15 lists `create / update / soft_delete / lock / void / clock_in / clock_out / password_change`. Phases 2 and 8 introduce `login`, `logout`, `conflict_resolve`. Phase-01 audit_log local schema declares `action TEXT NOT NULL` without a CHECK, so no DB-level change is needed.
- **Resolution:** Add §1 note: "`action` is intentionally unconstrained at the SQLite layer; the documented union is `create | update | soft_delete | lock | void | clock_in | clock_out | password_change | login | logout | conflict_resolve | vacuum`. The Rust `AuditAction` enum (added here) is the source of truth and is extended in subsequent phases."

### 7.9 audit_log local tenant index
- **Gap:** MEDIUM | Missing Index | PRD §9.1
- §1 declares `audit_log_at` on `(at)` but no index on `(entity_id_tenant, at)`. Tenant-scoped audit queries (phase-08 `audit::query`) will full-scan once the table grows.
- **Resolution:** Update §1 migration:
  ```sql
  CREATE INDEX audit_log_tenant_at ON audit_log(entity_id_tenant, at DESC);
  ```
  Drop the standalone `audit_log_at` (covered by the composite).

### 7.10 i18n scaffolding ownership
- **Gap:** HIGH | Missing Setup | PRD §10.6
- PRD §10.6 mandates `src/i18n/locales/{ar,en}/translation.json` with namespaces `common / auth / reception / accounting / inventory / admin / audit / errors / receipts`. Phase-01 references `<LanguageToggle>` and `<RtlBoundary>` but never declares creation of the directory or namespace layout.
- **Resolution:** Add to §3.Frontend Setup:
  ```
  src/i18n/
    index.ts                       # i18next init; phase-02 owns the first-launch ar-forcing detector
    locales/
      ar/{common,errors,receipts}.json
      en/{common,errors,receipts}.json
  ```
  Other namespaces (`auth`, `reception`, `accounting`, `inventory`, `admin`, `audit`) are added by their owning phases.

### 7.11 WCAG 2.1 AA verification baseline
- **Gap:** HIGH | Missing A11y Requirement | PRD §10.7
- PRD §10.7 mandates WCAG 2.1 AA. No phase declares the standard or wires an automated check.
- **Resolution:** Add to §6 Verification:
  > 11. `pnpm a11y` (script added here) runs `@axe-core/cli` against the dev server's `/login` and `/no-access` routes; assert zero serious or critical violations. Each later phase extends the page list.
  Add the script to `package.json` via `pnpm add -D @axe-core/cli`. Phase-08 finalizes the full-app sweep.

### 7.12 No-phantom-toast policy
- **Gap:** LOW | Missing Banner/Pill | PRD §10.8
- PRD §10.8 says "No phantom error toasts. Network failures are absorbed by the sync engine." Phase-01 does not declare the toast-suppression policy.
- **Resolution:** Add to §3.Frontend: a single `<Toaster>` mounted in `<AppShell>` filters errors whose `Error.cause` matches `NetworkError | OfflineError`. Convention documented in `src/lib/toast.ts`.

### 7.13 `<StatusBar>` and `<Breadcrumbs>` components
- **Gap:** LOW | Missing Component | PRD §3.3
- `<AppShell>` description mentions "status bar" without declaring a `<StatusBar>` component. PRD §3.3 implies breadcrumbs on hierarchical detail pages but no `<Breadcrumbs>` is declared.
- **Resolution:** Add to §3.Frontend components table:
  ```
  | <StatusBar>   | src/components/shell/status-bar.tsx   | Footer strip with sync pill, last-synced timestamp, build version |
  | <Breadcrumbs> | src/components/shell/breadcrumbs.tsx  | Auto-derived from React Router matched routes via useMatches() |
  ```
  `Breadcrumbs` reads `useMatches()` and renders `<Link>` for each route `handle.crumb`.

### 7.14 Tracing PII-redaction layer
- **Gap:** LOW | Missing Setup | PRD §5.1
- PRD §5.1 mentions a tracing redaction layer for PII. Phase-01 mentions `tracing` and `tracing-subscriber` only.
- **Resolution:** Add to §3.Tauri:
  ```rust
  // src-tauri/src/observability.rs
  pub struct RedactionLayer;
  // Replaces field values matching `patient_name|password|token|hash|email`
  // with "[REDACTED]" in the event's serialized form.
  ```
  Registered via `tracing_subscriber::registry().with(RedactionLayer).with(fmt_layer).init()`.

### 7.15 `outbox.op` enum reconciled to upsert-only in v1
- **Gap:** CRITICAL | Missing Deletion Handling | PRD §9, §10.4
- §1 declares `outbox.op CHECK IN ('upsert','delete')` but no v1 entity uses hard delete. The `delete` value is dead code that risks future misuse.
- **Resolution:** Amend §1 outbox schema to `op TEXT NOT NULL CHECK (op = 'upsert')`. Document explicitly in §4: "All deletions in v1 are soft (the entity's `deleted_at` is set and the row is pushed as a normal upsert). The `delete` enum value is reserved for the Horizon-2 PII purge per PRD §10.4 and is not used in v1." Server-side `SyncPushService` rejects any push body with `op='delete'` with `422 unsupported_op`. Verification: a unit test asserts the server returns 422 for a `delete` op.

### 7.16 Delete-vs-edit conflict policy
- **Gap:** HIGH | Missing Conflict Policy | PRD §9 conflict matrix
- §4 SyncPullService step 2 says "compare version and updated_at, apply per declared conflict policy" but never specifies the delete-vs-edit case. Under naive LWW, an edit at T2 after a soft-delete at T1 would resurrect the deleted row.
- **Resolution:** Append to §4 SyncPullService step 2 and SyncPushService step 5: "Delete-vs-edit rule (applies to all LWW and additive-only entities): when comparing two rows where exactly one of (`local`, `incoming`) has `deleted_at IS NOT NULL`, the row with the later `updated_at` wins; on tie, the deletion wins (deleted_at NOT NULL beats NULL). For `manual` policy entities (`visits`, `settings`) a delete-vs-edit conflict is parked unconditionally in `ConflictParked`." Add §6 verification step: device A soft-deletes doctor X at T1; device B edits doctor X at T0 < T1 while offline; reconnect both; assert doctor X remains soft-deleted.

### 7.17 `outbox.parked` flag breaks retry-storm on parked conflicts
- **Gap:** HIGH | Missing Idempotency | PRD §9 idempotency
- §4 SyncEngine push loop "if server returns conflict, do NOT delete the outbox row" leaves the row eligible for the next retry tick, looping indefinitely against the same conflict envelope.
- **Resolution:** Amend §1 outbox schema with column `parked INTEGER NOT NULL DEFAULT 0 CHECK (parked IN (0,1))`. Update the partial index to `... WHERE attempts < 10 AND parked = 0`. In §4 SyncEngine push step 2.iii: when server returns `conflict` for `op_id X`, set `outbox.parked = 1` and stop retrying. The conflict resolver writes `parked = 0` via `sync::resolve_conflict` to release the row for re-push under the chosen resolution.

### 7.18 ProcessedOp retention vacuum
- **Gap:** MEDIUM | Missing Idempotency | PRD §9
- The server `ProcessedOp` table grows unbounded; every push from every device persists a row forever. A year of operation degrades push latency.
- **Resolution:** Add to §5 Infrastructure: a daily server-side job purges rows where `processedAt < now - 30 days` (max outbox retry window is 60 min). Scheduled via Fastify-managed Tokio interval at 03:30 server-local. Each run emits one `audit_log` row `action='vacuum'`, `entity='processed_ops'`, `entity_id=<system zero UUID>`, `delta={ purged_count, cutoff }`. Idempotent.

### 7.19 `SyncCursor` compound primary key
- **Gap:** MEDIUM | Missing Sync Rule | §2
- §2 declares `model SyncCursor { deviceId @id ... }` with a single cursor per device. §4 SyncPullService consumes a tenant-scoped change stream, implying one cursor per (device, tenant).
- **Resolution:** Amend §2:
  ```prisma
  model SyncCursor {
    deviceId        String
    entityIdTenant  String   @map("entity_id_tenant")
    cursor          String
    updatedAt       DateTime @updatedAt @map("updated_at") @db.Timestamptz
    @@id([deviceId, entityIdTenant])
    @@map("sync_cursors")
  }
  ```
  Cursor format: `<rfc3339_updated_at>|<id_uuid>` (composite for stability across rows updated at the same ms). Local `sync_state.pull_cursor` uses the same encoding.

### 7.20 SyncEngine startup-replay reconciles in-flight outbox
- **Gap:** MEDIUM | Missing Idempotency | PRD §9 boot
- §4 SyncEngine boot has no step that reconciles outbox rows left from a crashed prior session. A row whose push succeeded but whose ack was lost will be re-pushed, potentially re-executing side effects (receipt files, etc.).
- **Resolution:** Add §4 SyncEngine boot step 1a: "Reconcile outbox. SELECT all rows with `attempts > 0 AND parked = 0`. Batch their `op_id` values and POST `/sync/lookup-op` (new lightweight server route, body `{ op_ids: string[] }`, returns `{ found: string[] }`). For each returned `op_id`, delete the local outbox row. Remaining rows go through the normal retry loop." Add server route to §3 server-routes table: `| POST | /sync/lookup-op | Tenant-scoped op_id existence check; pure read, no side effects |`.

### 7.21 audit_log push is strict additive (server rejects deleted_at)
- **Gap:** MEDIUM | Missing Conflict Policy | PRD §10.4
- §4 declares `audit_log | additive-only` but does not say the server rejects pushes carrying `deleted_at != null`. Without this rule a local vacuum that flips `dirty=1` would silently delete the server audit row.
- **Resolution:** Append to §4 Sync Semantics row for `audit_log`: "Server REJECTS any push carrying `deleted_at != null` on an `audit_log` row with `422 audit_immutable`. The local vacuum (phase-08 §4) operates on local-only rows and never sets `dirty=1` for the soft-delete. Push policy is upsert-on-id; existing rows are updated only for sync-metadata fields."

### 7.22 Sync server URL configuration
- **Gap:** HIGH | Missing Setup | PRD §9
- The Rust `SyncEngine` calls `/sync/push` and `/sync/pull` but no phase declares how the server URL is discovered on a fresh install.
- **Resolution:** Add to §3 Frontend: `<FirstLaunchSetupModal>` (`src/components/setup/first-launch-setup.tsx`) prompts the user for the sync server URL on first run; persists via `tauri-plugin-store` under key `config/syncServerUrl`. Rust IPC: `config::set_sync_server_url(url)` and `config::get_sync_server_url()`. `SyncEngine::new(config: SyncConfig)` reads at boot; if URL is null/empty, engine boots in fully-offline mode (no push/pull attempted, pill shows `offline`). Env-var override `IDC_SYNC_SERVER_URL` for dev. Add `tauri-plugin-store` to §5 plugin registrations.

### 7.23 OS notification capability decision
- **Gap:** LOW | Missing Capability | PRD §10.8
- PRD §10.8 says sync errors surface to the user via the in-app pill. No phase explicitly declares whether an OS notification permission is registered.
- **Resolution:** Append to §5 capabilities: "v1 does NOT register `notification:default`; the in-app `<SyncPill>` is the only signal. Rationale: per PRD §10.8 the workflow is the authoritative source. Reconsider in Horizon-1 if operators report missed sync errors during long offline windows."

### 7.24 Skip-to-content link in `<AppShell>`
- **Gap:** MEDIUM | Missing A11y Requirement | WCAG 2.4.1
- WCAG 2.4.1 ("Bypass Blocks") requires a skip-to-content link. §3 Frontend `<AppShell>` declares sidebar/topbar/main but no skip link.
- **Resolution:** Add to §3 Frontend: `<SkipToContent>` is the first focusable element inside `<AppShell>`. Visually hidden via `sr-only focus:not-sr-only ...` Tailwind utilities; targets `<main id="main-content">`. i18n key `a11y.skip_to_content` added to the `common` namespace. Verified in phase-08 §7.13 a11y sweep.

### 7.25 Visible focus-ring policy on shadcn variants
- **Gap:** LOW | Missing A11y Requirement | PRD §10.7
- PRD §10.7 mandates visible `focus-visible` rings. shadcn default variants only show focus on some controls.
- **Resolution:** Add to §5 Infrastructure: override `src/components/ui/{button,icon-button,link,tabs}.tsx` to include `focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none` on every variant. Document in `SHADCN.md`. Phase-08 a11y sweep asserts axe-core `focus-visible` rule passes on every interactive component.

### 7.26 Canonical `ErrorResponseSchema` (TypeBox)
- **Gap:** MEDIUM | Missing Error Variant | PRD §5.2
- §3 Server declares an `error-handler` plugin but no canonical error-response schema; per-route Swagger docs cannot reference a single shape.
- **Resolution:** Add to §3 Server schemas:
  ```ts
  export const ErrorResponseSchema = Type.Object({
    code: Type.String(),
    message: Type.String(),
    details: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
    traceId: Type.String(),
  });
  ```
  Every route in this phase and later phases references `ErrorResponseSchema` for its 400/401/403/404/409/422/500 responses. Domain error codes: `LOCK_OPERATOR_INELIGIBLE`, `VOID_NOT_LOCKED`, `SETTINGS_REQUIRED_KEY_IMMUTABLE`, `AUDIT_IMMUTABLE`, `ADDITIVE_VIOLATION`, `UNSUPPORTED_OP`, `CONFLICT_PARKED`, etc.

### 7.27 Consolidated Rust `AppError` enum
- **Gap:** LOW | Missing Error Variant | Cross-phase
- Phases 2-8 each introduce a domain error enum (`SettingsError`, `UserError`, `ShiftError`, `LockError`, `VoidError`, ...). No top-level union exists; `#[tauri::command]` returns are not uniform.
- **Resolution:** Add `src-tauri/src/error.rs`:
  ```rust
  #[derive(thiserror::Error, Debug)]
  pub enum AppError {
      #[error(transparent)] Auth(#[from] AuthError),
      #[error(transparent)] Settings(#[from] SettingsError),
      #[error(transparent)] Sync(#[from] SyncError),
      // domain variants added by subsequent phases via #[from]
  }
  // serde::Serialize impl emits { code, message, details? } mirroring ErrorResponseSchema.
  ```
  All `#[tauri::command]` functions return `Result<T, AppError>`. Subsequent phases register their domain errors as `AppError` variants in their §3 Tauri block.

### 7.28 `metrics_events` local table for telemetry
- **Gap:** MEDIUM | Missing Telemetry | PRD §1.3
- PRD §1.3 measures lock p95, sync push p95, receipt-print success >99%. No local table records these events; the soak harness cannot verify them.
- **Resolution:** Add to §1 Local Schema:
  ```sql
  CREATE TABLE metrics_events (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN (
                       'lock_start','lock_end',
                       'receipt_print_ok','receipt_print_fail',
                       'sync_push_ok','sync_push_fail','sync_pull_ok','sync_pull_fail',
                       'sync_conflict')),
    at              TEXT NOT NULL,
    payload_json    TEXT,
    entity_id       TEXT NOT NULL
  );
  CREATE INDEX metrics_events_kind_at ON metrics_events(entity_id, kind, at);
  ```
  Non-syncable (no `version`, no `dirty`, no outbox). 30-day retention via the same vacuum that prunes `audit_log`. Phase-05 lock service emits `lock_start`/`lock_end`. Phase-08 `diagnostics::summary` IPC reads this table; soak harness reads it for p95 statistics.

### 7.29 Husky + lint-staged scaffold (consumed by phase-08 §7.10)
- **Gap:** LOW | Missing Setup | phase-08 §7.10 forward-reference
- Phase-08 §7.10 cites a Husky+lint-staged install "in phase-01 §7 follow-up". No such entry existed.
- **Resolution:** Add to §5 Infrastructure: scaffold `.husky/pre-commit` running `pnpm lint-staged`. `package.json` gains `"prepare": "husky"` and `"lint-staged": { "*.{ts,tsx}": ["eslint --fix", "pnpm i18n:scan-touched"], "src-tauri/**/*.rs": ["cargo fmt --"] }`. Devs are NOT expected to bypass with `--no-verify`; the i18n-scan invariant is documented in phase-08 §7.10.

### 7.30 Sync error i18n keys (Pass-3)
- **Gap:** MEDIUM | Missing i18n Key | Pass-3 GAP-A-2; phase-03 §7.29
- Phase-03 §7.29 catalogues `errors:*` per phase. The sync-engine and AuthService-bridged events emit AppError variants whose i18n keys were not enumerated.
- **Resolution:** Append to the phase-03 §7.29 inventory the following phase-01 row: `errors:sync.network_offline`, `errors:sync.server_unavailable`, `errors:sync.auth_expired`, `errors:sync.already_resolved` (consumed by `<SyncPill>` error state, by the `auth::session_expired` event from §7.25, and by the resolver `409 ALREADY_RESOLVED` flow from phase-08 §7.22). Phase-08 i18n lint cross-checks every `AppError` variant against this list and CI-fails if a variant lacks an entry on either locale.

### 7.31 Delete-vs-edit carve-out for `audit_log`
- **Gap:** MEDIUM | Missing Sync Rule Reconciliation | §7.16, §7.21; Pass-3 GAP-A-4
- §7.16 declares the universal delete-vs-edit rule "applies to all LWW and additive-only entities". §7.21 says `audit_log` server REJECTS any push carrying `deleted_at != null` with `422 audit_immutable`. The two coexist only by carving out `audit_log`; the carve-out was never written.
- **Resolution:** Append to §4 Sync Semantics: "Delete-vs-edit carve-out -- §7.16's universal rule applies to every syncable entity EXCEPT `audit_log`. For `audit_log`: server rejects any pushed row with `deleted_at != null` (per §7.21); the local pruner (phase-08 §4 + §7.21) sets `deleted_at` and EXPLICITLY does NOT set `dirty=1` (a separate `vacuum_unsynced_safe` repo path that bypasses the standard sync-row update closure). Verification: a unit test asserts (a) `AuditRepo::vacuum_unsynced_safe(cutoff)` does not flip `dirty` on touched rows; (b) the post-vacuum outbox contains zero `audit_log` upserts."

### 7.32 `AuditLog` server `pulledAt` column
- **Gap:** HIGH | Missing Field | Pass-3 GAP-C-1; PRD line 302
- PRD line 302 mandates a server-only `pulledAt` timestamp on every Prisma model. Phase-02 §7.17 added it to User/Setting; phase-03 §7.19 added it to all 8 catalog models. `AuditLog` (§2) has no such field and no §7.x retrofit.
- **Resolution:** Add to §2 `model AuditLog`:
  ```prisma
  pulledAt        DateTime? @map("pulled_at") @db.Timestamptz
  ```
  Set by `SyncPullService` after each successful pull batch ship to a device. Used by phase-08 §7.17 `diagnostics::summary` for "last delivered" telemetry; never returned to clients.

### 7.33 `AuditLog` server tenant-scoped indexes
- **Gap:** MEDIUM | Missing Index | Pass-3 GAP-C-6; phase-08 §7.5
- The `AuditLog` Prisma block declares `@@index([entity, entityId, at])`, `@@index([actorUserId, at])`, `@@index([at])` -- none scoped by `entityIdTenant`. Phase-08 `/audit/query` runs `WHERE entityIdTenant = :tenant ORDER BY at DESC, id DESC` and will full-scan as the table grows.
- **Resolution:** Replace the three unscoped composites in §2 `model AuditLog` with:
  ```prisma
  @@index([entityIdTenant, at(sort: Desc)])
  @@index([entityIdTenant, entity, entityId, at(sort: Desc)])
  @@index([entityIdTenant, actorUserId, at(sort: Desc)])
  ```
  The local SQLite mirror was added in §7.9; this restores symmetry on the server side.

### 7.34 SyncEngine telemetry emission points
- **Gap:** MEDIUM | Missing Telemetry | §7.28; Pass-3 GAP-F-2
- §7.28 declares `metrics_events.kind in { sync_push_ok, sync_push_fail, sync_pull_ok, sync_pull_fail, sync_conflict, ... }` but §4 SyncEngine business logic is silent on writing them. Phase-08 §7.16 soak harness asserts on `sync_conflict` rows that no producer would emit.
- **Resolution:** Append to §4 Tauri SyncEngine push/pull loops:
  - `push_step`: on 2xx response write `metrics_events { kind:'sync_push_ok', payload:{ batch_size, duration_ms } }`. On non-2xx write `kind:'sync_push_fail', payload:{ batch_size, http_status, error }`.
  - `pull_step`: mirror `sync_pull_ok` / `sync_pull_fail` with `{ batch_size, since_cursor, duration_ms }`.
  - On parked-conflict response (server returns conflict envelope), write `kind:'sync_conflict', payload:{ op_id, entity, auto_resolved:false }`.
  Inserts are non-syncable (no `dirty=1`) and use the same WAL pool. Same retention semantics as phase-08 §7.21 metrics vacuum.

### 7.35 Reserved (formerly embedded-mode env-flag gating)
- **Gap:** N/A -- the IDC system ships as a standalone Tauri app; no embedded-mode gating required.
- Section retained as a numbering placeholder so downstream §7.x cross-references remain stable.

### 7.36 `audit_log.action` enum + `daily_close_run`
- **Gap:** MEDIUM | Missing Enum Value | Pass-3 GAP-D-1; phase-07 §7.18
- Phase-07 §7.18 writes audit rows with `action='daily_close_run'` and notes "added to the application-enforced audit-action enum (phase-01 §7.8 expanded by reference)". The receipt was never written into §7.8.
- **Resolution:** Extend §7.8 union list to include `daily_close_run`. Final closed enum (14 values): `create | update | soft_delete | lock | void | discard | clock_in | clock_out | password_change | login | logout | conflict_resolve | vacuum | daily_close_run`. Update phase-08 §1 SQLite CHECK to mirror. Ensures the Rust `AuditAction` enum compiles when phase-07 attempts the write.
