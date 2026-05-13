# Phase 9: Pre-Ship Hardening (Sync Server Persistence + Cross-Surface Cleanup)

**Goal:** Make v0.1.0 actually ship-ready. Wire the Prisma-backed sync and auth stores against the schema authored in Phases 1-8 (currently runs on in-memory Maps), bring up a real Postgres deployment surface (Dockerfile.dev + compose + migration bootstrap), enforce RS256 in production, close the cross-surface audit-logging gap on manual conflict resolution, and clear the residual placeholder / debug / brittle-`unreachable!` artefacts surfaced by the pre-ship audit (2026-05-12).

**Surfaces:** All
**Dependencies:** Phase 08
**Complexity:** L

## §1 Local Schema Changes (Tauri SQLite)

No new tables.

### Modified tables

None.

### New enums

None. (`audit_log.action` enum already covers `conflict_resolve` per phase-08 §1.)

Migration file: `src-tauri/migrations/009_pre_ship.sql` ships as a no-op header (see §5 for the rationale; the file MUST exist so the migration runner records the version even when no DDL changes).

## §2 Server Schema Changes (Prisma / Postgres)

No new models or fields. `sync-server/prisma/schema.prisma` (19 models, 494 lines) is already complete — Phases 1-8 authored it. This phase wires it up.

### Modified models

None.

### New enums

None.

### Migration strategy

- Rename the existing `prisma/migrations/20260512000000_inventory_adjustments_delta_sign/` slot if it has not been applied (it currently exists as a "prepared" file per phase-06 §7.14) and verify it lex-orders cleanly against any future migrations.
- Adopt `prisma db push --accept-data-loss` as the dev-mode bootstrap (matches [.claude/rules/sync-server.md](../../.claude/rules/sync-server.md#prisma) `init-custom-sql.sql` workflow).
- Author `sync-server/prisma/init-custom-sql.sql` with the raw-SQL pieces previously declared in phase-03 §7.20 (`DoctorCheckPricing` paired partial unique index), phase-03 §7.21 (`InventoryConsumptionMap` paired partial unique index), phase-05 §7.33 (`inventory_adjustments` `BEFORE UPDATE` trigger that aborts), phase-06 §7.14 (`inventory_adjustments` per-reason delta-sign `CHECK`), and phase-05 §7.53 (`visits` CHECK extension for the 7 name-snapshot columns). `Dockerfile.dev` runs `prisma db push` then `psql -f init-custom-sql.sql` on every container start.

## §3 DDD Implementation

### Frontend (React)

Pages: none new.

Components: none new.

Behavioural changes only:

| File | Change |
|-|-|
| [src/providers/auth-provider.tsx:88](../../src/providers/auth-provider.tsx#L88) | Remove the `console.log("[AuthProvider] /api/auth not reachable...")`. The `setAuthPhase("standalone")` on the next line is already the user-visible signal. Keep `console.error` calls on real failure paths. |
| [src/pages/admin/inventory/detail.tsx:77](../../src/pages/admin/inventory/detail.tsx#L77) | Replace `defaultValue: "Pick a flat check type for now; subtype mapping not in this MVP form."` with `defaultValue: "Subtype mapping is not supported in this form. Pick a flat check type."` and ensure both `en/admin.json` and `ar/admin.json` carry `admin.inventory.consumption_subtype_picker` so the `defaultValue` is never user-visible. |
| [src/components/setup/first-launch-setup.tsx:80](../../src/components/setup/first-launch-setup.tsx#L80) | Ensure `setup.subtitle` is in `en/auth.json` and `ar/auth.json` (currently relies on `defaultValue`). No JSX change beyond verifying the i18n key exists. |
| [src/components/shell/sidebar.tsx:152](../../src/components/shell/sidebar.tsx#L152) | Confirm the "Coming soon" disabled item is intentional and matches [.claude/rules/design-system.md §11](../../.claude/rules/design-system.md). If it stays, ensure the i18n key resolves in both locales; if it goes, remove the markup entirely. |

Zustand stores: none.

React Query keys + hooks: none new.

Zod schemas: none new.

### Tauri / Rust

Domain layer changes:

| File | Change |
|-|-|
| [src-tauri/src/domains/inventory/service/mod.rs:282](../../src-tauri/src/domains/inventory/service/mod.rs#L282) | Replace `AdjustmentReason::ConsumeVisit => unreachable!()` with `AdjustmentReason::ConsumeVisit => return Err(AppError::Internal("ConsumeVisit reached construction switch after early-return guard".into()))`. The early-return at L224-L228 makes this path unreachable in practice, but the panic is a footgun for any future refactor that re-orders the guard. The error path is preferred over `unreachable_unchecked`. |
| [src-tauri/src/domains/catalog/service/operator_service.rs:222](../../src-tauri/src/domains/catalog/service/operator_service.rs#L222) | Re-read the comment that reads `// Phase-04 hardens the "block on open shift" rule. For now we cascade`. Phase 04 shipped. Decide: (a) if cascade is now the documented policy, **delete the "phase-04" forward-reference** and rewrite the comment to state the rule directly; or (b) if "block on open shift" is the intended rule, add the guard (call `OperatorShiftRepo::list_open(operator_id)` before cascading) and audit-log the block. Pick (a) unless the PRD requires the harder rule — the PRD does not, so option (a) is the default. |
| [src-tauri/src/domains/catalog/service/operator_service.rs:2](../../src-tauri/src/domains/catalog/service/operator_service.rs#L2) | Remove the `Open-shift block is a placeholder for phase-04.` line from the module doc-comment for the same reason. |
| [src-tauri/src/lib.rs:135-155](../../src-tauri/src/lib.rs) | Replace the five startup `eprintln!` lines with `tracing::info!` calls (no behavioural change; the embedded-mode banner is genuinely useful for troubleshooting). Keep them gated behind `if std::env::var("IDC_EMBEDDED_MODE").is_ok()` so standalone mode stays quiet. |

Repository traits: no changes.

SQLite repository: no changes.

Tauri commands: no new commands. Existing IPC surface is complete.

### Sync Server (Fastify)

The bulk of the phase lives here. Goal: swap every Memory* store for a Prisma-backed equivalent against the existing 19-model schema.

#### Domain layer (no changes to interfaces)

The current code is already structured against repository interfaces:

- `sync-server/src/app/sync/domain/repositories/` — interfaces consumed by `SyncPushService` / `SyncPullService` / `ConflictResolveService`.
- `sync-server/src/app/auth/domain/repositories/` — interfaces consumed by `AuthService`.

This phase adds Prisma implementations alongside the existing Memory ones. No interface changes; the swap is purely at the plugin-wire level.

#### Infrastructure additions

New Prisma repositories (one file per repo, alongside the existing `memory/` siblings):

| File | Implements | Notes |
|-|-|-|
| `sync-server/src/app/sync/infrastructure/prisma/audit-repo.ts` | `AuditLogRepository` | Append + `queryAudit` (phase-08 §7.20). LWW-immune (additive policy per phase-01 §7.16). |
| `sync-server/src/app/sync/infrastructure/prisma/processed-op-repo.ts` | `ProcessedOpRepository` | `has` / `remember` against `ProcessedOp` model. Composite PK `(op_id, entityId)`. |
| `sync-server/src/app/sync/infrastructure/prisma/sync-cursor-repo.ts` | `SyncCursorRepository` | `getCursor` / `bumpCursor`. Composite PK `(entityId, deviceId)` per phase-01 §7.19. |
| `sync-server/src/app/sync/infrastructure/prisma/conflict-parked-repo.ts` | `ConflictParkedRepository` | `park` / `load` / `resolve` / `listOpen`. |
| `sync-server/src/app/sync/infrastructure/prisma/entity-repo.ts` | All 15 syncable-entity repositories used by `SyncPushService.dispatchEntity` and `SyncPullService.changesSince` | Single file that dispatches on `entity` string to the appropriate Prisma model. Uses `prisma.$transaction([...])` per-batch for atomicity. LWW helper centralised here. |
| `sync-server/src/app/auth/infrastructure/prisma/user-store.ts` | `UserStore` + `RefreshTokenStore` | Replaces `MemoryUserStore`. Password hashes still Argon2id; refresh tokens still sha256 before persisting (phase-02 §7.21). |

The `MemorySyncStore` and `MemoryUserStore` files stay in the tree but move under `infrastructure/memory/` (already there) and become **test-only** fixtures consumed by `test/` suites. Production paths NEVER instantiate them.

#### Plugin wiring (the actual swap)

[sync-server/src/app/plugins/sync-services.ts](../../sync-server/src/app/plugins/sync-services.ts) rewrite:

```ts
import fp from 'fastify-plugin'
import { PrismaClient } from '@prisma/client'

import { PrismaAuditLogRepo } from '../sync/infrastructure/prisma/audit-repo.js'
import { PrismaProcessedOpRepo } from '../sync/infrastructure/prisma/processed-op-repo.js'
import { PrismaSyncCursorRepo } from '../sync/infrastructure/prisma/sync-cursor-repo.js'
import { PrismaConflictParkedRepo } from '../sync/infrastructure/prisma/conflict-parked-repo.js'
import { PrismaEntityRepo } from '../sync/infrastructure/prisma/entity-repo.js'
import { ConflictResolveService } from '../sync/service/conflict-service.js'
import { SyncPullService } from '../sync/service/pull-service.js'
import { SyncPushService } from '../sync/service/push-service.js'

export default fp(async (fastify) => {
  // Single shared client; honoured by the Prisma plugin (see below).
  const prisma = fastify.prisma

  const auditRepo = new PrismaAuditLogRepo(prisma)
  const processedRepo = new PrismaProcessedOpRepo(prisma)
  const cursorRepo = new PrismaSyncCursorRepo(prisma)
  const conflictRepo = new PrismaConflictParkedRepo(prisma)
  const entityRepo = new PrismaEntityRepo(prisma)

  fastify.decorate('pushService',
    new SyncPushService(entityRepo, processedRepo, cursorRepo, auditRepo, conflictRepo))
  fastify.decorate('pullService',
    new SyncPullService(entityRepo, cursorRepo))
  fastify.decorate('conflictService',
    new ConflictResolveService(conflictRepo, processedRepo, auditRepo))
})

declare module 'fastify' {
  interface FastifyInstance {
    pushService: SyncPushService
    pullService: SyncPullService
    conflictService: ConflictResolveService
  }
}
```

[sync-server/src/app/plugins/auth-services.ts](../../sync-server/src/app/plugins/auth-services.ts) rewrite: identical pattern. Construct `PrismaUserStore(prisma)` instead of `new MemoryUserStore()`. Bootstrap path (lines 29-36) stays, now persisting to Postgres.

New plugin: `sync-server/src/app/plugins/prisma.ts`

```ts
import fp from 'fastify-plugin'
import { PrismaClient } from '@prisma/client'

export default fp(async (fastify) => {
  const prisma = new PrismaClient({
    log: process.env.NODE_ENV === 'development' ? ['warn', 'error'] : ['error'],
  })
  await prisma.$connect()
  fastify.decorate('prisma', prisma)
  fastify.addHook('onClose', async () => { await prisma.$disconnect() })
})

declare module 'fastify' {
  interface FastifyInstance {
    prisma: PrismaClient
  }
}
```

Autoload order (per [.claude/rules/sync-server.md](../../.claude/rules/sync-server.md#autoload-order-sync-server)): `prisma.ts` ships at `name: 'prisma'`, marked as a dependency of `auth-services` and `sync-services` (`fp(..., { dependencies: ['prisma'] })`).

#### Conflict resolution audit-log row (phase-08 §1 gap closure)

[sync-server/src/app/sync/service/conflict-service.ts:67](../../sync-server/src/app/sync/service/conflict-service.ts#L67) currently calls `await this.conflicts.resolve(opId, tenantId, userId)` and returns. The phase-08 §1 audit enum includes `conflict_resolve`, but no writer emits it. Extend the constructor to take an `AuditLogRepository` and append an audit row in the same transaction as `conflicts.resolve`:

```ts
await this.prisma.$transaction(async (tx) => {
  await this.conflicts.resolveTx(tx, opId, tenantId, userId)
  await this.audit.appendTx(tx, {
    id: randomUUIDv7(),
    actorUserId: userId,
    entityIdTenant: tenantId,
    action: 'conflict_resolve',
    entity: parked.entity,
    entityRowId: parked.entityRowId,
    delta: { choice: input.choice, opId, resolveOpId: input.resolveOpId ?? null },
    at: new Date().toISOString(),
  })
})
```

The `ConflictParkedRepository.resolve` method splits into `resolve(opId, ...)` (public) and `resolveTx(tx, opId, ...)` (internal). The audit row is server-canonical — it lives only on the server until the next `/sync/pull` brings it down to the resolver's device.

#### Error-handler reach

[sync-server/src/app/auth/infrastructure/memory-user-store.ts:118,121](../../sync-server/src/app/auth/infrastructure/memory-user-store.ts) currently `throw new Error('invalid refresh token')` / `'expired refresh token'`. The Prisma replacement MUST throw `DomainError` (`AUTH_INVALID_REFRESH` / `AUTH_EXPIRED_REFRESH`, status 401) so the global error handler returns the right code instead of falling through to 500. Apply the same fix to the existing memory path if it stays in the tree as a test fixture.

#### JWT enforcement (`auth-jwt.ts:16`)

[sync-server/src/app/plugins/auth-jwt.ts:16](../../sync-server/src/app/plugins/auth-jwt.ts#L16) currently does `process.env.JWT_SECRET ?? 'dev-only-secret'`. Rewrite:

```ts
const publicKey = process.env.JWT_PUBLIC_KEY
const sharedSecret = process.env.JWT_SECRET
const isProd = process.env.NODE_ENV === 'production'

if (publicKey && publicKey.trim().length > 0) {
  await fastify.register(fjwt, { secret: { public: publicKey }, verify: { algorithms: ['RS256'] } })
} else if (!isProd && sharedSecret && sharedSecret.length >= 32) {
  fastify.log.warn('JWT running in HS256 dev fallback. Set JWT_PUBLIC_KEY for production.')
  await fastify.register(fjwt, { secret: sharedSecret })
} else {
  throw new Error(
    'JWT plugin: production requires JWT_PUBLIC_KEY (RS256). ' +
    'In non-production set JWT_SECRET to a 32+ char shared secret.'
  )
}
```

No silent `'dev-only-secret'` fallback. Tests get a `JWT_SECRET` env var set by their bootstrap (already true in current `test/` helpers).

#### Healthz wiring (`healthz.ts:36`)

[sync-server/src/app/routes/healthz.ts:36-40](../../sync-server/src/app/routes/healthz.ts#L36) hardcodes `db: 'ok'` and `redis: 'ok'`. Replace with real probes:

```ts
async () => {
  const dbOk = await fastify.prisma.$queryRaw`SELECT 1`.then(() => true).catch(() => false)
  // Redis is optional in v0.1.0; report 'ok' when REDIS_URL is unset (n/a) and probe when set.
  const redisOk = fastify.redis ? await fastify.redis.ping().then(() => true).catch(() => false) : true
  const migrationsApplied = await migrationsTableExists(fastify.prisma)
  return {
    status: dbOk && redisOk ? 'ok' as const : 'fail' as const,
    db: dbOk ? 'ok' as const : 'fail' as const,
    redis: redisOk ? 'ok' as const : 'fail' as const,
    migrationsApplied,
    version: '0.1.0',
  }
}
```

`HealthSchema.status` widens to `'ok' | 'fail'` (currently `Type.Literal('ok')`). 200 is returned regardless; the body indicates degradation.

#### Env schema (`.env.template` vs runtime reads)

The committed [sync-server/.env.template](../../sync-server/.env.template) lists `JWT_PUBLIC_KEY_PATH` and `JWT_PRIVATE_KEY_PATH` but the runtime ([auth-jwt.ts:15](../../sync-server/src/app/plugins/auth-jwt.ts#L15)) reads `JWT_PUBLIC_KEY` (PEM value). And the template is missing `JWT_SECRET`, `BOOTSTRAP_SUPERADMIN_EMAIL`, `BOOTSTRAP_SUPERADMIN_PASSWORD`, `BOOTSTRAP_TENANT_ID`, `METRICS_TOKEN`, `DEFAULT_ENTITY_ID`. Fix the template:

```
# Auth (RS256 in production)
JWT_PUBLIC_KEY=
# Dev-only HS256 fallback (NODE_ENV != production). Must be 32+ chars.
JWT_SECRET=
JWT_ACCESS_TTL_SECONDS=900
JWT_REFRESH_TTL_SECONDS=2592000

# First-launch superadmin bootstrap (phase-02 §7.21)
BOOTSTRAP_SUPERADMIN_EMAIL=
BOOTSTRAP_SUPERADMIN_PASSWORD=
BOOTSTRAP_TENANT_ID=
DEFAULT_ENTITY_ID=

# Internal Prometheus metrics token (phase-08 §7.17). Unset = endpoint 404s.
METRICS_TOKEN=
```

Also add `@fastify/env` schema validation in a new plugin `sync-server/src/app/plugins/env.ts` so a missing-or-empty `DATABASE_URL` fails fast at boot instead of falling through to a Prisma connection error.

Verify the on-disk dev `.env` is `.gitignore`'d (it currently is — `git ls-files` returned only `.env.template`). Add a CI step to ensure `.env` never gets committed.

#### Routes

No new routes. `routes/metrics.ts` keeps `hide: true` (correct: Prometheus scrape endpoint gated by `X-Internal-Token`; not for human consumption). Document this rationale in `metrics.ts` so future audits don't re-flag it.

## §4 Business Logic

### Sync server: SYNC_STORE env var

The current code comment ([sync-services.ts:13](../../sync-server/src/app/plugins/sync-services.ts#L13)) names a `SYNC_STORE=memory|prisma` env var that does not exist. Delete the comment; production wiring is unconditional Prisma. Tests construct services with `MemorySyncStore` directly via test bootstrap, not via env.

### Refresh-token persistence semantics

`MemoryUserStore.rotate` (currently L113-L135) holds both a `tokenHashes` Map and a `tokens` Map. Prisma replacement uses the `RefreshToken` model's `tokenHash @unique` index — single source of truth, no parallel structure to drift. Behaviour:

1. Lookup by `sha256(presentedToken)` against `tokenHash`.
2. If `revokedAt` is non-null OR `expiresAt < now`, throw `DomainError('AUTH_INVALID_REFRESH', ..., 401)` or `('AUTH_EXPIRED_REFRESH', ..., 401)`.
3. Atomic rotation: `prisma.$transaction([ revoke current, insert new ])`. No window where neither token is valid.
4. Both new and old rows live until the retention vacuum prunes revoked rows older than `JWT_REFRESH_TTL_SECONDS`.

### Audit-log emission on conflict resolution

See §3 — single `prisma.$transaction` around `conflicts.resolve` + `audit.append`. The audit row's `delta` JSONB carries `{ choice, opId, resolveOpId }`. The resolver UI is unchanged (it already triggers the request; the row appears on next pull).

### Cursor semantics under Prisma

`PrismaSyncCursorRepo.bumpCursor` MUST use `upsert` against the composite PK `(entityId, deviceId)` per phase-01 §7.19. The cursor value is `Date` (Postgres `timestamptz`); compare lexicographically on ISO strings is equivalent for monotonic UTC. Pull queries use `where: { entityId, updatedAt: { gt: cursor } }` ordered by `updatedAt asc` then `id asc` for stable pagination.

### LWW helper

Centralise the `(version, updatedAt, originDeviceId)` tiebreak (currently inlined in `MemorySyncStore.upsertLWW`) inside `PrismaEntityRepo.lwwShouldApply(serverRow, incoming) -> boolean`. Push handlers call this before issuing the `prisma.<model>.update`. Same algorithm as phase-03 §7.17; just moved to one place.

## §5 Infrastructure Updates

### TENANT_MODELS

No new entries. The current set (15 syncable models) is correct and already declared in the Prisma extension config.

### Audit triggers

No new triggers. Existing `with_audit` (Rust) + `conflict_resolve` audit row (this phase) cover the actions.

### Local SQLite indexes

None.

### Tauri capabilities

No changes.

### New Tauri plugin registrations

None.

### New Fastify plugins / queues

- `sync-server/src/app/plugins/prisma.ts` — `PrismaClient` + lifecycle hook.
- `sync-server/src/app/plugins/env.ts` — `@fastify/env` validation (fails boot on missing `DATABASE_URL` / `NODE_ENV`).
- `sync-server/src/app/plugins/redis.ts` is intentionally deferred — v0.1.0 does not require a queue; phase-10+ may add it.

### Docker (sync-server)

New files:

- `sync-server/Dockerfile.dev`:

  ```dockerfile
  FROM node:22-alpine
  WORKDIR /app
  RUN corepack enable && corepack prepare pnpm@latest --activate
  COPY package.json pnpm-lock.yaml ./
  RUN pnpm install --frozen-lockfile
  COPY . .
  RUN pnpm prisma generate
  RUN pnpm build
  EXPOSE 3161
  CMD ["sh", "-c", "pnpm prisma db push --accept-data-loss && psql \"$DATABASE_URL\" -f prisma/init-custom-sql.sql && node dist/main.js"]
  ```

- `sync-server/docker-compose.yaml`:

  ```yaml
  services:
    sync-db:
      image: postgres:16-alpine
      environment:
        POSTGRES_USER: postgres
        POSTGRES_PASSWORD: postgres
        POSTGRES_DB: idc_sync
      ports:
        - "5449:5432"
      volumes:
        - sync_db_data:/var/lib/postgresql/data
    sync-server:
      build:
        context: .
        dockerfile: Dockerfile.dev
      environment:
        NODE_ENV: development
        DATABASE_URL: postgresql://postgres:postgres@sync-db:5432/idc_sync
        PORT: 3161
        JWT_SECRET: ${JWT_SECRET}
        BOOTSTRAP_SUPERADMIN_EMAIL: ${BOOTSTRAP_SUPERADMIN_EMAIL:-}
        BOOTSTRAP_SUPERADMIN_PASSWORD: ${BOOTSTRAP_SUPERADMIN_PASSWORD:-}
        BOOTSTRAP_TENANT_ID: ${BOOTSTRAP_TENANT_ID:-}
        DEFAULT_ENTITY_ID: ${DEFAULT_ENTITY_ID:-}
      ports:
        - "3161:3161"
      depends_on:
        - sync-db
      volumes:
        - ./src:/app/src
        - ./prisma:/app/prisma
  volumes:
    sync_db_data:
  ```

- `sync-server/.dockerignore` covering `node_modules`, `dist`, `.env`, `coverage`.

- `sync-server/prisma/init-custom-sql.sql` — see §2.

### CI guardrail

Add a one-line check in any CI job:

```bash
test "$(git ls-files sync-server/.env)" = "" || (echo "::error::sync-server/.env must not be tracked" && exit 1)
```

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` — must pass. The `unreachable!()` swap to `AppError::Internal` keeps the type-level proof valid; clippy will not regress.
2. `pnpm lint && pnpm build` — frontend builds cleanly. The three i18n string changes do not alter the bundle.
3. `cd sync-server && pnpm lint && pnpm build:ts && pnpm test` — all existing tests pass against the new Prisma plugin by injecting `JWT_SECRET` and pointing `DATABASE_URL` at an ephemeral Postgres (testcontainers or a CI service). Memory stores stay imported only by test bootstrap.
4. `cd sync-server && docker compose up -d sync-db && DATABASE_URL=postgresql://postgres:postgres@localhost:5449/idc_sync pnpm prisma db push --accept-data-loss` — schema applies cleanly against a real Postgres.
5. `docker compose up -d sync-server` then `curl http://localhost:3161/healthz` — returns `{ status: 'ok', db: 'ok', redis: 'ok', migrationsApplied: true, version: '0.1.0' }`. Kill the DB container and re-curl; the response body flips `db: 'fail'` and `status: 'fail'`, still HTTP 200.
6. `JWT_PUBLIC_KEY=""` and `JWT_SECRET=""` and `NODE_ENV=production` — server refuses to boot with the literal error from `auth-jwt.ts`. `NODE_ENV=development` with `JWT_SECRET` >= 32 chars boots with a `warn`-level log line.
7. **Persistence round-trip (the real test):** log in as the bootstrap superadmin, push a doctor row through `/sync/push`, `docker compose restart sync-server`, log in again, pull via `/sync/pull?since=<empty>` — the doctor row survives the restart. Same for an open refresh token.
8. **Conflict resolution + audit:** force a `manual` conflict on the `visits` entity (two devices touch the same row offline). Confirm `/sync/conflicts/:opId/resolve` returns `{ status: 'applied' }` and a subsequent `/sync/pull` returns a new `audit_log` row with `action = 'conflict_resolve'` and `delta.choice = ...`. The local SQLite audit pulls it down through the existing Phase-1 pull path.
9. **Frontend behavioural smoke:**
   - `pnpm tauri dev`, open `/admin/inventory/<id>` while logged in as superadmin, add a consumption row against a `has_subtypes=true` parent. The error message must be the i18n-resolved string in both `en` and `ar`, never the `defaultValue`.
   - Open DevTools. Reload the app. The `console.log("[AuthProvider] /api/auth not reachable...")` line is gone; `console.error` lines on real failures still appear.
   - Open the first-launch setup modal (manually unset the sync URL setting). The subtitle and label both render the i18n string in the active locale.
10. **No regressions:** the full pre-push battery from [.claude/rules/dev-workflow.md §9](../../.claude/rules/dev-workflow.md) passes on every surface (lint, type-check, build, clippy, test, sync server pnpm test + curl smoke). Total Rust test count delta = 0; total sync-server test count grows by ≥ 6 (4 Prisma-repo unit tests + 1 healthz probe test + 1 conflict-resolve-audit integration test).

## §7 Open Decisions

These do not block the phase but should be answered during planning:

1. **Postgres image pinning.** `postgres:16-alpine` vs a specific minor — pin to `16.4-alpine` to match what the next stable release shipped against. Owner: deployment.
2. **Prisma migration model for v0.1.0.** `prisma db push` is the documented dev path ([sync-server.md §Prisma](../../.claude/rules/sync-server.md)). For production deployments, decide between (a) sticking with `db push` (acceptable for a single-site clinic) or (b) cutting `prisma migrate deploy` from a `prisma/migrations/` directory generated against an empty database. Default to (a) for v0.1.0; revisit at v0.2.0 if multi-environment rollout becomes a concern.
3. **Redis posture.** v0.1.0 has no BullMQ usage in the phase plans. The Prisma plugin can boot without Redis; the healthz probe reports `redis: 'ok'` when `REDIS_URL` is unset (interpret as "not configured"). Keep Redis out of compose entirely until a phase needs it.
4. **Bootstrap secrets in compose.** `BOOTSTRAP_SUPERADMIN_PASSWORD` in `docker-compose.yaml` is dev-only convenience. Production deployment must inject these via the host environment, never the compose file. Document in `sync-server/README.md`.
5. **`operator_service.rs` cascade rule.** Section §3 above defaults to option (a) "cascade is the policy, delete the phase-04 forward-reference comment". If the planning team wants option (b) (block on open shift), add a §7.1 in this phase with the exact guard signature and the new IPC error code, plus a frontend toast key.

## §8 Out of Scope

- Anything not surfaced by the 2026-05-12 pre-ship audit.
- Adding new Tauri commands or new screens.
- Schema changes (the schema is complete).
- BullMQ / Redis introduction.
- Multi-tenant deployment topology beyond `entityId` scoping that already exists.
- Self-updater wiring (Business OS owns child-app updates per [.claude/rules/tauri.md](../../.claude/rules/tauri.md)).
- Performance tuning beyond what naturally happens when the in-memory store is replaced by Postgres with the existing indexes.

## §9 Audit Provenance

Findings consolidated from the pre-ship audit run on 2026-05-12. Severities verified against ground truth before inclusion (the original audit report contained two false positives that are NOT in this phase: a claimed missing `prisma/schema.prisma` — the file exists with 19 models — and a brittle-`unreachable!()` flagged as a runtime panic risk — it is guarded by an early return, demoted to a code-smell cleanup in §3 above).

| Finding | Severity | Section |
|-|-|-|
| Sync routes wired to `MemorySyncStore` | BLOCKER | §3 Sync Server, §4 |
| Auth routes wired to `MemoryUserStore` | BLOCKER | §3 Sync Server, §4 |
| `JWT_SECRET ?? 'dev-only-secret'` fallback in production | BLOCKER | §3 Sync Server (auth-jwt rewrite) |
| `healthz` hardcoded `db: ok` | BLOCKER | §3 Sync Server (healthz wiring) |
| No `Dockerfile` / `docker-compose.yaml` for sync-server | BLOCKER | §5 |
| Manual conflict resolution emits no audit row | BLOCKER | §3 Sync Server, §4 |
| `.env.template` missing keys + wrong variable names | SHIP-CONCERN | §3 Sync Server (env schema) |
| `memory-user-store.ts` raw `Error` throws | SHIP-CONCERN | §3 Sync Server (error-handler reach) |
| `console.log` in `auth-provider.tsx` | SHIP-CONCERN | §3 Frontend |
| MVP placeholder text in `admin/inventory/detail.tsx` | SHIP-CONCERN | §3 Frontend |
| Inline English `defaultValue` in `first-launch-setup.tsx` | SHIP-CONCERN | §3 Frontend |
| Brittle `unreachable!()` in `inventory/service/mod.rs:282` | NIT | §3 Tauri/Rust |
| Stale "phase-04" comment in `operator_service.rs` | NIT | §3 Tauri/Rust |
| Startup `eprintln!` in `lib.rs` | NIT | §3 Tauri/Rust |
| `.env` not git-tracked (false positive in original audit) | n/a | verified clean |
| `metrics.ts hide: true` (false positive — intentional internal endpoint) | n/a | verified clean |
| `prisma/schema.prisma` missing (false positive — 19 models, 494 lines) | n/a | verified clean |
