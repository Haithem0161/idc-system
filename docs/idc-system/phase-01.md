# Phase 1: Tauri Spine

**Goal:** Stand up the offline-first foundation — local schema runner, audit-first transaction helper, sync engine skeleton, JWT-aware app shell with bilingual i18n + RTL — so every later phase can plug domains in without re-litigating cross-cutting concerns.

**Surfaces:** Frontend | Tauri/Rust
**Dependencies:** None (this is the seed phase).
**Complexity:** XL
**PRD references:** §3.1 (navigation tree), §3.3 (navigation pattern), §4 (architectural patterns), §5.1 (Tauri/Rust), §5.5 (Auth), §6 (sync columns + `users` + `audit_log`), §10.6 (i18n), §10.8 (offline UX).
**Decisions consumed:** D-014 (UUID v7), D-015 (90-day local audit retention), D-018 (sequential delivery), D-021 (single SQLite file, multi-user via actor scoping), D-022 (shadcn baseline), D-023 (capabilities), D-024 (tracing PII redaction), D-028 (idle lock), D-030 (pre-push validation).

---

## Section 1: Local Schema Changes (Tauri SQLite)

Migrations live at `src-tauri/migrations/NNN_<name>.sql`, applied in order on startup, recorded in `_migrations`. Idempotent + forward-only.

### Migration `001_meta.sql`

```sql
-- Migration tracking. Local-only; never synced.
CREATE TABLE IF NOT EXISTS _migrations (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  filename    TEXT NOT NULL UNIQUE,
  applied_at  TEXT NOT NULL                       -- RFC3339 UTC
);

-- Sync engine cursor + status. Local-only.
CREATE TABLE IF NOT EXISTS sync_state (
  key         TEXT PRIMARY KEY,                   -- 'pull_cursor' | 'last_push_at' | 'last_pull_at' | etc.
  value       TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);

-- Outbox per offline-first.md. Local-only.
CREATE TABLE IF NOT EXISTS outbox (
  op_id            TEXT PRIMARY KEY,             -- UUID v7
  entity           TEXT NOT NULL,
  entity_id        TEXT NOT NULL,
  op               TEXT NOT NULL CHECK (op IN ('upsert','delete')),
  payload          BLOB NOT NULL,                -- MessagePack-encoded row
  created_at       TEXT NOT NULL,
  attempts         INTEGER NOT NULL DEFAULT 0,
  next_attempt_at  TEXT NOT NULL,
  last_error       TEXT NULL
);
CREATE INDEX outbox_next_attempt
  ON outbox(next_attempt_at)
  WHERE attempts < 10;
```

### Migration `002_users.sql`

Mirrors PRD §6.1.1 verbatim.

```sql
CREATE TABLE IF NOT EXISTS users (
  id                TEXT PRIMARY KEY,
  email             TEXT NOT NULL,
  name              TEXT NOT NULL,
  password_hash     TEXT NOT NULL,
  role              TEXT NOT NULL CHECK (role IN ('superadmin','receptionist','accountant')),
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  last_login_at     TEXT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique
  ON users(entity_id, email)
  WHERE deleted_at IS NULL;
```

### Migration `003_audit_log.sql`

Mirrors PRD §6.1.15. Note column naming: tenant scope is `entity_id_tenant`; `entity_id` is the audited row id.

```sql
CREATE TABLE IF NOT EXISTS audit_log (
  id                TEXT PRIMARY KEY,
  actor_user_id     TEXT NOT NULL REFERENCES users(id),
  action            TEXT NOT NULL,                -- 'create','update','soft_delete','lock','void','clock_in','clock_out','password_change'
  entity            TEXT NOT NULL,
  entity_id         TEXT NOT NULL,
  delta             TEXT NOT NULL,                -- JSON { field: { from, to } }
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

### What this phase does NOT touch (schema)

- No PRD reference data tables (`check_types`, `doctors`, etc.) — Phase 3.
- No `operator_shifts` — Phase 4.
- No `visits` — Phase 5.
- No inventory tables — Phase 6.
- No FTS5 virtual tables — Phase 3.

---

## Section 2: Server Schema Changes (Prisma / Postgres)

**No server entries this phase.** The sync-server scaffold remains untouched until Phase 2 promotes it. The Tauri sync engine runs locally but its push/pull HTTP calls fail-soft (status pill shows `offline` and outbox holds rows) until Phase 2 lands the endpoints.

---

## Section 3: DDD Implementation

### Frontend (React)

#### New pages / routes

| Path | File | Description |
|-|-|-|
| `/login` | `src/pages/auth/login.tsx` | Email + password form. Online: posts to sync-server `/auth/login` (when present, P2). Offline: matches against cached Argon2id hash. |
| `/lock` | `src/pages/auth/lock.tsx` | Idle re-auth screen; uses cached creds. |
| `/` | redirect | Role-based redirect. |
| `/no-access` | `src/pages/no-access.tsx` | Fallback for unknown role / inactive user. |
| `/audit` | `src/pages/audit/index.tsx` | Placeholder. The full search UI lands in Phase 9. |

App shell components (rendered around all authed routes):

- `src/components/shell/AppShell.tsx` — wraps `<Outlet />` with sidebar + top bar.
- `src/components/shell/Sidebar.tsx` — role-aware (per `useAuthStore().role`).
- `src/components/shell/TopBar.tsx` — sync pill, language toggle, user menu.
- `src/components/shell/SyncPill.tsx` — reads `useSyncStatusStore`. Five states: `idle`, `pushing`, `pulling`, `offline`, `error`.
- `src/components/shell/LanguageToggle.tsx` — `ar` ↔ `en`. Calls `useLanguageStore.setLanguage`.

#### Zustand stores

| Store | Path | Persisted |
|-|-|-|
| `useAuthStore` | `src/stores/auth-store.ts` | no (in-memory; tokens fetched from Rust on hydrate) |
| `useSyncStatusStore` | `src/stores/sync-status-store.ts` | no |
| `useLanguageStore` | `src/stores/language-store.ts` | yes (`tauri-plugin-store`, per-device) |

`useThemeStore` already exists; it stays as is.

#### React Query hooks

| Hook | Key | Backing |
|-|-|-|
| `useUser()` | `['user', 'me']` | `auth_get_state` IPC |
| `useSyncStatus()` | `['sync', 'status']` | Tauri event `sync:status` (subscription, not query refetch) |

#### Zod schemas

| Schema | Path |
|-|-|
| `UserSchema` | `src/lib/schemas/user.ts` |
| `LoginFormSchema` | `src/lib/schemas/auth.ts` |
| `RoleSchema` | `src/lib/schemas/role.ts` |
| `SyncStatusSchema` | `src/lib/schemas/sync.ts` |

#### i18n bundles

`src/i18n/locales/{ar,en}/`:

- `common.json` — buttons (`save`, `cancel`, `confirm`, `discard`), statuses (`active`, `inactive`, `loading`, `error`, `empty`), generic labels (`name`, `email`, `role`, `language`).
- `auth.json` — login screen, lock screen, no-access fallback, validation errors.
- `errors.json` — domain error keys for toasts (e.g. `errors.auth.invalid_credentials`, `errors.sync.conflict`, `errors.network.offline`).

`src/i18n/index.ts` configures react-i18next; default locale `ar`; namespaces above; fallback `en` for `ar` keys missing during dev.

#### RTL plumbing

- `<html dir="rtl">` on first launch and whenever `useLanguageStore.language === 'ar'`.
- Tailwind v4 logical-property utilities (`ps-*`, `pe-*`, `ms-*`, `me-*`, `text-start`, `text-end`).
- Lint rule: ESLint plugin to flag `pl-*`, `pr-*`, `text-left`, `text-right` outside the receipt thermal-text renderer (which is fixed-width Latin/Arabic mix).

### Tauri/Rust

#### Domain entities

`src-tauri/src/domains/users/domain/user.rs`:

```rust
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub role: Role,
    pub is_active: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub sync: SyncColumns,
}

pub enum Role { Superadmin, Receptionist, Accountant }

impl User {
    pub fn try_new(email: &str, name: &str, password: &str, role: Role, entity_id: Uuid) -> Result<Self, AppError> { /* validates email format, name length, password strength, hashes via argon2id */ }
    pub fn reconstitute(row: UserRow) -> Self { /* trusts DB; no validation */ }
    pub fn matches_password(&self, candidate: &str) -> bool { /* argon2 verify */ }
    pub fn touch(&mut self, now: DateTime<Utc>) { /* bumps updated_at + version + dirty */ }
}
```

`src-tauri/src/domains/audit/domain/audit_event.rs`:

```rust
pub struct AuditEvent {
    pub id: Uuid,
    pub actor_user_id: Uuid,
    pub action: AuditAction,
    pub entity: String,
    pub entity_id: Uuid,
    pub delta: serde_json::Value,
    pub ip: Option<String>,
    pub device_id: String,
    pub at: DateTime<Utc>,
    pub sync: SyncColumns,
}

pub enum AuditAction { Create, Update, SoftDelete, Lock, Void, ClockIn, ClockOut, PasswordChange }
```

`src-tauri/src/domains/sync/domain/outbox_op.rs`:

```rust
pub struct OutboxOp {
    pub op_id: Uuid,
    pub entity: String,
    pub entity_id: Uuid,
    pub op: OutboxKind,
    pub payload: Vec<u8>,            // MessagePack
    pub created_at: DateTime<Utc>,
    pub attempts: i64,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

pub enum OutboxKind { Upsert, Delete }
```

#### Repository traits

`src-tauri/src/domains/users/domain/repositories.rs`:

```rust
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError>;
    async fn upsert(&self, user: &User) -> Result<(), AppError>;
}
```

`src-tauri/src/domains/audit/domain/repositories.rs`:

```rust
#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn append(&self, tx: &mut sqlx::Transaction<'_, Sqlite>, event: &AuditEvent) -> Result<(), AppError>;
    async fn list_for_entity(&self, entity: &str, entity_id: Uuid) -> Result<Vec<AuditEvent>, AppError>;
}
```

`src-tauri/src/domains/sync/domain/repositories.rs`:

```rust
#[async_trait]
pub trait OutboxRepository: Send + Sync {
    async fn enqueue(&self, tx: &mut sqlx::Transaction<'_, Sqlite>, op: &OutboxOp) -> Result<(), AppError>;
    async fn next_batch(&self, limit: i64) -> Result<Vec<OutboxOp>, AppError>;
    async fn mark_pushed(&self, op_ids: &[Uuid]) -> Result<(), AppError>;
    async fn mark_failed(&self, op_id: Uuid, error: &str, retry_at: DateTime<Utc>) -> Result<(), AppError>;
    async fn pending_count(&self) -> Result<i64, AppError>;
}
```

#### SQLite repositories

- `src-tauri/src/domains/users/infrastructure/sqlite_user_repo.rs` — `sqlx::query_as!` for typed reads; `sqlx::query!` for writes.
- `src-tauri/src/domains/audit/infrastructure/sqlite_audit_repo.rs`.
- `src-tauri/src/domains/sync/infrastructure/sqlite_outbox_repo.rs`.

All three use prepared statements; transactions opened by services and threaded through.

#### Tauri commands

| Command | Args | Returns | Description |
|-|-|-|-|
| `auth_login` | `{ email: String, password: String }` | `Result<UserProfile, AppError>` | Online: POST `/auth/login`, cache tokens + Argon2id hash in stronghold. Offline: verify cached hash; mark session offline-only. |
| `auth_logout` | `()` | `Result<(), AppError>` | Best-effort POST `/auth/logout`; clear stronghold tokens; rotate `UserContext`. |
| `auth_get_state` | `()` | `Result<AuthState, AppError>` | Returns `{ user, status: 'authenticated'|'offline_authenticated'|'expired'|'anonymous' }`. |
| `auth_lock` | `()` | `Result<(), AppError>` | Locks app to `/lock`; fired by idle timer. |
| `auth_unlock` | `{ password: String }` | `Result<(), AppError>` | Verifies password against cached hash; unlocks. |
| `device_id_get` | `()` | `Result<String, AppError>` | Reads or generates device id (persisted via `tauri-plugin-store`). |
| `sync_status_get` | `()` | `Result<SyncStatus, AppError>` | Snapshot of `{ state, pending_count, last_synced_at }`. |
| `sync_force_push` | `()` | `Result<(), AppError>` | Manual drain; useful when staff knows the network just came back. |
| `sync_force_pull` | `()` | `Result<(), AppError>` | Manual pull. |
| `sync_resolve_conflict` | `{ op_id: Uuid, choice: ConflictChoice, manual_payload?: Json }` | `Result<(), AppError>` | Applies a manual conflict resolution. UI lands fully in Phase 9; the IPC exists from Phase 1 because the engine emits conflicts. |

All commands registered in `src-tauri/src/lib.rs::run()` via `tauri::generate_handler!`.

### Sync Server (Fastify)

**N/A this phase.** Phase 2 lands the server.

---

## Section 4: Business Logic

### `MigrationRunner`
File: `src-tauri/src/db/migrations.rs`. Single public entry: `apply(pool: &SqlitePool) -> Result<(), AppError>`.

Steps:
1. Open a connection; ensure `_migrations` table exists.
2. List `migrations/*.sql` from a compile-time `include_dir!` macro embed (so the binary ships migrations).
3. For each not-yet-applied filename (sorted lexicographically):
   1. Open a transaction.
   2. Execute the file as a single `EXECUTE_MANY` statement.
   3. Insert `(filename, NOW())` into `_migrations`.
   4. Commit.
4. Log applied count at `info!`.

PRAGMAs set on every connection (per `rust.md`): `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON`, `busy_timeout = 5000`.

### `WithAuditTxn`
File: `src-tauri/src/services/with_audit.rs`. Generic transactional helper.

```rust
pub async fn with_audit<F, T>(
    pool: &SqlitePool,
    actor_user_id: Uuid,
    device_id: &str,
    action: AuditAction,
    entity: &str,
    entity_id: Uuid,
    f: F,
) -> Result<T, AppError>
where
    F: for<'c> FnOnce(&'c mut sqlx::Transaction<'_, Sqlite>) -> Pin<Box<dyn Future<Output = Result<(T, serde_json::Value), AppError>> + Send + 'c>>,
```

Steps:
1. Begin transaction.
2. Run `f`, which returns `(result, delta_json)`.
3. Append an `audit_log` row with the delta in the same transaction.
4. Commit. On error, the transaction rolls back atomically (mutation + audit row).

**Rule:** every write in any later phase goes through this helper. Bare `sqlx::query` writes are a code-review reject.

### `SyncEngine`
Files: `src-tauri/src/sync/engine.rs`, `pusher.rs`, `puller.rs`.

`SyncEngine::start(state: AppState, cancel: CancellationToken) -> JoinHandle<()>`:

1. Spawn `pusher_loop` (every 2s while not cancelled, drain outbox in batches of 50).
2. Spawn `puller_loop` (every 10s while not cancelled, GET `/sync/pull?since=cursor`).
3. Both emit `sync:status` Tauri events on state change.

`pusher_loop` step sequence:
1. `OutboxRepo::next_batch(50)`.
2. POST `/sync/push` with MessagePack body of the batch.
3. On 200: `OutboxRepo::mark_pushed(...)`.
4. On 401: trigger `AuthService::refresh`; retry once; if still 401 emit `sync:status = error` and pause.
5. On 409 (conflict): write conflict rows to a local `sync_conflicts` table (added in Phase 9 — for Phase 1 we just log + emit the event).
6. On 5xx / network error: `mark_failed` with exponential backoff (`next_attempt_at = now + min(2^attempts seconds, 5min)`).

Until Phase 2 ships the server, both loops will hit network errors and retry indefinitely. The status pill correctly shows `offline`. This is the intended fail-soft behavior.

### `AuthService`
File: `src-tauri/src/domains/auth/services/auth_service.rs`.

Methods:
- `login(email, password) -> AuthState` — online first, falls back to offline if network fails AND a cached Argon2id hash exists for this email.
- `logout()` — clears stronghold; emits `auth:state` event.
- `refresh()` — POST `/auth/refresh`; rotates tokens; updates stronghold.
- `lock()` — clears in-memory access token; sets state to `locked`.
- `unlock(password)` — verifies cached hash; restores access token from stronghold.
- `cache_credentials(email, password)` — Argon2id hash + secure storage write; invoked after every successful online login.

Idle lock timer: managed in frontend (`src/providers/idle-lock-provider.tsx`); calls `auth_lock` IPC after `settings.idle_lock_minutes` of input idleness (default 10).

### `DeviceIdProvider`
File: `src-tauri/src/services/device_id.rs`. Reads or generates a UUID v7 via `tauri-plugin-store` keyed `device.id`. Persistent across app launches.

### `AppState`
File: `src-tauri/src/state.rs`.

```rust
pub struct AppState {
    pub db_pool: Arc<SqlitePool>,
    pub user_context: RwLock<Option<UserContext>>,
    pub device_id: String,
    pub sync_engine: Arc<SyncEngineHandle>,
    pub settings_cache: RwLock<HashMap<String, String>>,   // populated in Phase 3
}
```

Wired into Tauri via `app.manage(Arc::new(state))`. Commands receive `tauri::State<'_, Arc<AppState>>`.

### `RoleGate` (frontend)
File: `src/providers/role-gate.tsx`. Wraps the route element in `useAuthStore().role` checks. If the role is missing or doesn't include the route's required role, redirect to `/no-access`.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions on the server
None this phase (sync server lands in P2). Note for P2 author: TENANT_MODELS at end of P2 = `[User, AuditLog]`.

### Audit triggers
None. The `with_audit` helper runs in app code, not as DB triggers.

### Local SQLite indexes added
- `outbox_next_attempt` on `outbox(next_attempt_at) WHERE attempts < 10`.
- `users_email_unique` on `users(entity_id, email) WHERE deleted_at IS NULL`.
- `audit_log_entity` on `audit_log(entity, entity_id, at)`.
- `audit_log_actor` on `audit_log(actor_user_id, at)`.
- `audit_log_at` on `audit_log(at)`.

### Tauri capabilities (`src-tauri/capabilities/default.json`)

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "IDC default capabilities",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "log:default",
    "os:default",
    "store:default",
    "stronghold:default",
    "sql:default",
    { "identifier": "fs:scope", "allow": [{ "path": "$APPDATA/idc-system/receipts/**" }] },
    { "identifier": "fs:scope", "allow": [{ "path": "$APPDATA/idc-system/logs/**" }] },
    "dialog:save",
    "dialog:open"
  ]
}
```

No bare `http`, no bare `fs:default`, no `shell:default` (per D-023).

### New Tauri plugin registrations (`src-tauri/src/lib.rs`)

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_log::Builder::new().level(tracing::Level::INFO).build())
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_store::Builder::default().build())
    .plugin(tauri_plugin_stronghold::Builder::new(|password| { /* derive key */ }).build())
    .plugin(tauri_plugin_sql::Builder::default().build())
    .plugin(tauri_plugin_dialog::init())
    .manage(app_state)
    .invoke_handler(tauri::generate_handler![ /* commands */ ])
    .run(tauri::generate_context!())
    .expect("tauri runtime failed");
```

### New Fastify plugins / queues
None this phase.

### Logging setup (Rust)
File: `src-tauri/src/observability/tracing.rs`. JSON file appender at `$APPDATA/idc-system/logs/idc-YYYY-MM-DD.log`. Custom `Layer` redacts fields named `password`, `password_hash`, `token`, `refresh_token`, `patient_name` from `info!`-level events. Full payloads only at `debug!` behind a `--features verbose-logs` Cargo feature.

---

## Section 6: Verification

Run these in order; do not move to Phase 2 until all pass.

1. **Rust lint clean.**
   ```bash
   cd src-tauri && cargo clippy --all-targets -- -D warnings
   ```
2. **Rust formatter clean.**
   ```bash
   cd src-tauri && cargo fmt --check
   ```
3. **Rust tests pass.**
   ```bash
   cd src-tauri && cargo test
   ```
   At minimum: `MigrationRunner` integration test (apply twice — second is a no-op), `WithAuditTxn` test (mutation + audit row commit atomically; failure rolls back both), `OutboxRepo` round-trip test, `User::try_new` validation tests.
4. **Frontend lint + build clean.**
   ```bash
   pnpm lint && pnpm build
   ```
5. **Desktop app boots.**
   ```bash
   pnpm tauri dev
   ```
   Smoke-test:
   - Login screen renders in Arabic by default with RTL.
   - Switching to English mirrors layout instantly; reload preserves choice.
   - Successful login (against a hardcoded test user seeded via a one-off migration in dev) lands on `/no-access` (since no module routes exist yet) and the sync pill cycles to `offline` (no server). No errors in DevTools.
   - `auth_lock` IPC triggered manually; redirects to `/lock`; entering correct password returns to previous route.
   - Closing the app and reopening it offline: cached login still works (stronghold + Argon2id verify).
6. **Sync engine fail-soft.** With no server reachable, the outbox holds rows (smoke-add via a Tauri-dev helper); status pill is `offline`; no toast spam.
7. **Audit-first invariant.** A manual `sqlx::query!("INSERT INTO users ...")` outside `with_audit` is rejected by code review (no automated check yet; lint rule lands in Phase 9 alongside the audit page).
8. **i18n coverage.** Every string in `src/pages/auth/*` and `src/components/shell/*` resolves through `t('namespace.key')`. `pnpm dlx i18next-parser` scan reports zero hardcoded UI strings.
9. **RTL spot check.** Every shadcn baseline component (Section 3) renders correctly with `<html dir="rtl">`. Chevrons in `Select` and `DropdownMenu` mirror.
10. **Pre-push composite.**
    ```bash
    pnpm lint && pnpm build && (cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test)
    ```

### What this phase explicitly does NOT verify

- Sync round-trip with the real server (Phase 3 is the first phase that round-trips a real domain entity).
- Conflict resolver UI (Phase 9).
- Daily vacuum job for `audit_log` (Phase 9).
- Pre-push validation script `tools/pre-push-check.sh` (Phase 10 lands the file; the equivalent commands run manually until then).
- Any reception / accounting / inventory / admin module surfaces.

### Frontend summary update
After this phase passes verification, bump `docs/idc-system/frontend-summary.md` Sections 1, 2, 3, 5, 6 to reflect the Phase-1 deliverables actually shipped (the seed entries become "verified"). Bump `docs/idc-system/status.md` Phase 1 row to `Completed` with counters set. Per `planning.md`, this update is part of completing the phase, not a follow-up.

---

## Section 7: PRD Gap Additions

### 7.1 Accessibility (WCAG 2.1 AA) verification — MEDIUM
**Gap:** PRD §10.7 mandates WCAG 2.1 AA (keyboard nav, focus rings, screen-reader labels, color-contrast). Phase 1 verification didn't include an explicit accessibility pass.
**Category:** Missing Verification.
**Remediation:** Append to Phase 1 Section 6 verification:
- Run `axe-core` browser test against `/login`, `/lock`, `/no-access`, `/audit` placeholder.
- Manual keyboard-only walkthrough: Tab through login form, lock screen, app shell sidebar, language toggle.
- Verify every icon-only button has an `aria-label` keyed via `t('common.aria.<action>')`.
- Verify focus rings via `:focus-visible` Tailwind utility on every interactive element.
- Carry forward as a per-phase verification rule from P3 onward.

### 7.2 MFA explicitly out of v1 — LOW
**Gap:** Research Q-004 left MFA open. PRD §5.5 cites RS256 + cached creds without specifying MFA. The auth.md rule references MFA flows, which could be misread as in-scope.
**Category:** Missing Setup decision.
**Remediation:**
- Document in `research.md` D-031 (added during Pass 1): MFA is **out of scope for v1** — single-tenant + physical-access threat model. The auth.md `/auth/mfa` route is implemented as a 501 in Phase 2 to keep the surface stable for Horizon-2.
- Phase 1 `auth_login` IPC: success returns `{ user, status: 'authenticated' }` with no `mfa_required` field. No UI for MFA in Phase 1.

### 7.3 In-memory conflict queue (Pass-V+) — LOW
**Gap:** Phase 3 §7.3 references "a Phase 1 in-memory queue" that holds manual conflicts until P9 ships the persistent `sync_conflicts` table. P1 §4 `SyncEngine` only documents "log + emit event" with no actual queue.
**Category:** Missing Logic.
**Remediation:** In Phase 1 §4 `SyncEngine`, add a `conflicts: Mutex<VecDeque<ConflictRow>>` field on the engine handle. The pusher loop's 409 branch enqueues to this `VecDeque` (cap 100; oldest-evicted with a warning) and emits `sync:conflict`. The frontend `useUnresolvedConflicts` query (lands fully in P9) reads via a P1-shipped `sync_conflicts_list` IPC that snapshots the deque. From P9 onward the deque is replaced with the SQLite `sync_conflicts` table; in-memory rows are migrated on first P9 boot.
