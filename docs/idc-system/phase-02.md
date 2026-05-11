# Phase 2: Authentication & Users

**Goal:** Ship the full authentication surface (online + offline login, refresh, change password, lock screen) along with the `users` and `settings` entities, the Login and No-Access pages, the Admin Users CRUD, and the Admin Settings form.

**Surfaces:** All
**Dependencies:** Phase 01
**Complexity:** L

## §1 Local Schema Changes (Tauri SQLite)

Migration file: `src-tauri/migrations/002_users_settings.sql`.

### users (PRD §6.1.1)

```sql
CREATE TABLE users (
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
CREATE UNIQUE INDEX users_email_unique ON users(entity_id, email) WHERE deleted_at IS NULL;
```

### settings (PRD §6.1.11)

```sql
CREATE TABLE settings (
  id                TEXT PRIMARY KEY,
  key               TEXT NOT NULL,
  value             TEXT NOT NULL,
  value_type        TEXT NOT NULL CHECK (value_type IN ('int','decimal','text','bool')),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX settings_key ON settings(entity_id, key) WHERE deleted_at IS NULL;
```

### Required settings seeded on first run

| Key | Type | Default |
|-|-|-|
| `dye_cost_iqd` | int | 10000 |
| `report_cost_iqd` | int | 10000 |
| `internal_doctor_pct` | int | 30 |
| `idle_lock_minutes` | int | 10 |
| `arabic_numerals` | bool | false |
| `clinic_display_name_ar` | text | (empty) |
| `clinic_display_name_en` | text | (empty) |
| `currency_symbol` | text | `د.ع` |

Seed runs as the last statement of `002_users_settings.sql` using `INSERT OR IGNORE`.

### Modified tables

`audit_log`: no schema change. The Phase-1 deferred FK from `actor_user_id -> users(id)` is added here via SQLite table rebuild (PRAGMA `legacy_alter_table` + `CREATE TABLE audit_log_new ... ; INSERT INTO audit_log_new SELECT * FROM audit_log; DROP audit_log; ALTER TABLE audit_log_new RENAME TO audit_log;`). Indexes recreated identically. The rebuild is idempotent because it runs only if `PRAGMA foreign_key_list(audit_log)` lacks the FK.

## §2 Server Schema Changes (Prisma / Postgres)

### User (PRD §6.1.1)

```prisma
model User {
  id              String    @id
  email           String
  name            String
  passwordHash    String    @map("password_hash")
  role            UserRole
  isActive        Boolean   @default(true) @map("is_active")
  lastLoginAt     DateTime? @map("last_login_at") @db.Timestamptz
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  refreshTokens   RefreshToken[]
  auditEntries    AuditLog[]    @relation("AuditActor")

  @@unique([entityId, email], name: "user_email_unique")
  @@map("users")
}

enum UserRole {
  superadmin
  receptionist
  accountant
}
```

### Setting (PRD §6.1.11)

```prisma
model Setting {
  id              String        @id
  key             String
  value           String
  valueType       SettingType   @map("value_type")
  createdAt       DateTime      @map("created_at") @db.Timestamptz
  updatedAt       DateTime      @map("updated_at") @db.Timestamptz
  deletedAt       DateTime?     @map("deleted_at") @db.Timestamptz
  version         Int           @default(0)
  lastSyncedAt    DateTime?     @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?       @map("origin_device_id")
  entityId        String        @map("entity_id")

  @@unique([entityId, key])
  @@map("settings")
}

enum SettingType {
  int
  decimal
  text
  bool
}
```

### RefreshToken (server-only)

```prisma
model RefreshToken {
  id              String    @id @default(uuid())
  userId          String    @map("user_id")
  tokenHash       String    @unique @map("token_hash")
  entityIdTenant  String    @map("entity_id_tenant")
  expiresAt       DateTime  @map("expires_at") @db.Timestamptz
  revokedAt       DateTime? @map("revoked_at") @db.Timestamptz
  createdAt       DateTime  @default(now()) @map("created_at") @db.Timestamptz
  deviceId        String?   @map("device_id")

  user            User      @relation(fields: [userId], references: [id])

  @@index([entityIdTenant, userId])
  @@map("refresh_tokens")
}
```

Refresh-token rotation: every `/auth/refresh` revokes the presented token and issues a fresh one in the same transaction.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/login` | `src/pages/auth/login.tsx` | Email + password form. Falls back to offline cache when server unreachable. |
| `/no-access` | `src/pages/auth/no-access.tsx` | "Contact your administrator" message; only route a user with unknown role can reach. |
| `/lock` | `src/pages/auth/lock.tsx` | Re-auth screen after idle timeout. Works offline. |
| `/admin/users` | `src/pages/admin/users/list.tsx` | Users list with role chips and active toggle. |
| `/admin/users/:id` | `src/pages/admin/users/detail.tsx` | User edit + password reset button. |
| `/admin/settings` | `src/pages/admin/settings.tsx` | Form for the v1 required settings keys. |

Routing additions in `src/routes/index.tsx`: register the six new routes; Login + Lock outside the `<AppShell>` wrapper, Admin routes inside it with role gate.

Zustand stores:

| Store | File | State |
|-|-|-|
| `useAuthStore` | `src/stores/auth-store.ts` | `{ user, role, accessToken, refreshToken, locked: boolean }`; persisted via auth-provider, NOT in plain `localStorage`. |
| `useIdleStore` | `src/stores/idle-store.ts` | `{ lastActivityAt, idleLockMinutes }`. |

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useCurrentUser` | `['auth','currentUser']` | Reads the in-memory user; not network-backed. |
| `useUsersList` | `['users','list']` | Lists users from local SQLite. |
| `useUser(id)` | `['users','detail', id]` | Single user. |
| `useUserCreate` / `useUserUpdate` / `useUserSoftDelete` / `useUserResetPassword` | mutations | IPC calls. |
| `useSettings` | `['settings','all']` | Reads all settings rows from local SQLite. |
| `useSettingUpdate` | mutation | Edits one key. |

Zod schemas in `src/lib/schemas/`:

| Schema | File | Shape |
|-|-|-|
| `LoginSchema` | `src/lib/schemas/auth.ts` | `{ email: z.string().email(); password: z.string().min(8) }`. |
| `UserSchema` | `src/lib/schemas/user.ts` | All `users` columns; `role` is the three-value enum. |
| `UserCreateSchema` | `src/lib/schemas/user.ts` | `{ email, name, role, password }`. |
| `SettingSchema` | `src/lib/schemas/setting.ts` | `{ key, value, valueType }` with value coerced by `valueType`. |
| `SettingsBundleSchema` | `src/lib/schemas/setting.ts` | Map of known v1 keys to typed values. |

### Tauri / Rust

Domain entities (in `src-tauri/src/domains/auth/` and `src-tauri/src/domains/settings/`):

```rust
pub struct User {
  pub id: Uuid,
  pub email: String,
  pub name: String,
  pub password_hash: String,
  pub role: UserRole,
  pub is_active: bool,
  pub last_login_at: Option<DateTime<Utc>>,
  pub entity_id: String,
}
impl User {
  pub fn try_new(email: &str, name: &str, role: UserRole, password: &str) -> Result<Self, AppError> {
    // Argon2id hash via argon2 crate; normalize email lower; validate role.
  }
  pub fn authenticate(&self, password: &str) -> Result<(), AppError> { /* argon2id verify */ }
  pub fn deactivate(self) -> Self { /* sets is_active = false */ }
}

pub enum UserRole { Superadmin, Receptionist, Accountant }

pub struct Setting {
  pub id: Uuid,
  pub key: String,
  pub value: SettingValue,
  pub entity_id: String,
}
pub enum SettingValue { Int(i64), Decimal(String), Text(String), Bool(bool) }
impl Setting {
  pub fn try_new(key: &str, value: SettingValue) -> Result<Self, AppError> { /* validate key in v1 set */ }
}
```

Repository traits:

```rust
#[async_trait]
pub trait UserRepo {
  async fn create(&self, tx: &mut Tx, user: User) -> Result<(), AppError>;
  async fn update(&self, tx: &mut Tx, user: User) -> Result<(), AppError>;
  async fn soft_delete(&self, tx: &mut Tx, id: Uuid) -> Result<(), AppError>;
  async fn get_by_id(&self, id: Uuid) -> Result<Option<User>, AppError>;
  async fn get_by_email(&self, email: &str) -> Result<Option<User>, AppError>;
  async fn list(&self, filter: UserFilter) -> Result<Vec<User>, AppError>;
}

#[async_trait]
pub trait SettingRepo {
  async fn get(&self, key: &str) -> Result<Option<Setting>, AppError>;
  async fn put(&self, tx: &mut Tx, setting: Setting) -> Result<(), AppError>;
  async fn list(&self) -> Result<Vec<Setting>, AppError>;
}
```

SQLite repo notes:

- `password_hash` never logged; the `tracing` redaction layer matches the `password` and `password_hash` field names.
- `users` writes always go through `with_audit` from Phase 1.
- `settings` writes record both `before` and `after` value and value_type in the audit delta.

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `auth::login` | `{ email, password }` | `LoginResult { user, role, mode: 'online'\|'offline' }` | Tries online via `/auth/login`; on failure falls back to stronghold cached hash. |
| `auth::refresh` | none | `{ accessToken, refreshToken }` | Rotates tokens via `/auth/refresh`. |
| `auth::logout` | none | `()` | Revokes server token; clears in-memory user. |
| `auth::change_password` | `{ oldPassword, newPassword }` | `()` | Online only. Invalidates stronghold cache; recaches on next online login. |
| `auth::current_user` | none | `Option<User>` | Returns in-memory current user. |
| `auth::lock` | none | `()` | Sets `locked = true` (lock screen). |
| `auth::unlock` | `{ password }` | `()` | Compares to stronghold cache; unlocks. |
| `users::list` | `{ includeInactive?: bool }` | `User[]` | Lists from local SQLite. |
| `users::get` | `{ id }` | `User` | Single user. |
| `users::create` | `UserCreateInput` | `User` | Inserts a row; audits `create`; enqueues outbox. |
| `users::update` | `UserUpdateInput` | `User` | Updates with audit delta. |
| `users::soft_delete` | `{ id }` | `()` | Soft-delete + `is_active = false`. |
| `users::reset_password` | `{ id, newPassword }` | `()` | Sets new hash; emits `password_change` audit. |
| `settings::list` | none | `Setting[]` | All settings rows. |
| `settings::get` | `{ key }` | `Option<Setting>` | Single key. |
| `settings::update` | `{ key, value, valueType }` | `Setting` | Upsert by `(entity_id, key)`. |

Register all in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Entity classes:

```ts
class User {
  static create(input): User { /* normalize email, validate role, Argon2id hash on server */ }
  toResponse(): UserResponse { /* never include password_hash */ }
}

class Setting {
  static create(input): Setting { /* validate key + valueType */ }
  toResponse(): SettingResponse { /* parse value per valueType */ }
}
```

Repository interfaces:

```ts
interface UserRepository {
  create(user: User): Promise<User>;
  update(user: User): Promise<User>;
  softDelete(id: string, tenantId: string): Promise<void>;
  getById(id: string, tenantId: string): Promise<User | null>;
  getByEmail(email: string, tenantId: string): Promise<User | null>;
  list(params: UserListParams): Promise<User[]>;
}

interface SettingRepository {
  upsertByKey(setting: Setting): Promise<Setting>;
  getByKey(key: string, tenantId: string): Promise<Setting | null>;
  list(tenantId: string): Promise<Setting[]>;
}

interface RefreshTokenRepository {
  issue(userId: string, tenantId: string, deviceId: string): Promise<{ token: string; expiresAt: Date }>;
  rotate(presentedToken: string, deviceId: string): Promise<{ token: string; expiresAt: Date }>;
  revoke(presentedToken: string): Promise<void>;
  revokeAllForUser(userId: string): Promise<void>;
}
```

TypeBox schemas:

| Schema | Purpose |
|-|-|
| `LoginBodySchema` | `{ email, password }`. |
| `LoginResponseSchema` | `{ accessToken, refreshToken, user, role, publicKey }`. |
| `RefreshBodySchema` | `{ refreshToken }`. |
| `RefreshResponseSchema` | `{ accessToken, refreshToken }`. |
| `ChangePasswordBodySchema` | `{ oldPassword, newPassword }`. |
| `UserResponseSchema` | `users` row sans `password_hash`. |
| `SettingResponseSchema` | `settings` row with parsed `value`. |

Route table:

| Method | Path | Description |
|-|-|-|
| `POST` | `/auth/login` | Email + password to access + refresh tokens. |
| `POST` | `/auth/refresh` | Rotates refresh token; revokes presented one atomically. |
| `POST` | `/auth/logout` | Revokes refresh token. |
| `POST` | `/auth/change-password` | Verifies old; rehashes new; revokes all refresh tokens for user. |

Users and settings flow through `/sync/push` and `/sync/pull`; no dedicated `/users` or `/settings` REST endpoints.

## §4 Business Logic

### Frontend

`<LoginForm>`:

1. Validate via `LoginSchema`.
2. Call `auth::login`.
3. On `mode: 'online'`, navigate to role-default route (`/reception` for receptionist/superadmin; `/accounting` for accountant).
4. On `mode: 'offline'`, navigate similarly and show a passive "Offline session" indicator near the user menu.
5. On unknown role from server, navigate to `/no-access`.

`<IdleWatcher>` (mounted inside `<AppShell>`):

1. Resets timer on `mousemove`, `keydown`, `click`, `touchstart`.
2. On timeout (= `settings.idle_lock_minutes`), dispatches `auth::lock` and routes to `/lock`.
3. Lock screen prompts for password and calls `auth::unlock`.

### Tauri / Rust

`AuthService::login(email, password)`:

1. Try online via `reqwest` POST `/auth/login`.
2. On 200: persist server response. Cache an Argon2id hash of the password in stronghold under `creds/<email>`. Cache the JWT public key in stronghold under `jwt/publicKey`. Set in-memory `UserContext`. Audit `login` (additive entry on `audit_log` with action `login`; not in PRD enum, so add `login` action enum value to the audit action union for this phase). Return `LoginResult{ mode: 'online' }`.
3. On network failure: look up `creds/<email>` in stronghold; if present, verify Argon2id; on success, populate `UserContext` from the locally cached `users` row; return `LoginResult{ mode: 'offline' }`.
4. On invalid creds online: return `LoginError::Invalid`. Do NOT fall back to offline.

`AuthService::refresh()`:

1. POST `/auth/refresh` with the refresh token.
2. On 200, persist new tokens; emit `auth:refreshed` event.
3. On 401, surface `auth:expired` and pause sync push.

`AuthService::change_password(old, new)`:

1. Online required. POST `/auth/change-password`.
2. On 200, recompute stronghold cache: store new Argon2id hash under `creds/<email>`.
3. Audit `password_change`.

`UserService::create(input)`:

1. Caller must have role `superadmin`.
2. Hash the supplied initial password with Argon2id.
3. `with_audit(action='create', entity='users', entity_id=new_id)` inserting the row; outbox enqueued in the same tx.

`UserService::reset_password(id, new)`:

1. Superadmin only.
2. Update `password_hash`; bump `version`; mark `dirty=1`.
3. On the server, revoke all refresh tokens for the user (server applies on `/sync/push` receipt of password_hash change).
4. Audit `password_change`.

`SettingsService::update(key, value)`:

1. Validate `value` against `valueType`.
2. `with_audit(action='update', entity='settings', entity_id=row.id)` upserting the row.
3. Conflict policy: `manual`. If the local push receives 409 from the server, surface a `sync:conflict` event for the resolver (Phase 8).

### Sync Server

`AuthService::login(email, password, deviceId, tenantId)`:

1. Lookup user by `(entityId, email)` lowercase.
2. Argon2id verify.
3. Issue RS256 access token with claims `{ sub, role, entityId, deviceId, iat, exp }`. 15-minute lifetime.
4. Issue refresh token (opaque random 32-byte string); store hash via `RefreshTokenRepository`; 30-day lifetime.
5. Return `{ accessToken, refreshToken, user, role, publicKey }`.

`AuthService::refresh(presented, deviceId)`:

1. Look up `RefreshToken` by `tokenHash`.
2. Reject if `revokedAt != null` or `expiresAt < now`.
3. In a transaction: set `revokedAt = now`; insert new refresh token; issue new access token.
4. Return new pair.

`AuthService::changePassword(userId, oldPlain, newPlain)`:

1. Verify old.
2. Rehash new (Argon2id, server-side parameters).
3. Update user; bump `version`; revoke all `RefreshToken` rows for user.

### Sync Semantics

| Entity | Policy | Idempotency | Notes |
|-|-|-|-|
| `users` | `last-write-wins` | `op_id` | `password_hash` server-canonical; pushed but the server only accepts the hash if the writing client is `superadmin` per JWT claim. |
| `settings` | `manual` | `op_id` | Concurrent edits surface as 409; resolver in Phase 8. |

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

```ts
export const TENANT_MODELS = ['audit_log', 'users', 'settings'] as const;
```

### Audit trigger additions

None. Auth domain writes via `with_audit` like every other domain.

### Local SQLite indexes

- `users_email_unique` (partial, `WHERE deleted_at IS NULL`).
- `settings_key` (partial, `WHERE deleted_at IS NULL`).

### Tauri capabilities

No new capability scopes beyond Phase 1 (stronghold + store + os already added).

### Plugin registrations

- Argon2id via `argon2` crate (already implied; ensure feature flags `std`, `password-hash`).
- `jsonwebtoken` crate added for RS256 token verification on the client side.

### Fastify plugins / BullMQ queues

- No BullMQ yet.
- `@fastify/jwt` configured with the loaded RS256 key pair.

### What this phase does NOT touch

- No reference data (catalog).
- No shifts, visits, patients, inventory, audit page.
- No conflict resolver UI (Phase 8).

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover Argon2id round-trip, login online, login offline fallback, refresh rotation, lock/unlock.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`: Login screen renders in both ar and en; idle watcher locks after one minute (test override).
5. `cd sync-server && pnpm test`: `/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password` happy paths and error cases.
6. Sync round-trip: create a user offline; reconnect; assert the user exists on the server.
7. Conflict scenario: edit the same `settings.dye_cost_iqd` on two devices; assert one push returns 409 with a `ConflictParked` row created server-side.
8. Offline auth: login online; disconnect; relaunch app; offline login succeeds against stronghold cache.
9. Token rotation: call `/auth/refresh`; assert the old refresh token is revoked (second use returns 401).
10. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 16 gaps incorporated below._

### 7.1 `thermal_width` seeded setting
- **Gap:** HIGH | Missing Setting Key | PRD §6.1.11, phase-05 §4
- Phase-05 receipt renderer reads `settings.thermal_width` (32 or 48 chars). The key is neither in the PRD §6.1.11 required-keys list nor in the phase-02 seed.
- **Resolution:** Add to §1 settings seed (and also seed `thermal_printer_name`, referenced by phase-05 §7.23):
  ```sql
  INSERT INTO settings (id, entity_id, key, value, value_type, ...)
  VALUES ('<uuid>', :tenant, 'thermal_width', '32', 'int', ...);
  INSERT INTO settings (id, entity_id, key, value, value_type, ...)
  VALUES ('<uuid>', :tenant, 'thermal_printer_name', '', 'text', ...);
  ```
  `thermal_width` allowed values: `32` (58mm) or `48` (80mm); validated by `SettingsService::update` against this enum. `thermal_printer_name` is free text validated against `settings::list_printers` membership on save (empty string = "use OS default"). Both are required keys (extend §7.2 delete-protection list). Note: the `value_type` literal MUST be `'int'`, not `'integer'`, to satisfy the §1 CHECK constraint `value_type IN ('int','decimal','text','bool')`.

### 7.2 Required-key delete protection
- **Gap:** HIGH | Missing Validation | PRD §6.1.11 invariant 3
- §4 `SettingsService` does not enforce PRD invariant 3 (required keys cannot be soft-deleted). The current spec has no `delete` path at all.
- **Resolution:** Add `SettingsService::soft_delete` that rejects when `key IN ('dye_cost_iqd','report_cost_iqd','internal_doctor_pct','idle_lock_minutes','arabic_numerals','clinic_display_name_ar','clinic_display_name_en','currency_symbol','thermal_width','thermal_printer_name')`. Error variant: `SettingsError::RequiredKeyImmutable`. No IPC command is exposed for `settings::delete`; service is internal-only.

### 7.3 `SettingsService::update` superadmin gate
- **Gap:** HIGH | Missing Precondition | PRD §8.6
- PRD §7.4 implies settings edits are superadmin-only; §4 `SettingsService::update` lists value-type validation and conflict policy but no role gate.
- **Resolution:** Add step 0 to `SettingsService::update`:
  ```
  if user_context.role != Role::Superadmin {
      return Err(SettingsError::Forbidden);
  }
  ```
  Re-stated in the IPC handler before delegating to the service.

### 7.4 "Settings changed - recompute?" draft banner
- **Gap:** HIGH | Missing Logic | PRD §8.6
- PRD §8.6 requires active draft visits to show a "settings changed - recompute?" banner. Phase-02 SettingsService emits no event for the frontend to observe.
- **Resolution:** Add to §4 SettingsService::update step 5: emit Tauri event `settings:changed` with `{ key, old_value, new_value, changed_at }`. The frontend's `<NewVisitForm>` (phase-05) listens and renders `<SettingsChangedBanner>` when an active draft exists. Cross-reference added to phase-05 §7.

### 7.5 User soft-delete atomicity and token revocation
- **Gap:** HIGH | Missing Business Rule | PRD §6.1.1 inv 4, §7.4
- §4 `UserService` does not specify `soft_delete`; PRD requires `deleted_at` AND `is_active = false` set atomically. Additionally, PRD §7.4 says reset-password forces sign-out across devices, but soft-delete does not enumerate refresh-token revocation.
- **Resolution:** Add `UserService::soft_delete(user_id, by_user_id)` to §4:
  1. Open tx.
  2. Update `users SET deleted_at = now, is_active = 0, version = version + 1, dirty = 1`.
  3. `with_audit` writes `soft_delete` action.
  4. Enqueue outbox.
  5. Commit.
  Server-side sync push acceptance additionally invokes `RefreshTokenRepo::revoke_all_for_user(user_id)` (delete all rows). Document this in §3.Server `AuthService` and add a server-side hook in `SyncPushService` for the `users` entity.

### 7.6 `UserService::update` and email-lowercase on update path
- **Gap:** MEDIUM | Missing Service Method | PRD §6.1.1
- §4 specifies `UserService::create` and `UserService::reset_password` but no `UserService::update`. PRD requires email-lowercase normalization on every write, not only create.
- **Resolution:** Add `UserService::update(user_id, fields)` to §4 with:
  1. Lowercase `email` if present.
  2. Re-validate role enum.
  3. Re-validate name non-empty after trim.
  4. `with_audit` writes `update` action with field-level delta.
  5. Bump `version`, set `dirty = 1`.
  Exposed as IPC `users::update(args) -> User`.

### 7.7 `/` root role-redirect page
- **Gap:** HIGH | Missing Page | PRD §3.1 navigation tree
- Frontend-summary names `src/pages/index/redirect.tsx` for phase-02; §3 Frontend Pages table omits the route entirely.
- **Resolution:** Add to §3 Frontend Pages table:
  ```
  | /              | src/pages/index/redirect.tsx | Role-based redirect: superadmin → /admin/users; accountant → /accounting; receptionist → /reception; no-role → /no-access |
  ```
  Implementation reads `useCurrentUser()`; renders a `<Navigate replace to={destination} />`.

### 7.8 `<RequireRole>` route gate component
- **Gap:** HIGH | Missing Component | frontend-summary §Conventions
- The frontend conventions reference `<RequireRole roles={...}>`; phase-02 introduces role-based routing without declaring the component file or contract.
- **Resolution:** Add to §3 Frontend components table:
  ```
  | <RequireRole>  | src/components/auth/require-role.tsx | Route-guard. Renders children when current user's role ∈ allowedRoles; otherwise <Navigate replace to="/no-access" />. |
  ```
  Signature: `({ roles: Role[], children: ReactNode })`. Used in the router config to wrap each module's outlet.

### 7.9 `lock::trigger` vs `auth::lock` naming
- **Gap:** MEDIUM | Mismatched Path | roadmap §Cumulative IPC
- Roadmap "Cumulative IPC Command and Route Targets" lists `lock::trigger`; phase-02 ships `auth::lock` / `auth::unlock`.
- **Resolution:** Canonical names are `auth::lock` and `auth::unlock`. Update roadmap inventory in a follow-up edit (this Section 7.x notes the canonical names).

### 7.10 JWT public-key fetch-and-pin at app start
- **Gap:** MEDIUM | Incomplete Coverage | PRD §5.5
- PRD §5.5 requires fetching and pinning the JWT public key in stronghold at app start. §4 AuthService only caches it inside the login response handler.
- **Resolution:** Add to §4 Tauri AuthService:
  - Method `bootstrap_jwt_key()` called from `lib.rs::setup` on app start.
  - GETs `/auth/jwks` (new server route, listed in §3 Server routes table); compares against stronghold's pinned `jwt/publicKey`; refuses startup if mismatched without an admin override.
  - Login response no longer overwrites the pin; only `bootstrap_jwt_key` does, behind a one-time `--reset-jwt-pin` flag.

### 7.11 First-launch ar-forcing detector
- **Gap:** MEDIUM | Missing Setup | PRD §10.6
- PRD §10.6 says first-launch ignores OS locale and forces `ar`. No phase declares the detector module.
- **Resolution:** Add to §3 Frontend Setup:
  ```ts
  // src/i18n/first-launch.ts
  export async function detectInitialLocale(): Promise<'ar' | 'en'> {
    const stored = await store.get<string>('locale');
    if (stored === 'ar' || stored === 'en') return stored;
    await store.set('locale', 'ar');
    await store.save();
    return 'ar';
  }
  ```
  Called from `src/i18n/index.ts` before i18next init.

### 7.12 `arabic_numerals` formatter helper
- **Gap:** MEDIUM | Missing Setup | PRD §10.6
- The setting is seeded but no helper wraps `Intl.NumberFormat('ar-IQ', { numberingSystem: 'arab' })` for consistent rendering across reports/receipts.
- **Resolution:** Add to §3 Frontend Setup:
  ```ts
  // src/lib/format/numerals.ts
  export function formatIqd(amount: number, opts: { locale: 'ar' | 'en', arabicDigits: boolean }): string;
  export function formatInt(n: number, opts: { locale: 'ar' | 'en', arabicDigits: boolean }): string;
  ```
  Both helpers read the live `settings.arabic_numerals` value via `useSettings()`. All money/integer rendering in feature code must go through these helpers.

### 7.13 Last-synced timestamp in user menu
- **Gap:** MEDIUM | Missing UI Element | PRD §10.8
- PRD §10.8 says the "Last synced" timestamp must be visible during outage. §3 Frontend declares `<UserMenu>` without this field.
- **Resolution:** Extend `<UserMenu>` to render `useSyncStatus().lastPulledAt | lastPushedAt`, formatted as relative ("3 min ago") with absolute on hover. Show a red dot when `last_pushed_at` is older than 5 minutes and outbox is non-empty.

### 7.14 UserContext rotation on `auth::logout` vs `auth::lock`
- **Gap:** MEDIUM | Missing Setup | PRD §9.2
- PRD §9.2 says session swap rotates `UserContext` in memory. Phase-02 distinguishes lock (preserves context) from logout (clears context) but the rotation flow is not explicit.
- **Resolution:** Add to §4 Tauri AuthService:
  - `auth::lock`: sets `AppState.user_context = Some(ctx_with_locked: true)`; in-memory cache preserved; UI shows lock screen.
  - `auth::unlock(password) -> Result<()>`: verifies password against `StrongholdCredsCache`; clears `locked` flag.
  - `auth::logout`: sets `AppState.user_context = None`; revokes refresh token; settings_cache cleared.
  - `auth::login(...)`: replaces `AppState.user_context` and `settings_cache` atomically (single write-lock).

### 7.15 `<UserDeleteConfirm>` and refresh-token revocation UX
- **Gap:** LOW | Missing Component | PRD §7.4
- The Admin Users page allows soft-deleting a user; PRD §7.4 says this also forces sign-out. No phase declares the confirmation flow.
- **Resolution:** Add to §3 Frontend components table:
  ```
  | <UserDeleteConfirm> | src/components/admin/user-delete-confirm.tsx | Modal warning "This will revoke all active sessions for <name>." Dispatches users::soft_delete(id). |
  ```
  Localized via `admin.users.delete_confirm.{title,body,confirm,cancel}` keys.

### 7.16 Email lowercase normalization on update path
- **Gap:** MEDIUM | Missing Validation | PRD §6.1.1
- Covered by §7.6 (UserService::update). No additional resolution required; this entry is retained as a cross-reference to PRD §6.1.1's "lowercase normalized at write" rule.

### 7.17 Server-only `pulledAt` column on `User` and `Setting`
- **Gap:** HIGH | Missing Field | PRD §6 (line 302)
- PRD line 302 mandates every server Prisma model includes a server-only `pulledAt` timestamp in addition to the standard sync columns. The §2 `User` and `Setting` Prisma blocks omit it.
- **Resolution:** Add `pulledAt DateTime? @map("pulled_at") @db.Timestamptz` to `model User` and `model Setting`. Set on each `/sync/pull` shipment (assigned in `SyncPullService` after a successful batch send). Used for "last delivered" diagnostics in phase-08 telemetry; not exposed to clients.

### 7.18 `audit_log.action` migration to extend the enum
- **Gap:** HIGH | Missing Schema Change | phase-01 §7.8
- §4 line "add `login` action enum value" is descriptive, but `audit_log.action` in phase-01 is a free-text TEXT column without CHECK. The 12-value closed union (per phase-01 §7.8 and phase-08 §1) is enforced only at the application layer.
- **Resolution:** Append to §1 `migrations/002_users_settings.sql` a comment block documenting the application-enforced enum: `-- audit_log.action closed union: create, update, soft_delete, lock, void, clock_in, clock_out, password_change, login, logout, conflict_resolve, vacuum. Enforcement is in src-tauri/src/audit/action.rs (AuditAction::from_str) and server src/audit/action-schema.ts (TypeBox literal union). Adding a new value requires updating both validators.` On the server, mirror the union as a TypeBox `Type.Union([Type.Literal('create'), ...])` exported from `sync-server/src/audit/action-schema.ts`; the schema is referenced by `/sync/push` body validators.

### 7.19 Server-side settings `manual` conflict detection
- **Gap:** HIGH | Missing Sync Rule | PRD §6.1.11
- §4 client `SettingsService::update` declares `manual` policy and emits `sync:conflict`, but the server side has no documented `SyncPushService::accept_settings` branch.
- **Resolution:** Add to §4 Server:
  ```
  SyncPushService::acceptSettings(row, opId):
    1. Load current Setting by (entityId, key).
    2. ProcessedOp.has(opId) → return cached envelope.
    3. If existing AND existing.version >= row.version AND existing.updatedAt != row.updatedAt:
         - INSERT ConflictParked { opId, entity:'settings', payload: { local: row, server: existing } }.
         - Return 409 { code: 'CONFLICT_PARKED', opId } envelope (mirrors ErrorResponseSchema).
    4. Else UPSERT by (entityId, key); bump version; cache ProcessedOp; return 200.
  ```
  Identical structural logic for `visits` (already declared in phase-05 §4) — the helper `parkConflict(opId, entity, payload)` is shared via phase-01 §3 `ConflictParkedRepository`.

### 7.20 Tauri `users::list` response strips `password_hash`
- **Gap:** HIGH | Field Type Mismatch | PRD §11
- §3 Tauri command `users::list` returns `User[]`. The `User` domain struct includes `password_hash`. Server `UserResponse.toResponse()` strips it; Tauri does not.
- **Resolution:** Add to §3 Tauri a `UserResponse` Rust struct:
  ```rust
  #[derive(serde::Serialize)]
  pub struct UserResponse {
      id: Uuid, email: String, name: String, role: UserRole,
      is_active: bool, last_login_at: Option<DateTime<Utc>>,
      created_at: DateTime<Utc>, updated_at: DateTime<Utc>,
      entity_id: Uuid, version: i64, dirty: bool,
  }
  impl From<User> for UserResponse { /* drops password_hash */ }
  ```
  Change `users::list`, `users::get`, `users::create`, `users::update`, `users::reset_password` return types from `User`/`Vec<User>` to `UserResponse`/`Vec<UserResponse>`. The Rust `User` aggregate keeps `password_hash` for internal sync-engine use only; it never leaves the Rust process boundary.

### 7.21 First-launch superadmin bootstrap UX
- **Gap:** CRITICAL | Missing Setup | PRD §11.1
- The post-Phase-1 DB has zero users. `/login` cannot succeed and the app is unreachable on a fresh install. No phase documents a bootstrap flow.
- **Resolution:**
  - Frontend: add route `/setup/first-run` rendered when `users::list({ includeInactive: true })` returns empty; redirect to it from `/login` automatically. Form fields: superadmin email, name, password (min 8). Disabled on subsequent visits.
  - IPC: `users::create_first_admin(input) -> UserResponse` bypasses the superadmin gate but errors with `FirstAdminExists` if any user row exists (idempotent). Audit action: `bootstrap_admin` (added to the audit action enum in §7.18, requiring the local CHECK to include it; an entry under the §7.18 enum union).
  - Server: a Prisma `seed.ts` creates one bootstrap superadmin from env vars `BOOTSTRAP_SUPERADMIN_EMAIL`, `BOOTSTRAP_SUPERADMIN_PASSWORD`, `BOOTSTRAP_TENANT_ID`; hashed with Argon2id. Idempotent (no-op if any superadmin row exists). `Dockerfile.dev` runs `pnpm prisma db seed` after `migrate-deploy`. Operator must rotate the bootstrap password on first login.
  - On success, `users::create_first_admin` auto-logs the user in (online if server reachable, else stronghold-caches the hash).

### 7.22 `<SettingsForm>` value-type widget bindings
- **Gap:** MEDIUM | Missing UI Element | PRD §6.1.11
- §3 Frontend lists `/admin/settings` with "Form for the v1 required settings keys" but does not specify per-key widget binding.
- **Resolution:** Add to §4 Frontend `<SettingsForm>`:
  ```
  dye_cost_iqd, report_cost_iqd, idle_lock_minutes: <Input type="number" min={0}> with 'د.ع' or 'min' suffix.
  thermal_width: <Select options={[32, 48]}>.
  thermal_printer_name: <Combobox> options=settings::list_printers() result.
  internal_doctor_pct: <Input type="number" min={0} max={100}> with '%' suffix.
  arabic_numerals: <Switch>.
  clinic_display_name_ar, clinic_display_name_en, currency_symbol: <Input type="text">.
  ```
  Each control binds to `SettingValue` variant per `value_type`. Read-only when current role is not superadmin (per §7.3). Save submits as a single transaction so partial writes are impossible.

### 7.23 LWW tiebreak re-stated for `users` and `settings`
- **Gap:** HIGH | Missing Tiebreak | research.md (phase-03 §7.17 parallel)
- §4 Sync Semantics table lists `users | last-write-wins` and `settings | manual` but does not re-state the global `origin_device_id` lex tiebreak (phase-03 §7.17 added it for catalog entities; phase-02 must do the same).
- **Resolution:** Append to §4 Sync Semantics row notes:
  - `users`: "LWW tiebreak: when `updated_at` matches to the millisecond, the row with the lexicographically smaller `origin_device_id` wins. Documented globally in phase-01 §4 SyncEngine."
  - `settings`: "Manual-policy entities never apply LWW. Conflicts are parked unconditionally when versions diverge (see §7.19 server flow); no tiebreak applies."

### 7.24 `users` push asymmetric `password_hash` rule
- **Gap:** HIGH | Asymmetric Validation | PRD §11
- §4 sync semantics says "`password_hash` server-canonical; pushed but server only accepts if writing client is superadmin" while the roadmap sync-contracts table says "full row except password_hash". Contradictory; client implementation will diverge.
- **Resolution:** Disambiguate in §4 Sync Semantics row for `users`:
  - Push payload INCLUDES `password_hash` ONLY when the origin op was `users::create` or `users::reset_password` (both superadmin-gated). Server inspects JWT `role` claim and rejects 403 if mismatched.
  - Push payload for `users::update` (profile edits) EXCLUDES `password_hash` (forbidden via TypeBox `Type.Never`).
  - Pull payload from server EXCLUDES `password_hash` for all consumers. Local row retains its existing hash.
  - The Argon2id stronghold offline-login cache is updated only when the actor enters their own password (login or `change_password`), never on pull.
  - Add server TypeBox variants: `UserCreatePushSchema` (password_hash required), `UserUpdatePushSchema` (password_hash forbidden).
  - Update the roadmap.md sync-contracts table notes for `users` row to reflect the conditional inclusion.

### 7.25 Token-refresh failure handling
- **Gap:** HIGH | Missing Logic | PRD §5.5
- PRD §5.5 sets 15-min access tokens, but no phase specifies what the Rust `SyncEngine` (or any IPC consumer) does when a server call returns 401.
- **Resolution:** Add to §4 Tauri: a `TokenManager` task. On any 401 from a sync-server call: call `POST /auth/refresh` once using the stronghold-cached refresh token. On 200, retry the original request once. On refresh failure (401/expired/revoked) emit `auth::session_expired` event; `<AppShell>` listens and routes to `/login` while preserving the outbox. Outbox is NEVER cleared on auth failure (the user can re-login and resume pushes). Idempotency keys persist across re-login.

### 7.26 Lock-on-suspend / lock-on-resume OS hooks
- **Gap:** MEDIUM | Missing Setup | PRD §5.5
- PRD §5.5 only specifies idle-time lock. OS-level suspend is not handled; a stolen laptop could be woken without re-auth if the idle timer hadn't fired yet.
- **Resolution:** Add to §4 Tauri: register Tauri window listeners for `tauri://blur` and platform-specific suspend events (via `tauri-plugin-os` where available). On suspend or 60+ seconds of focus loss, dispatch `auth::lock` immediately. Document as defense-in-depth beyond the idle timer. Add `tauri-plugin-os` to §5 plugin registrations.

### 7.27 Stronghold cred-cache rotation on synced password change
- **Gap:** MEDIUM | Missing Logic | PRD §5.5
- PRD §5.5 says the offline cache invalidates on successful online password change. `auth::change_password` is online-only, but no phase specifies what happens when an admin resets a user's password from another device (sync delivers the new `password_hash`).
- **Resolution:** Add to §4 Tauri the `SyncEngine` apply-path hook `AuthService::on_user_synced(user)`: when `users.password_hash` changes for the locally cached user, delete `creds/<email>` from stronghold immediately. Next offline login fails for that user (forces online re-auth). Audit action: `password_change` already in the enum.

### 7.28 IPC role-gate symmetry for users / settings
- **Gap:** HIGH | Missing Role Gate | PRD §11.1
- §3 declares `users::*` and `settings::*` commands but only `users::create`, `users::reset_password`, `settings::update` are explicitly gated in §4 prose.
- **Resolution:** Add to §4 Tauri a single block listing the gated commands and assert each opens with `require_role(&app_state, &[Role::Superadmin])?` returning `AppError::Auth(AuthError::Forbidden)`. Gated: `users::list` (no, accessible by all logged-in users — list only excludes inactive unless superadmin), `users::get` (self or superadmin), `users::create`, `users::update`, `users::soft_delete`, `users::reset_password`; all of `settings::update`, `settings::set_locale`. Server `/sync/push` mirrors: rejects pushes mutating `users` or `settings` rows when the JWT `role` claim is not `superadmin` (per the same role-gate helper used in phase-01 §3 TenantPlugin).

### 7.29 Prisma `User` <-> `OperatorShift` back-relations
- **Gap:** CRITICAL | Missing Relation | Pass-3 GAP-B-1; phase-04 §2
- Phase-04 §2 `OperatorShift` declares `checkInByUser User @relation("ShiftCheckIn", ...)` and `checkOutByUser User? @relation("ShiftCheckOut", ...)`. The `User` model in §2 has no inverse fields. `prisma generate` will fail with "missing opposite relation field".
- **Resolution:** Append to §2 `model User` relations block:
  ```prisma
  shiftsCheckedIn   OperatorShift[] @relation("ShiftCheckIn")
  shiftsCheckedOut  OperatorShift[] @relation("ShiftCheckOut")
  ```
  Both are inverse-only (no FK on this side). No migration impact (pure Prisma annotation).

### 7.30 IQD currency formatter helper
- **Gap:** LOW | Missing Setup | PRD §10.5 lines 2113-2115; Pass-3 GAP-D-3
- PRD §10.5 mandates IQD integers + `settings.currency_symbol` suffix; PRD §10.6 mandates locale-aware thousands separators. `currency_symbol` is seeded by §7.1; no phase declares the shared formatter helper.
- **Resolution:** Add to §3 Frontend setup: `src/lib/format/money.ts` exporting `formatIQD(value: number, locale: 'ar' | 'en', symbol: string): string` -- runs `Intl.NumberFormat(locale).format(value)` then appends ` ${symbol}`. When `useSettings().arabic_numerals` is true and `locale === 'ar'`, transliterates ASCII digits via the helper from §7.12. All money cells in lists, forms, dashboards, and receipts call this helper. §6 verification grep: `grep -rE "(IQD|د\\.ع)" src --include="*.tsx" --exclude src/lib/format/money.ts` returns zero matches.

### 7.31 `<ResetPasswordModal>` component
- **Gap:** MEDIUM | Missing UI Element | PRD §7.4 actions row + §7.4.1; Pass-3 GAP-E-15
- PRD §7.4 lists "Reset password" as a row action on Users; §7.4.1 enumerates the password reset field. §3 declared `/admin/users/:id` page with "User edit + password reset button" as one line and `<UserDeleteConfirm>` component but no `<ResetPasswordModal>`.
- **Resolution:** Add to §3 Frontend components: `<ResetPasswordModal>` (`src/components/admin/reset-password-modal.tsx`). Inputs: new password (Zod min 8), confirm input. Submit dispatches `users::reset_password`. On success show toast "Password reset; user must sign in again on every device." No password is rendered back. Triggered from `<UsersListRowActions>` (also new -- icon-button row showing edit, reset-password, soft-delete) and from `/admin/users/:id` action bar. Both row-action and detail-bar guarded with `<RequireRole roles={['superadmin']}>`.

### 7.32 Settings page role-gate clarification
- **Gap:** LOW | Inconsistency | §7.22; Pass-3 GAP-E-16
- §7.22 declared per-key widgets are "Read-only when current role is not superadmin". With phase-03 §7.36 adding `<RequireRole>` around `/admin/*` (superadmin-only), the read-only branch is unreachable.
- **Resolution:** Remove the read-only-when-not-superadmin clause from §7.22 -- `<SettingsForm>` is reachable only by superadmin. If accountants need to inspect select settings (e.g., `currency_symbol`), surface them via the user-menu Diagnostics modal from phase-08 §7.17 instead. Cross-reference phase-03 §7.36 for the route-level guard.

### 7.33 `settings` sync-apply guard for required-key delete
- **Gap:** LOW | Missing Validation | §7.2; Pass-3 GAP-C-7
- §7.2 added `SettingsService::soft_delete` rejecting required keys, but the protection lives only in the local IPC path. A pull from another device delivering a row with `deleted_at != null` for a required key would be applied. §7.19 server `acceptSettings` only handles version-divergence conflicts.
- **Resolution:** Append to §7.19 server flow and add a sync-apply hook on the client:
  - Server `SettingsService::acceptPush(row, opId)` step list addition: "If `row.deletedAt != null` AND `row.key IN ('dye_cost_iqd','report_cost_iqd','internal_doctor_pct','idle_lock_minutes','arabic_numerals','clinic_display_name_ar','clinic_display_name_en','currency_symbol','thermal_width','thermal_printer_name')` -> return `422 SETTINGS_REQUIRED_KEY_IMMUTABLE`."
  - Client `SyncEngine::apply_pull` for `settings` runs the identical guard before writing -- drops the row, logs WARN, surfaces a one-time toast `errors:settings.required_key`.
  - Add §6 verification: device A soft-deletes `dye_cost_iqd` via raw SQL bypassing the IPC, pushes; assert server rejects 422; assert device B never reflects deletion.
