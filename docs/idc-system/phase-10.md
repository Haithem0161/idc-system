# Phase 10: Sync Correctness, Auth Binding & Multi-Device Round-Trip

**Goal:** Make the declared offline-first conflict contract actually true on the pull side, close the server-side auth-binding holes, add schema-version negotiation, and prove a real two-device sync round trip against real Postgres -- the gating release work for a multi-device clinic deployment.

**Surfaces:** All (Frontend | Tauri/Rust | Sync Server)
**Dependencies:** Phase 09 (complete)
**Complexity:** L

---

## 0. Context & Scope

This phase implements the three highest-priority workstreams from
[STUDY-AND-CONTRACT.md](./STUDY-AND-CONTRACT.md) -- WS-1 (sync round-trip
correctness), WS-2 (auth & security), and the WS-1 verification gate (two-device
E2E). Launch decision: **multi-device from day one**, so the CRITICAL conflict
mismatch and the round-trip proof are hard blockers.

A code-extraction pass (2026-06-15) read the exact current source at every change
site. It found that **several items the contract flagged are already shipped**:
production `DATABASE_URL` / `JWT_PUBLIC_KEY` enforcement (`env.ts:67-84`), the
eager Prisma probe (`prisma.ts:31-48`), and ProcessedOp idempotency in the
resolve path (`conflict-service.ts:60-164`). Those are **verify-don't-rebuild**.
The genuine, line-cited gaps are narrower and sharper, and are the scope below.

**This phase does NOT touch:** local SQLite schema (no migration; the dirty/version
columns already exist), the 15 syncable entity tables, the frontend design system,
UI polish (WS-5), Rust panic hardening (WS-6), or feature gaps (WS-8). No new
Prisma models. One new server route (`GET /auth/profile`). Two new optional sync
headers. Zero new IPC commands except where noted in 3.2.

### Workstream-to-task map

| WS | Task group | Severity origin |
|-|-|-|
| WS-1 | T1 conflict-policy dispatch (pull-side park), T2 dirty-flag race, T3 schema-version negotiation, T4 merged-payload re-validation | CRITICAL #1, HIGH #5, HIGH #8, HIGH #10 |
| WS-2 | T5 refresh/logout subject binding, T6 `GET /auth/profile`, T7 strict-prod JWT alg | CRITICAL #2, CRITICAL #4, HIGH #6 |
| Gate | T8 two-device E2E harness + clinical-day fixture | HIGH #9 |
| WS-3 (verify) | T9 metrics instrumentation, T10 `migrate deploy` + backup, T11 tenant-scope audit | HIGH #15-adjacent, MEDIUM #16, HIGH #14 |

---

## 1. Local Schema Changes (Tauri SQLite)

**No new tables. No migration file.** Every syncable table already carries
`version` and `dirty` (used by the LWW gate today). T1/T2 change *how* the pull
loop uses them, not the schema.

One optional addition, deferred unless T3 needs it: a `schema_version` value in
`sync_state` is **not** required -- the client schema version is a build-time
constant (`MIGRATION_SCHEMA_VERSION`), mirroring how `X-App-Version` already works.
No DDL.

---

## 2. Server Schema Changes (Prisma / Postgres)

**No new models.** `ConflictParked`, `ProcessedOp`, `SyncCursor`, `RefreshToken`,
`User`, `Setting`, `Visit` already exist with all needed columns
(`schema.prisma:12-82`, verified). The `RefreshToken.userId` column already exists
and is the binding point for T5.

No new enums. No new indexes (the `(entityIdTenant, *)` isolation indexes already
exist).

**Migration policy change (T10):** stop using `prisma db push --accept-data-loss`
at container start (`Dockerfile.dev:28`) for production; switch to
`prisma migrate deploy` against committed migration files, preceded by a
`pg_dump` backup. This is an ops/Docker change, not a schema change. The existing
`init-custom-sql.sql` (partial unique indexes, append-only trigger, CHECK
constraints) is preserved and runs after `migrate deploy`.

---

## 3. DDD Implementation

### 3.1 Tauri / Rust

#### T1 -- Pull-side conflict-policy dispatch (CRITICAL)

**Current (verified):** `puller.rs:170-247` hard-codes a `match change.entity`
that calls each `apply_*_change` handler. Every handler in `puller_entities.rs`
applies an LWW gate, including `apply_settings_change` (`:6-68`) and
`apply_visits_change` (`:727-865`), whose ON CONFLICT clause is
`WHERE settings.version < excluded.version AND settings.dirty = 0`
(`:43-44`, `:796-797`). `policy_for()` (`conflict/mod.rs:35-80`) declares
`settings` and `visits` as `Policy::Manual` but is never consulted on pull --
the file's own NOTE (`:42-60`) admits this.

**Server already parks on push** (`push-service.ts:159-186`, via
`detectSettingConflict`/`detectVisitConflict` in `entity-store.ts:128-152`). The
gap is purely the **pull** side: a server row with a higher version silently
LWW-overwrites unsynced local edits to settings (money math) and visits.

**Change:**

1. In `puller.rs::apply_changes`, route each change through `policy_for(entity)`
   to decide handling, replacing the implicit per-arm assumption:
   - `Policy::AdditiveOnly` -> existing `INSERT OR IGNORE` handlers (unchanged).
   - `Policy::LastWriteWins` -> existing LWW handlers (changed only by T2).
   - `Policy::Manual` -> new park-on-divergence path (below).
   Keep the `match` for the *handler function* (entity-specific SQL), but gate
   the Manual entities through a divergence pre-check before applying.

2. Add a pull-side divergence detector mirroring the server's, in a new
   `puller_entities.rs::detect_local_settings_divergence` /
   `detect_local_visit_divergence`. Logic (settings):
   - Read the local row `(version, dirty, value, value_type)` inside the tx.
   - If a local row exists AND (`dirty = 1` OR `local.version != incoming.version`)
     AND the content differs -> **divergence**. Do NOT apply; park locally.
   - Else apply via the existing upsert (covers first-seen and clean fast-forward).

3. **Local parking:** insert the incoming-vs-local pair into the local outbox as a
   parked conflict so `sync_list_conflicts` surfaces it. The outbox already has a
   `park()` / parked state (`pusher.rs:110-127`); add a
   `park_pulled_conflict(entity, entity_id, local_payload, server_payload, reason)`
   on the outbox repo that writes a parked row with `reason = "manual_policy_pull_divergence"`.
   Emit `CONFLICT_EVENT` (`engine.rs:22-23`, already wired) so the resolver UI
   refetches (`useSyncConflicts`, `queries.ts:64-74`).

4. Update `conflict/mod.rs` to remove the stale NOTE and add a doc test asserting
   the dispatch site calls `policy_for`. Keep the existing 4 mapping tests.

**Ripple:** the resolver UI currently handles server-returned conflicts; pull-side
parked conflicts must use the same `Conflict`/`ServerConflict` envelope shape so
`sync_resolve_conflict` resolves them identically. The resolve flow already bumps
version above both sides and re-pushes -- reuse it.

#### T2 -- Close the dirty-flag stale-read race (HIGH)

**Current (verified):** every handler reads `SELECT version FROM <t> WHERE id=?`
(`puller_entities.rs:15-24`) inside the tx, then early-exits in Rust if
`incoming <= existing`, then runs the `INSERT...ON CONFLICT ... WHERE version <
excluded.version AND dirty = 0`. A concurrent local mutation between the SELECT and
the ON CONFLICT evaluation can flip `dirty=1`/bump `version`, and the row is then
silently skipped while the Rust early-exit was decided on stale data. SQLite (WAL,
5s busy_timeout, `sqlite.rs:19-43`) has no row locks; the pool hands out separate
connections.

**Change:** delete the Rust-level `SELECT version` pre-read and early-exit
(`:15-24` pattern) from **all ~14 LWW handlers**. Rely solely on the atomic SQL
`WHERE version < excluded.version AND dirty = 0` gate. Inspect
`query.rows_affected()`; `0` means "stale or locally dirty" -> correct silent skip.
This makes the version check atomic with the conflict evaluation.

For Manual entities (settings, visits) the T1 divergence pre-check replaces the
LWW pre-read; same atomicity principle -- the divergence read and the park/apply
happen in one tx with no inter-statement window that changes the decision.

**Ripple:** tests that count "applied rows = 0 on stale" still pass (the SQL gate
yields the same outcome). Add a concurrent-mutation regression test (4.T2).

#### T3 -- Schema-version negotiation (HIGH)

**Current (verified):** client sends `X-App-Version` + `X-Device-Id` on every
sync request (`http_client.rs`); server `version-gate` plugin compares against
`MIN_CLIENT_VERSION` and returns 426 `UPGRADE_REQUIRED`; client maps to
`AppError::UpgradeRequired`; engine emits `app:upgrade_required`
(`engine.rs` event, `sync-events.ts:134` listener). No schema version exists.

**Change (fail-open, additive):**

- Define `const SYNC_SCHEMA_VERSION: u32` in the Rust client (bump with each
  syncable migration; today = 9 to match local-migration-009). Send it as
  `X-Schema-Version` on push/pull (`http_client.rs` request builders).
- Server `env.ts`: add `MIN_CLIENT_SCHEMA_VERSION` (default `''`/0). `version-gate`
  plugin: if set and `X-Schema-Version` < min, return 426 with
  `{ code: 'UPGRADE_REQUIRED', reason: 'schema_version', minSchemaVersion }`.
  If the header is absent (older client) -> fail-open (do not reject), matching the
  existing unparseable-version behavior.
- Add `server_schema_version: Type.String()` to `PullResponseSchema` so the client
  can log drift even when the gate passes.
- Reuse the `app:upgrade_required` event/UX (no new event) -- a schema mismatch is
  also "must upgrade the client." Extend the 426 mapping in `error.rs` only if we
  need to message app-vs-schema differently; default reuse.

**Why it blocks:** a future server migration adding a required field would make old
clients push payloads missing it with no protocol guard -> silent data loss. The
gate makes that loud.

#### Tauri command registration

No new IPC commands for T1/T2/T3 (they live inside the existing engine loop and
`sync_list_conflicts`/`sync_resolve_conflict`). Confirm no handler is added without
registering in `lib.rs::generate_handler!`.

### 3.2 Sync Server (Fastify)

#### T4 -- Re-validate merged payloads on resolve (HIGH)

**Current (verified):** `conflict-service.ts::applyChosen` (`:177-212`) upserts the
client `merged` payload after only an `is-object` check, then `store.upsertSetting`
/ `store.upsertVisit`. The push path validates every entity first
(`push-service.ts::validateVisit :644-743`, settings checks `:159-186`). The route
doc promises "must validate against the entity schema" but the code does not.

**Change:** before `applyChosen` upserts a `merged` (or `local`) payload, run it
through the **same validators** the push path uses. Extract `validateVisit`,
`validateSetting` (and the `PROTECTED_SETTING_KEYS` delete guard) into a shared
`sync/service/validators.ts` imported by both `push-service.ts` and
`conflict-service.ts`. On failure throw `DomainError('VALIDATION_ERROR', ..., 422)`.
This closes the bypass where a malformed merge corrupts the store.

**Route/schema:** `ResolveBodySchema` (`conflicts.ts:8-16`) unchanged. Add a `422`
response to the route's response map. Swagger description already claims this; now
true.

#### T5 -- Bind refresh/logout to JWT subject (CRITICAL)

**Current (verified):** `auth.ts:94-118` -- `/auth/refresh` and `/auth/logout` have
no `onRequest: [fastify.authenticate]` and pass only the body `refreshToken` to the
service. `auth-service.ts:70-98` and `user-store.ts:126-174` find the token by
`tokenHash` with no `userId`/`jwt.sub` check. A leaked refresh token is a skeleton
key.

**Change:**

- Add `onRequest: [fastify.authenticate]` to both routes; add `security:
  [{ bearerAuth: [] }]` to their schemas.
- Pass `jwt.sub` (`request.user.sub`, typed via the augmentation at
  `auth-jwt.ts:84-99`) into `authService.refresh` / `authService.logout`.
- In `user-store.ts::rotate` and `::revokeByPlaintext`, add a predicate: the loaded
  `refreshToken.userId` MUST equal the passed `jwtSub`; mismatch ->
  `DomainError('NOT_AUTHENTICATED', 'token does not belong to subject', 401)`.
  Keep the existing expiry/revocation checks.

**Ripple (client):** the Rust client must send `Authorization: Bearer <access>` on
refresh/logout. `auth_refresh_impl` (`commands.rs:535-555`) already holds the access
token in `AppState`; thread it into the http call. `auth_logout_impl`
(`commands.rs:113-130`) currently writes a local audit row and does NOT call the
server logout -- decide: either start calling server logout (preferred, revokes the
token) with the bearer header, or document that logout is local-only. Plan:
**call server logout with the bearer header** so the refresh token is actually
revoked server-side.

#### T6 -- `GET /auth/profile` ground-truth endpoint (CRITICAL)

**Current (verified):** no profile route exists (`auth.ts` has login/refresh/logout/
change-password/public-key/bootstrap). The client cannot verify the server agrees on
identity.

**Change:** add `GET /auth/profile`:
- `onRequest: [fastify.authenticate]`, `security: [{ bearerAuth: [] }]`.
- Handler: `const sub = request.user.sub`; `authService.getProfile(sub)` ->
  `users.getById(sub)` (`user-store.ts:47-50`); 404 if absent/inactive.
- Response `ProfileResponse = Type.Object({ id, email, name, role, entityId })`
  (no `passwordHash` -- unlike `LoginResponse`). Tags `['auth']`, full Swagger.
- Client: add an `auth_fetch_profile` path (or fold into the existing
  `auth_current_user` verification) so `useCurrentUser` can cross-check server
  identity after login/refresh.

#### T7 -- Strict RS256 in any non-dev environment (HIGH)

**Current (verified):** `auth-jwt.ts:26-60` keys `isProd` on
`NODE_ENV==='production'` exactly. A `staging`/unset deploy with `JWT_SECRET`
present silently registers HS256 (`:52`). The public key is served at
`/auth/public-key`, so an HS256-mode server plus a public key is a forge risk.

**Change:** invert the gate -- treat anything that is not an explicit dev/test env
as production-strict. Compute `const isDevLike = NODE_ENV === 'development' ||
NODE_ENV === 'test'`. Allow the HS256 fallback ONLY when `isDevLike`. Otherwise
require both RS256 keys or throw at boot (the existing throw at `:56-59` already
covers the missing-key case; this just narrows when HS256 is permitted).
`preship-guardrails.sh` already bans the `dev-only-secret` literal; add a guard
that `JWT_SECRET` is empty when `NODE_ENV` is not dev-like (optional).

#### T9 -- Instrument metrics on push/pull (MEDIUM, WS-3 verify)

**Current (verified):** `metrics.ts` defines `observeSyncPush`/`observeSyncPull`/
`incrConflict`; `push.ts:33-60` and `pull.ts:30-36` never call them -- `/metrics`
always returns zeros.

**Change:** wrap the service calls in `push.ts`/`pull.ts` with a timer and call
`fastify.metricsRegistry.observeSyncPush(durationMs, status)` /
`observeSyncPull(...)`; `incrConflict()` when conflicts are returned. No schema
change; `/metrics` stays token-gated (`metrics.ts:11-35`).

#### T11 -- Tenant-scoping audit (HIGH, WS-3 verify)

**Current (verified):** tenant isolation is manual (`tenant.ts:13-31` sets
`request.tenantId`; every Prisma repo includes `WHERE entityIdTenant`). No Prisma
extension/RLS. One missed filter = cross-tenant leak.

**Change (audit, not feature):** enumerate every Prisma query in
`sync-server/src/app/**/infrastructure/prisma/*.ts` and the services, and assert
each read/write either filters or stamps `entityIdTenant`, or is provably
tenant-agnostic. Produce a checklist table in `status.md` blockers and add a
unit/contract test that a cross-tenant pull/push/resolve cannot see another
tenant's rows. Fix any gap found.

### 3.3 Frontend (React)

Minimal. T1/T3 reuse existing wiring:
- `useSyncConflicts` (`queries.ts:64-74`) already refetches on `sync:conflict`;
  pull-side parked conflicts surface through the same hook -- verify the resolver
  page renders a pull-divergence reason string (add an i18n key
  `sync_conflicts.reason.manual_policy_pull_divergence` in `en` + `ar`).
- T3 schema mismatch reuses the existing `app:upgrade_required` UX -- no new page.
- T6: extend `useCurrentUser` to optionally cross-check `GET /auth/profile` after
  login/refresh (surfaces a mismatch toast). No new route.

No new Zustand stores. No new pages.

---

## 4. Business Logic & Verification

### Sync semantics (post-change)

| Entity class | Pull-apply | Push | Conflict |
|-|-|-|-|
| AdditiveOnly (`audit_log`, `operator_shifts`, `inventory_adjustments`) | `INSERT OR IGNORE` | append | none (both survive) |
| LastWriteWins (users, 8 catalog, `inventory_items`, `patients`) | atomic SQL gate only (T2) | LWW by `(updated_at, origin_device_id)` | last writer wins |
| Manual (`settings`, `visits`) | divergence pre-check -> apply clean fast-forward OR park locally (T1) | server parks divergence (existing) | resolver UI, both sides |

Idempotency key: `op_id` (UUID v7) on push; `resolve_op_id = sha256(op_id|choice|
canonical_merged)` on resolve (existing, verified working).

### T8 -- Two-device round-trip gate

**Current (verified):** `e2e/specs/multi-device/{conflict-round-trip,pull-fan-out}.spec.ts`
are `this.skip()` stubs (`gate.ts::multiDeviceDescribe` requires
`RUN_FULL_E2E=true && MULTI_DEVICE=true`). `clinical-day.sql` is an empty scaffold;
`scripts/regen-clinical-day.ts` does not exist. `wdio.conf.ts` boots ONE binary; no
sync-server in the harness. Only `release.yml` runs in CI -- no E2E job.

**Build:**
1. `scripts/regen-clinical-day.ts` -> populate `clinical-day.sql` (users, catalog,
   200 patients, 30 visits, inventory) loadable into both SQLite and Postgres
   (per the scaffold's documented target contents).
2. A harness that: `docker compose up` the sync-server + Postgres; builds the Tauri
   debug binary once; spawns **two** binaries with distinct app-data dirs +
   device-ids pointed at the same server; drives both via separate WebdriverIO
   sessions.
3. Implement `pull-fan-out.spec.ts`: device A creates+pushes a visit; device B's
   pull surfaces it. Implement `conflict-round-trip.spec.ts`: both edit the same
   visit/settings offline; reconnect; second pusher gets parked; resolver appears on
   both; resolving on one propagates on next pull.
4. Add a CI workflow `e2e.yml` (`workflow_dispatch` + nightly) that runs the gate
   with `RUN_FULL_E2E=true MULTI_DEVICE=true`. Keep it out of the per-commit path
   (too slow); pre-push stays as-is.

**This is the release gate:** the phase is not "done" until pull-fan-out and
conflict-round-trip pass green against real Postgres.

### T10 -- Production migrations + backup

Replace `Dockerfile.dev:28` CMD for the prod image: generate Prisma migration files
locally (`prisma migrate dev`), commit them, and have the prod start run
`pg_dump` -> `prisma migrate deploy` -> `psql -f init-custom-sql.sql` -> start.
Never `db push --accept-data-loss` in production. (Context7: `migrate deploy` reads
the URL from config, applies pending migrations idempotently, never drops data.)

### Verification checklist

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings` -- clean.
2. `cd src-tauri && cargo test` -- all pass, incl. new T1/T2 tests.
3. `pnpm lint && pnpm build` -- clean (incl. `lint-i18n`, `lint-rtl`).
4. `cd sync-server && pnpm test` -- all pass, incl. new T4/T5/T6/T7/T11 tests.
5. **T1:** a pulled settings/visit row that diverges from a dirty local row PARKS
   (does not overwrite); `sync_list_conflicts` returns it; resolving propagates.
6. **T2:** concurrent-mutation regression -- local mutation during pull does not get
   clobbered; `dirty=1` preserved; pushes later.
7. **T3:** a client sending `X-Schema-Version` below `MIN_CLIENT_SCHEMA_VERSION` gets
   426; a client omitting the header is NOT rejected (fail-open).
8. **T4:** resolving with a malformed `merged` payload returns 422, store unchanged.
9. **T5:** refresh/logout without `Authorization` -> 401; with a bearer whose `sub`
   != token's `userId` -> 401.
10. **T6:** `GET /auth/profile` returns the JWT subject's `{id,email,name,role,entityId}`,
    no `passwordHash`; 401 without auth.
11. **T7:** booting with `NODE_ENV=staging` + only `JWT_SECRET` -> refuses to start
    (no HS256 outside dev/test).
12. **T8 (gate):** `RUN_FULL_E2E=true MULTI_DEVICE=true pnpm test:e2e` -> pull-fan-out
    and conflict-round-trip green against docker-compose Postgres.
13. `cargo test` + `pnpm test` regression -- no prior tests broken.
14. `bash tools/preship-guardrails.sh` -- clean.

---

## 5. Infrastructure Updates

- No new Tauri capabilities/plugins (T8 spawns binaries via the test harness, not
  the app).
- Sync server: new `MIN_CLIENT_SCHEMA_VERSION` in `env.ts` schema + `.env.template`.
  New shared `validators.ts`. New `GET /auth/profile` route (autoloaded).
- New `e2e.yml` CI workflow (dispatch + nightly). New `scripts/regen-clinical-day.ts`.
- `Dockerfile` prod variant: `migrate deploy` + `pg_dump` (T10). Commit Prisma
  migration files.
- No new BullMQ/queues.

---

## 6. Sequencing & Dependencies

```
T7 (strict JWT) ─┐
T5 (bind) ───────┼─> WS-2 done (parallel, independent of WS-1)
T6 (profile) ────┘

T2 (race) ──> T1 (manual park) ──┐
T3 (schema-ver) ─────────────────┼─> WS-1 done
T4 (merged re-validate) ─────────┘

T9 (metrics), T11 (tenant audit), T10 (migrate deploy) ── WS-3 verify (parallel)

[WS-1 done] + [T8 harness + clinical-day fixture] ──> ROUND-TRIP GATE (green) ──> phase complete
```

T2 lands before T1 (T1's Manual path reuses the atomic-gate principle). WS-2 (T5-7)
runs fully parallel to WS-1. T8 depends on WS-1 landing to assert the corrected
behavior. T10 is required before the gate runs against a persistent Postgres.

---

## 7. Section 7+: Gap Additions

_(Reserved for gap-analysis passes against this phase, per
[.claude/rules/planning.md](../../.claude/rules/planning.md) §Gap Analysis Methodology.)_
