# Phase 2: Sync Server Foundation

**Goal:** Stand up the Fastify + Prisma + Postgres sync server so Phase 1's client can authenticate, push outbox batches, pull peer changes, and surface manual conflicts. Land the auth surface end-to-end (login + refresh + logout + change-password) plus the sync surface skeleton (push/pull/resolve).

**Surfaces:** Sync Server
**Dependencies:** Phase 1 (the client must exist to test the round-trip).
**Complexity:** XL
**PRD references:** §5.2 (Sync Server), §5.5 (Auth), §6.1.1 (users), §6.1.15 (audit_log), §9.1 (Tenant Scoping), §10.8 (Offline UX).
**Decisions consumed:** D-004 (single-tenant w/ entity_id forward-compat), D-014 (UUID v7 client-generated), D-015 (server retains audit indefinitely), D-016 (sync conflict policies), D-018 (sequential delivery), D-024 (PII redaction).

---

## Section 1: Local Schema Changes (Tauri SQLite)

**No new tables.** Phase 1 already ships `users`, `audit_log`, `outbox`, `sync_state`, `_migrations`. This phase wires the server endpoints those tables push to and pull from.

The `SyncEngine` from Phase 1 starts succeeding instead of fail-softing once Phase 2 ships.

---

## Section 2: Server Schema Changes (Prisma / Postgres)

Schema lives at `sync-server/prisma/schema.prisma`. Postgres runs in Docker. Per `docker.md`, `prisma db push --accept-data-loss` runs on container restart.

### `User`

```prisma
model User {
  id              String    @id                                   // UUID v7 from client; server validates format
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

  refreshTokens RefreshToken[]
  sessions      Session[]
  auditLogs     AuditLog[]

  @@unique([entityId, email], name: "user_email_unique")
  @@index([entityId])
  @@map("users")
}

enum UserRole {
  superadmin
  receptionist
  accountant
}
```

### `AuditLog`

Mirrors PRD §6.1.15.

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

  actor User @relation(fields: [actorUserId], references: [id])

  @@index([entity, entityId, at])
  @@index([actorUserId, at])
  @@index([at])
  @@map("audit_log")
}
```

### `RefreshToken` (server-only — NOT in TENANT_MODELS, NOT synced)

```prisma
model RefreshToken {
  id          String    @id @default(uuid())
  userId      String    @map("user_id")
  tokenHash   String    @unique @map("token_hash")           // SHA-256; raw token never persisted
  issuedAt    DateTime  @default(now()) @map("issued_at") @db.Timestamptz
  expiresAt   DateTime  @map("expires_at") @db.Timestamptz
  revokedAt   DateTime? @map("revoked_at") @db.Timestamptz
  rotatedFrom String?   @map("rotated_from")
  deviceId    String?   @map("device_id")
  ip          String?

  user User @relation(fields: [userId], references: [id])

  @@index([userId, expiresAt])
  @@map("refresh_tokens")
}
```

### `Session` (server-only — telemetry, not auth)

```prisma
model Session {
  id          String   @id @default(uuid())
  userId      String   @map("user_id")
  deviceId    String   @map("device_id")
  startedAt   DateTime @default(now()) @map("started_at") @db.Timestamptz
  lastSeenAt  DateTime @map("last_seen_at") @db.Timestamptz
  endedAt     DateTime? @map("ended_at") @db.Timestamptz

  user User @relation(fields: [userId], references: [id])

  @@index([userId, startedAt])
  @@map("sessions")
}
```

### Postgres extensions / custom SQL
File: `sync-server/prisma/init-custom-sql.sql`.

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX IF NOT EXISTS users_email_trgm ON users USING gin (email gin_trgm_ops);
```

(Used in Phase 9 for server-side audit-log full-text. Land the extension here so future phases don't re-run schema sync.)

### What this phase does NOT touch (server schema)

- No `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing`, `Operator`, `OperatorSpecialty`, `Setting`, `Patient` — Phase 3.
- No `OperatorShift` — Phase 4.
- No `Visit` — Phase 5.
- No inventory models — Phase 6.

---

## Section 3: DDD Implementation

### Frontend (React)

#### New pages / routes
None. Phase 2 doesn't add any user-facing pages.

#### Updates
`src/lib/api-client.ts` — axios instance:
- baseURL from env (`VITE_SYNC_SERVER_URL`).
- `Authorization: Bearer <accessToken>` injected via request interceptor (token from `useAuthStore`).
- Response interceptor: on 401, calls `auth_refresh` IPC; retries once; if still 401, dispatches `useAuthStore.logout()` and redirects `/login`.

The login form from Phase 1 starts hitting the real server through this client.

### Tauri/Rust

#### `SyncClient`
File: `src-tauri/src/sync/client.rs`. `reqwest::Client` with:
- Base URL pulled from settings (default `http://localhost:3000` in dev, configurable in prod).
- Connect timeout 5s, request timeout 15s.
- `Authorization` header injected by reading `AppState.user_context` access token.
- 401 retry-once policy mirrored on the Rust side (the client interceptor handles the WebView path; `SyncClient` handles the engine path).

Methods:
- `push_batch(ops: &[OutboxOp]) -> Result<PushResponse, AppError>` — POST `/sync/push` with MessagePack body.
- `pull(since: Option<&str>) -> Result<PullResponse, AppError>` — GET `/sync/pull?since=...`.
- `resolve_conflict(op_id: Uuid, resolution: ConflictResolution) -> Result<(), AppError>` — POST `/sync/conflicts/:op_id/resolve`.
- `auth_login`, `auth_refresh`, `auth_logout`, `auth_change_password` — POST against the auth surface.

#### Updates
`src-tauri/src/services/auth_service.rs` — fills in the network half:
- `online_login(email, password)` → POST `/auth/login` → store tokens in stronghold + write Argon2id-hashed password to stronghold for offline cache.
- `refresh()` → POST `/auth/refresh`; rotate.
- `change_password(old, new)` → POST `/auth/change-password`; on success, recache offline hash.

### Sync Server (Fastify)

#### Plugin file layout
```
sync-server/
  src/app/
    plugins/                                  // auto-loaded
      env.ts                                  // @fastify/env (DATABASE_URL, JWT_*, REDIS_URL, etc.)
      sensible.ts                             // @fastify/sensible
      cors.ts                                 // @fastify/cors
      helmet.ts                               // @fastify/helmet
      rate-limit.ts                           // @fastify/rate-limit
      compress.ts                             // @fastify/compress
      multipart.ts                            // @fastify/multipart (used in P10 for restore upload)
      jwt.ts                                  // @fastify/jwt (RS256, public/private keys via env)
      swagger.ts                              // @fastify/swagger + @fastify/swagger-ui at /documentation
      prisma.ts                               // PrismaClient as decorator
      tenant.ts                               // tenant extension; injects entityId from JWT
      errors.ts                               // central errorHandler -> { success: false, error: {...} }
    common/
      schemas/                                // shared TypeBox schemas (SyncColumns, ErrorEnvelope, etc.)
      errors.ts                               // domain error classes
      uuid-v7.ts                              // validation helper (rejects non-v7)
    domains/
      auth/
        domain/
          user.ts, refresh-token.ts, session.ts
        infrastructure/
          repositories/{user.repo.ts, refresh-token.repo.ts, session.repo.ts}
        presentation/
          routes/auth.routes.ts
          schemas/auth.schemas.ts
        services/
          auth.service.ts, jwt.service.ts, password.service.ts
      audit/
        domain/audit-event.ts
        infrastructure/repositories/audit.repo.ts
        presentation/routes/audit.routes.ts (P9 only)
        services/audit.service.ts
    sync/
      service/
        push.service.ts, pull.service.ts, conflict.service.ts, cursor.service.ts
      conflict/
        policy.ts (LWW / additive-only / manual dispatch table)
      routes/
        sync.routes.ts                        // /sync/push, /sync/pull, /sync/conflicts/:opId/resolve
        schemas/sync.schemas.ts
    health/
      routes/health.routes.ts                 // GET /healthz
    app.ts                                    // builds Fastify, registers autoload plugins, registers routes
    main.ts                                   // boots app
```

#### Routes

| Method | Path | Description |
|-|-|-|
| `POST` | `/auth/login` | `{ email, password, deviceId }` → `{ accessToken, refreshToken, user, role, publicKey }`. 401 on bad creds; 403 on inactive; 429 on rate-limit. |
| `POST` | `/auth/refresh` | `{ refreshToken }` → rotate; old token revoked; new pair issued. 401 on bad/revoked. |
| `POST` | `/auth/logout` | `{ refreshToken }` → revoke + delete. Best-effort; 200 even if missing (idempotent). |
| `POST` | `/auth/change-password` | `{ currentPassword, newPassword }` (auth required) → hash + store; revoke all other refresh tokens. |
| `POST` | `/sync/push` | MessagePack body: `{ ops: OutboxOp[] }`. Validates UUID v7. Dispatches per-entity to LWW / additive / manual handlers. Returns `{ accepted: opId[], conflicts: ConflictRow[] }`. 409 if any op conflicts AND policy is `manual`. |
| `GET` | `/sync/pull` | `?since=<cursor>` → `{ changes: Change[], nextCursor: string, hasMore: boolean }`. Cursor is server's `updatedAt` watermark scoped to tenant. Limit 200 rows per page. |
| `POST` | `/sync/conflicts/:opId/resolve` | `{ choice: 'local' | 'server' | 'merge', mergedPayload?: Json }` → applies + clears conflict. Audit row written. |
| `GET` | `/healthz` | Liveness; no auth. Returns `{ status: 'ok', uptime, dbReachable }`. |

All routes carry full TypeBox schemas (description, summary, tags, body, querystring, params, response 200/400/401/403/404/409/422/5xx, security). Per `sync-server.md`, response shape is `{ success: true, data: ... }` or `{ success: false, error: { code, message, details? } }`.

#### TypeBox schemas (`sync-server/src/app/common/schemas/`)

- `SyncColumnsSchema` — the 9-column suffix every syncable row carries.
- `ErrorEnvelopeSchema` — `{ success: false, error: { code: string, message: string, details?: any } }`.
- `OkEnvelopeSchema<T>` — `{ success: true, data: T }`.
- `ListEnvelopeSchema<T>` — `{ success: true, data: { items: T[], total: number, cursor?: string } }`.
- `OutboxOpSchema` — `{ opId, entity, entityId, op: 'upsert' | 'delete', payload: Type.Unknown(), version, updatedAt, originDeviceId }`.
- `ConflictRowSchema` — `{ opId, entity, entityId, policy, local: ..., server: ... }`.

Nullable convention: `Type.Union([T, Type.Null()])` (NOT `Type.Optional`).

---

## Section 4: Business Logic

### `AuthService` (server)
File: `sync-server/src/app/domains/auth/services/auth.service.ts`.

Login step sequence:
1. Validate body via TypeBox; reject malformed UUIDs and emails.
2. Lookup user by email; reject if `deletedAt != null` or `isActive == false` (mask as `INVALID_CREDENTIALS` to avoid enumeration).
3. Argon2id-verify password; on miss throw `INVALID_CREDENTIALS`.
4. Issue access JWT (RS256, 15m, claims: `sub, email, role, isActive, sessionId, entityId, iat, exp, deviceId`).
5. Issue refresh token (32 random bytes → base64url; SHA-256 hash stored; raw returned to client; 30d expiry); insert `RefreshToken` row.
6. Insert/update `Session` row (one per device per user; updates `lastSeenAt`).
7. Update `User.lastLoginAt`; bump `version` + `updatedAt`.
8. Return `{ accessToken, refreshToken, user: toResponse(user), publicKey }`.

Refresh step sequence:
1. SHA-256 incoming refresh; lookup `RefreshToken` by hash.
2. If missing / revoked / expired → 401.
3. Atomic transaction: revoke old, insert new, return new pair. (Rotation prevents replay.)
4. Bump `Session.lastSeenAt`.

Logout: best-effort delete + revoke; 200 always.

Change password: requires valid access token; verify current; argon2-hash new; replace `passwordHash`; revoke all refresh tokens for this user EXCEPT the current session (so the originating device stays logged in).

### `SyncService` — Push
File: `sync-server/src/app/sync/service/push.service.ts`.

Push step sequence (per op):
1. Validate UUID v7 format on `opId`, `entityId`.
2. Resolve `entity` to a Prisma model + sync policy from a registry (`SYNC_POLICY_REGISTRY`).
3. Read existing row by `(entityId, tenantId)`.
4. If row absent → upsert; respond with `accepted`.
5. If row present:
   - Compare `version`. If incoming `version > server.version` → upsert; accepted.
   - If `version == server.version`:
     - LWW: compare `updatedAt`; later one wins (tiebreak by `originDeviceId` lexicographic).
     - Additive-only: append regardless (only valid for `audit_log`, `operator_shifts`, `inventory_adjustments` — these have no "update" semantics).
     - Manual: check field-level diff; if any conflicting field → return as `conflict`.
   - If `version < server.version` → return as `conflict` regardless of policy (stale write).
6. Apply or queue; emit audit row for every applied write.
7. Aggregate response `{ accepted, conflicts }`.

Each accepted op increments server's per-entity sync watermark for that tenant.

### `SyncService` — Pull
File: `sync-server/src/app/sync/service/pull.service.ts`.

Pull step sequence:
1. Validate `since` cursor; if absent → start from epoch.
2. For each entity in TENANT_MODELS:
   - Query rows where `updatedAt > since AND entityId = tenantId`, ordered by `updatedAt`, limit 200.
3. Merge + sort; emit `Change[]` shape `{ entity, op: 'upsert' | 'delete', payload }`.
4. Compute `nextCursor` = max `updatedAt` of returned rows; `hasMore` = batch size === 200.

### `ConflictService`
File: `sync-server/src/app/sync/service/conflict.service.ts`. Stores manual conflicts in a server-only `SyncConflict` table (added in Phase 9 alongside resolver UI; for Phase 2 the conflict path returns inline in the push response and the client logs them). Resolve endpoint is a stub that records the choice and applies the resolution; full UI lands in Phase 9.

### `TenantPlugin`
File: `sync-server/src/app/plugins/tenant.ts`. Fastify plugin that:
- On every authenticated request, reads `request.user.entityId` (from JWT).
- Decorates `request` with `request.tenantId`.
- Wraps `prisma.$extends({ query: { ... } })` to inject `where: { entityId }` into all queries on `TENANT_MODELS`.
- TENANT_MODELS = `['User', 'AuditLog']` for this phase.

Per `sync-server.md`: child/junction models inherit isolation via FK and MUST NOT be added to TENANT_MODELS (the extension would crash). Phases 3-6 add only top-level models with their own `entityId`.

### `JwtService`
File: `sync-server/src/app/domains/auth/services/jwt.service.ts`. Wraps `@fastify/jwt`. Public + private keys read at boot from `JWT_PUBLIC_KEY_PATH` and `JWT_PRIVATE_KEY_PATH` env vars. Sign + verify helpers; `getPublicKeyPem()` for the login response.

### `PasswordService`
File: `sync-server/src/app/domains/auth/services/password.service.ts`. `argon2id-rs` wrapper. Single source of truth for the work factors (memory cost 64 MiB, time cost 3, parallelism 4). Both server and client (Rust) use the **same** parameters so the cached hash matches the server's online hash byte-for-byte.

### Sync semantics summary

| Entity | Policy | Idempotency key | Notes |
|-|-|-|-|
| `users` | LWW | `op_id` | rare edits; admin-driven |
| `audit_log` | additive-only | `op_id` | append; tombstone unused |

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions on the server
**TENANT_MODELS = `['User', 'AuditLog']`** at end of Phase 2.

### Audit triggers
None.

### Local SQLite indexes added
None this phase.

### Tauri capabilities
No additions; `reqwest` runs in Rust and doesn't need WebView capabilities.

### New Tauri plugin registrations
None.

### New Fastify plugins
All listed in Section 3 plugin layout. No queues yet (BullMQ deferred — first user is Phase 9 vacuum, which can run as a `setInterval` for v1 if BullMQ feels heavy; revisit at end of P9).

### Docker compose
File: `docker-compose.yaml`.

```yaml
services:
  sync-db:
    image: postgres:16
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: sync_db
    ports: ["5432:5432"]
    volumes: ["sync-db-data:/var/lib/postgresql/data"]
  sync-server:
    build:
      context: ./sync-server
      dockerfile: Dockerfile.dev
    environment:
      DATABASE_URL: postgresql://postgres:postgres@sync-db:5432/sync_db
      JWT_PUBLIC_KEY_PATH: /run/keys/jwt-public.pem
      JWT_PRIVATE_KEY_PATH: /run/keys/jwt-private.pem
      PORT: 3000
    ports: ["3000:3000"]
    depends_on: ["sync-db"]
    volumes:
      - ./sync-server/src:/app/src
      - ./sync-server/prisma:/app/prisma
      - ./keys:/run/keys:ro
volumes:
  sync-db-data:
```

`Dockerfile.dev` runs `npx prisma db push --accept-data-loss && psql $DATABASE_URL -f prisma/init-custom-sql.sql && pnpm dev` per `docker.md`.

`./keys/jwt-{public,private}.pem` generated at first run by `tools/gen-jwt-keys.sh` (committed alongside the script, NOT the keys themselves).

### Swagger
Available at `http://localhost:3000/documentation` once the server is up. Every route documented with description, tags, summary, full schemas, all status codes, security.

---

## Section 6: Verification

1. **Server lint / typecheck / test.**
   ```bash
   cd sync-server && pnpm lint && pnpm typecheck && pnpm test
   ```
2. **Compose stack up.**
   ```bash
   docker compose up -d sync-db sync-server
   docker logs sync-server --tail 200 -f
   ```
   Confirms Prisma schema sync, init SQL applied, server listening on `:3000`.
3. **Healthz.**
   ```bash
   curl -s http://localhost:3000/healthz | jq
   ```
   Returns `{ status: 'ok', uptime, dbReachable: true }`.
4. **Swagger reachable.** Open `http://localhost:3000/documentation`; confirm every route from Section 3's table is listed with full schemas.
5. **Auth round-trip via MCP curl** (per `dev-workflow.md`):
   - Seed a user via a one-off `pnpm prisma:seed:dev` (script created in this phase, idempotent, only writes if no users exist; uses Argon2id with the same params as the client).
   - `POST /auth/login` with seeded creds → 200, returns tokens.
   - Use access token on `GET /healthz` (no-auth route, but verifies header parsing).
   - `POST /auth/refresh` → rotation; old refresh now 401.
   - `POST /auth/logout` → 200; refresh now 401.
6. **Push round-trip from the Phase-1 client.** With server up, `pnpm tauri dev` and:
   - Log in successfully through the Phase-1 login form.
   - Trigger a one-off dev IPC (`auth_change_password` IPC works as a benign mutation) — observe an outbox row appear, push, drain. `audit_log` row lands on the server (visible via `psql -c 'SELECT * FROM audit_log'`).
7. **Pull round-trip.** Insert an `audit_log` row directly on the server (`psql`); confirm the client pulls it within 10s; confirm sync pill cycles `pulling` → `idle`.
8. **Conflict scenario (manual).** Manually corrupt a `users.version` mismatch on two devices in dev; verify the server returns a `conflict` row in the push response and the engine logs it. (Resolver UI lands in P9.)
9. **Tenant scoping.** Insert a user with a different `entityId`; verify the authenticated tenant's pull omits that row.
10. **Pre-push composite.**
    ```bash
    pnpm lint && pnpm build &&
    (cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test) &&
    (cd sync-server && pnpm lint && pnpm test)
    ```

### What this phase explicitly does NOT verify

- Domain entities beyond `User` and `AuditLog` (Phase 3+).
- Operator shifts (Phase 4), visits (Phase 5), inventory (Phase 6).
- Conflict resolver UI (Phase 9).
- `/audit/query`, `/reports/visits`, `/reports/daily-close/:date` server routes (Phases 9, 7, 7).
- Backup endpoint (Phase 10).
- Rate-limit tuning (defaults in P2; revisit if P10 ops review flags abuse vectors).

### Summary update
After Phase 2 verifies, bump `status.md` row 2 to `Completed` (record `RefreshToken` and `Session` as the 2 server-only models; TENANT_MODELS = 2). `frontend-summary.md` Section 7 (RTL) gets no new entries since no UI was added; record explicitly "no shell additions in P2" in the Change Log.

---

## Section 7: PRD Gap Additions

### 7.1 Rate-limit defaults — LOW
**Gap:** `@fastify/rate-limit` is registered in Phase 2 plugins, but no specific limits are pinned. PRD §1.4 implicitly allows reasonable defaults; without numbers, ops can't audit.
**Category:** Missing Setup.
**Remediation:** Pin defaults at `sync-server/src/app/plugins/rate-limit.ts`:
- `/auth/login`: 10 req / 15 min per IP. 429 with `Retry-After`.
- `/auth/refresh`: 30 req / hour per IP.
- All other auth routes: 60 req / hour per user (skipped for unauth'd routes).
- `/sync/push`, `/sync/pull`: 600 req / hour per user (matches the 2s pusher and 10s puller cadence).
- All other authenticated routes: 1000 req / hour per user.

### 7.2 Offline cache refresh on password change — LOW
**Gap:** Phase 2 §4 says `change-password` revokes other refresh tokens but doesn't trigger an offline-cache refresh on the originating device. PRD §5.5 mandates "cache invalidates on any successful online password change".
**Category:** Missing Logic.
**Remediation:** Update Phase 2 `AuthService.change_password` to:
- Return new tokens AND a flag `cacheRefreshRequired: true` in the response.
- Tauri's `auth_change_password` IPC, on receipt, re-Argon2id-hashes the new password using the same params and rewrites the stronghold cache atomically with the new tokens.
- Add unit test: verify cached hash mismatches the old password after change.

### 7.3 `/auth/mfa` route stub — LOW (companion to P1 Gap 7.2)
**Gap:** auth.md rule references the MFA endpoint. Phase 2 doesn't ship it; clients calling it would get an unintentional 404.
**Category:** Missing Endpoint (Decision-driven).
**Remediation:** Register `POST /auth/mfa` returning 501 `{ error: { code: 'NOT_IMPLEMENTED', message: 'MFA is out of scope for v1' } }`. Documented in Swagger so consumers know it exists but is disabled.
