# IDC System — Comprehensive Study & Working Contract

_Date: 2026-06-15 · Declared version: v0.1.1 (note: `package.json`, `src-tauri/tauri.conf.json`, and `Cargo.toml` already read **0.1.2** — version drift between the release commit and the working tree is itself a finding; see §4)._

_Purpose: this is the single source of truth for taking the IDC System from its current "phase-09 complete" state to genuinely release-ready and real-life-usage-ready at a single-site Iraqi medical imaging center. It synthesizes seven parallel surveys (frontend architecture, UI/UX, Tauri/Rust, sync engine, sync server, end-to-end integration, release readiness, feature completeness). Every claim is traceable to a `path:line` from the surveys; the highest-value claims were spot-verified against source._

---

## 1. Executive Verdict

**Not shippable to a live clinic today — but close, and the gap is concentrated, not diffuse.** The application is genuinely impressive: a complete offline-first desktop app (Tauri v2 + React 19 + Rust + SQLite) with 116 registered IPC commands, all 34 required pages, a disciplined DDD layering across all three surfaces, audit-first transactional writes, immutable financial snapshots, and a bilingual RTL UI that adheres tightly to the Editorial design system. Test counts are real and large (969 frontend, ~1007 Rust integration, 271 sync-server). The architecture is sound. If this were a single-device, never-syncing kiosk, it would be close to ready.

**The honest gap between the "ship-ready" claim and reality is the seam between the three surfaces, plus production operations.** Three classes of problem block a real deployment. First, a **CRITICAL sync correctness bug**: `settings` and `visits` are declared `Manual` conflict policy at `src-tauri/src/sync/conflict/mod.rs:77`, but the pull-apply path applies them with last-write-wins gates (`puller_entities.rs:43-44`) — verified in source. A server-side row with a higher version silently overwrites unsynced local edits to settings (which drive money math) and visits. The policy registry is dead code used only in tests; nothing dispatches through it. Second, **server-side auth and persistence hazards**: `/auth/refresh` and `/auth/logout` do not bind the refresh token to the JWT subject, an HS256 dev fallback can reach a mis-tagged production deploy, and an unset `DATABASE_URL` silently boots an in-memory store that loses all data on restart. Third, **operational readiness is documented-but-unverified**: deployment secrets, VPS/nginx config, and the multi-device sync round-trip have never been exercised end-to-end; CI runs 1 of 19 E2E specs. None of these are deep architectural flaws — they are a focused set of fixes and one hard verification gate (a real two-device sync test against a real Postgres). Until the sync conflict mismatch is fixed and a true round-trip passes, the offline-first promise — "a network outage never corrupts the user's data" — is not yet true.

---

## 2. System Map

### 2.1 Three surfaces

| Surface | Where | Role | Source of truth |
|-|-|-|-|
| **Frontend** | `src/` (React 19, TS, Zustand, TanStack Query v5, i18next) | UI inside the Tauri webview. Never calls the sync server directly except via Rust. | Local SQLite (via IPC) for all reads/writes |
| **Tauri / Rust** | `src-tauri/src/` (Tokio, sqlx, 116 IPC commands, sync engine) | Local runtime, persistence, IPC, the only HTTP client to the server. | Local SQLite; owns tokens (never exposed to JS) |
| **Sync Server** | `sync-server/` (Fastify 4, Prisma, Postgres, TypeBox, RS256 JWT) | Sync push/pull, conflict parking, audit retention, reports, backups. Not a general API for the frontend. | Postgres (server-canonical for pulls) |

### 2.2 The 15 syncable entities and their declared conflict policy

Declared in `docs/idc-system/roadmap.md:100-113` and `src-tauri/src/sync/conflict/mod.rs:61-80`.

| Policy | Entities |
|-|-|
| **Last-write-wins** (by `updated_at`, tiebreak `origin_device_id` lexicographic) | `users`, `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`, `patients` |
| **Additive-only** (`INSERT OR IGNORE`, never updated) | `operator_shifts`, `inventory_adjustments`, `audit_log` |
| **Manual** (user must resolve a parked conflict) | `settings`, `visits` |

Every syncable row carries: `id` (UUID v7), `created_at`, `updated_at`, `deleted_at` (tombstone), `version`, `dirty`, `last_synced_at`, `origin_device_id`, `entity_id` (tenant). Server mirror columns: `lastSyncedAt?`, `pulledAt?`, `version`, `deletedAt?`, `originDeviceId?`.

### 2.3 Integration seams

| Seam | Mechanism | Contract artifact |
|-|-|-|
| Frontend ↔ Rust | `invoke<K>(command, args)` typed wrapper | `src/lib/ipc.ts` (CommandMap, 115+ signatures) ↔ `src-tauri/src/domains/*/commands.rs` + `lib.rs::generate_handler!` |
| Rust → Frontend (push) | Tauri events | `sync:status`, `sync:progress`, `sync:conflict`, `sync:applied`, `auth:session_expired`, `app:upgrade_required` (`src/features/sync/sync-events.ts`) |
| Rust ↔ Server (sync) | HTTP JSON | `POST /sync/push`, `GET /sync/pull?since=`, `GET /sync/conflicts`, `POST /sync/conflicts/{opId}/resolve`, `POST /sync/lookup-op` |
| Rust ↔ Server (auth) | HTTP JSON | `POST /auth/login`, `POST /auth/refresh`, `POST /auth/logout`, `POST /auth/change-password`, `GET /auth/public-key` |
| Error contract | `{code, message}` JSON → i18n key | `src-tauri/src/error.rs` (`AppError::code()`) ↔ `src/i18n/locales/en/errors.json` (`errors.codes.*`) ↔ server `DomainError` codes |
| Shape validation | Zod (client) ↔ TypeBox (server) ↔ serde (Rust) | `src/lib/schemas/*`, `sync-server/.../routes/*` TypeBox schemas |

### 2.4 How a write flows (offline → outbox → server)

```
User action (UI)
  → invoke() IPC                                    [src/lib/ipc.ts]
  → Rust command → service → AuditWriter::with_audit [audit_writer.rs]
      single tx: BEGIN
        compute before-snapshot
        business write (entity row, dirty=1)
        compute field-level delta → INSERT audit_log
        ENQUEUE outbox(audit_log) + outbox(business)  [op_id = UUID v7, payload = msgpack]
      COMMIT  ← committed locally; user is unblocked here
  ─ background SyncEngine (15s push / 30s pull) ─
  → next_batch(50) ordered (created_at, op_id)       [sqlite_outbox_repo.rs]
  → POST /sync/push  (Bearer JWT, X-Device-Id, X-App-Version)
  → server: dedupe ProcessedOp(op_id, tenant) → apply per policy → audit row
  → response {accepted, conflicts, rejected}
      accepted → mark_entities_synced + delete outbox row
      conflict → outbox.park()  → surfaces in conflict resolver UI
      transient/5xx → reschedule with exponential backoff (1–60min, 10-attempt cap)
```

Pull is the mirror: `GET /sync/pull?since=<cursor>` → apply changes + advance cursor **in one tx** (`puller.rs:85-95`) so a crash never re-pulls partially. **This is exactly where the §4 conflict-policy bug lives: the apply step uses LWW gates for every entity, ignoring the declared Manual policy.**

---

## 3. Surface-by-Surface Findings

### 3.1 Frontend Architecture

State-of-play: clean and mature. React Router v7 with `RequireRole`/`RequireAuth` guards, TanStack Query for server state, Zustand for client state, a one-shot `AuthBootstrap`, event-driven sync store, fully typed IPC wrapper. No significant tech debt; the flagged dead code is intentional forward surface. Main real risks are the absence of error boundaries and the magic-numbered Baghdad timezone offset.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| HIGH | No page-level error boundary | `src/pages/` | A thrown sync/render exception can crash the whole app instead of a recoverable screen. Admin pages mostly surface `query.error` via `<ErrorBanner>`; reception pages are inconsistent. |
| HIGH | No centralized form schemas | `src/lib/schemas/` | Form-level Zod (e.g. `CheckTypeCreateForm`) lives inline in components, not in `features/<domain>/schemas/`. Works, but a DDD/consistency gap that complicates shared client/server validation. |
| MEDIUM | Hardcoded Baghdad TZ offset | `src/stores/accounting-filters-store.ts:112-125` | `rangeAsUtc()` magic-numbers `+03:00`. Mirrors a hardcode on the Rust side (`src-tauri/src/shared/tz.rs`) and server (`reports.ts`). Single-site-safe, breaks on relocation/DST policy change. |
| MEDIUM | Untyped `auth:changed` mode payload | `src/features/auth/auth-bootstrap.tsx:66-78` | Defaults to `'online'` for any non-`'offline'` string; an unexpected Rust payload (e.g. `'locked'`) silently degrades to `'online'`. Contract is string-untyped. |
| MEDIUM | Broad query invalidation on `onSettled` | `src/features/visits/queries.ts:104-173` | Failed mutations still nuke `visitKeys.all` + inventory roots, causing needless refetch. Defensive but noisy. |
| MEDIUM | No Suspense/skeleton standard | `src/pages/` | Stale data shows until refetch completes; no shared skeleton component. |
| LOW (×5) | Toast stubbed (`toast.ts:22-33` logs to console), eager route loading, PII-stripped visit-tabs localStorage (deliberate, sound), 9 dead IPC commands (`ipc.ts:10-18`), unvalidated role literal cast (`auth-store.ts:26`). | various | All documented/intentional or polish. |

### 3.2 UI/UX

State-of-play: high-polish baseline, zero UI blockers. All 35 pages adhere to the Editorial design system (Inter + Geist Mono tnum, the token palette, status pills, KPI tiles, eyebrow rules, blink animations, focus rings). Loading/error/empty states handled; RTL complete via `RtlBoundary`. The one deliberately disabled feature ("Sign and freeze") is properly annotated. Issues are cosmetic consistency, not correctness.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| MEDIUM | `window.alert()` for tab-cap warning | `src/pages/reception/checks-grid.tsx:20`, `check-workspace.tsx:52` | Native browser alert breaks the design language. Replace with styled modal/toast. |
| MEDIUM | Error rendered as full-width `status-pill` | `dashboard.tsx:51-57`, `sync/conflicts.tsx:91-98` | `status-pill is-danger w-full justify-center` misuses an inline-badge component as an error container. Needs a dedicated `.error-banner`. |
| MEDIUM | Skeleton pattern not componentized | `dashboard.tsx:147-152`, `visits.tsx:110`, `daily-close.tsx:108` | Repeated inline `animate-pulse` divs; extract `<SkeletonLoader>`. |
| LOW (summary) | Non-localized placeholder in first-run sync URL (`first-run.tsx:126-127`); redundant `opacity-50` on disabled Sign-and-freeze; custom card hover shadow; hand-coded filter pills; 8-char device-id slice; select/textarea focus relies on `.input`. | various | Polish; none blocking. |

### 3.3 Tauri / Rust Backend

State-of-play: strong. 116 IPC commands all registered; clean domain/infrastructure/commands layering; `RwLock` state with no locks held across awaits; audit-first transaction ordering enforced by a canonical-order test; WAL SQLite with busy_timeout; 11 idempotent forward-only migrations. The one genuine non-test panic risk is unwrapped JSON serialization in the catalog event emitter.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| HIGH | `serde_json::to_string().unwrap()` in non-test path | `src-tauri/src/domains/catalog/events.rs:62,66,70,74` | Payload constructors for `PricingChangeKind` can panic on serialization failure. Should `map_err` into `AppError`. Enum-only so practically rare, but violates the explicit-error-handling rule. |
| MEDIUM | Receipt files written to disk inside the lock tx | `src-tauri/src/domains/visits/service/visit_service.rs:695` | Rendered to memory before tx (good) but atomic temp+rename happens inside; a mid-flight failure can leave temp files. Verify cleanup paths. |
| MEDIUM | Table-name allowlist for `format!` SQL | `sqlite_outbox_repo.rs:189-194` | `is_syncable_table()` gates `mark_entities_synced`. Allowlist is sound but must be audited to confirm no syncable table bypasses it. |
| MEDIUM (×2) | 50+ `#[cfg(test)]` unwraps and test modules inside domain entity files | `domains/*/domain/entities/*.rs` | No runtime risk (test-gated) but couples tests to domain layer. |
| LOW (×3) | Safe `unwrap_or_default`/`checked_div().unwrap_or(0)`/settings-cache warm-on-startup. | reports/settings | Intentional, safe. |

### 3.4 Offline-First Sync Engine (Rust)

State-of-play: architecturally complete (push/pull loops, exponential backoff, parking, atomic cursor commit) **but carries the system's single most dangerous bug.** The declared per-entity conflict policy is not what the engine actually enforces; pull-apply is uniformly LWW. This is the area with real release blockers.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| **CRITICAL** | Manual-policy entities applied with LWW on pull | `src/sync/conflict/mod.rs:77` (declares Manual) vs `src/sync/puller_entities.rs:43-44` (LWW gate) — **verified in source** | `settings` and `visits` are declared `Manual` but pull applies them via `WHERE version < excluded.version AND dirty = 0`, silently overwriting local edits when the server version is higher. Settings drive money math; visits are financial records. Manual policy must reject divergence and park, not LWW. |
| HIGH | Dirty-flag read is outside the apply tx (stale-read race) | `puller_entities.rs:15-24, 79-87, 736-745` | The local `version`/`dirty` SELECT happens before the `INSERT...ON CONFLICT`. A concurrent write marking the row dirty + enqueuing an outbox op can be clobbered because `ON CONFLICT` resets `dirty=0`. Move the read into the tx or add row locking. |
| HIGH | Conflict policy registry is not wired | `conflict/mod.rs:47-60` | The file's own comment admits `policy_for()` is used only in tests/comments; pull-apply and push-parking enforce policy inline. No client-side fallback if server conflict detection has a bug. |
| HIGH | Unreported-op hot-loop residual | `pusher.rs:151-181` | Backstop marks unreported ops as failures with backoff, but a server returning shifting result buckets across retries can still hot-loop. |
| MEDIUM (summary) | Outbox `mark_entities_synced` and `delete_acked` are two non-atomic calls (`pusher.rs:96-108`); Delete op type stubbed (Upsert-only, tombstones only); pull re-apply relies on idempotency after partial-fetch crash; metrics write after delete swallows errors; no boot-time outbox reconciliation despite `lookup_op` existing; 426 gate stops sync but auth stays reachable (stale client can still queue writes). | `pusher.rs`, `http_client.rs:169-209`, `engine.rs:401-407` | Each is defense-in-depth or a documented design trade; none individually blocking but together they harden the round trip. |
| LOW (×3) | Hardcoded `APPLY_ORDER` fails on unknown entity; on-hand recompute deliberately skips version bump (subtle); fixed 30s HTTP timeout, no adaptive backoff; no tenant scoping in outbox repo. | `puller.rs:123-256, 265-296`, `http_client.rs:11,87` | Forward-hardening. |

### 3.5 Sync Server (Fastify + Prisma + Postgres)

State-of-play: well-built (TypeBox on every route, RS256 with no constant fallback, multi-tenant composite keys, 19 Prisma models, idempotent dedupe). Not production-ready: auth-binding gaps, a reachable HS256 fallback, stubbed metrics, and an in-memory store that silently masks misconfiguration.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| **CRITICAL** | `/auth/refresh` and `/auth/logout` don't bind token to JWT subject | `sync-server/src/app/auth/routes/auth.ts:94-105, 107-118` | A leaked/intercepted refresh token is a skeleton key: anyone can rotate or force-logout any user. Fix: `WHERE id = token_hash AND user_id = jwt.sub`. |
| **CRITICAL** | No `GET /auth/profile` ground-truth endpoint | `sync-server/src/app/auth/routes/auth.ts` | Client cannot verify the server agrees on current identity; JWT claims are the only source. Add auth-guarded profile returning `{id,email,name,role,entityId}`. |
| **CRITICAL** | Unverified: ProcessedOp dedupe in conflict-resolve handler | `sync-server/src/app/sync/routes/conflicts.ts` (schema confirms `resolve_op_id` + `409 ALREADY_RESOLVED` at lines 15, 94, 117) | Schema declares idempotency; the handler's ProcessedOp short-circuit must be confirmed present or a duplicate resolve double-applies. Spot-check shows the contract is wired in the route schema; verify the service path. |
| HIGH | HS256 dev fallback reachable in non-`production` `NODE_ENV` | `sync-server/src/app/plugins/auth-jwt.ts:32-60`; `.env.template:42` | `isProd` keys off `NODE_ENV==='production'` exactly. A `staging`/unset deploy with `JWT_SECRET` present silently signs HS256 while the public key is served publicly → token forgery. Fail loud unless both RS256 keys present outside dev. |
| HIGH | In-memory store fallback on unset `DATABASE_URL` | `sync-server/src/app/plugins/prisma.ts`; `.env.template:10-24` | A prod deploy missing `DATABASE_URL` boots, accepts pushes, loses everything on restart. Make `DATABASE_URL` required when `NODE_ENV=production`. |
| HIGH | `healthz` hardcodes `redisOk='ok'` | `sync-server/src/app/routes/healthz.ts:81` | Masks Redis misconfiguration if Redis becomes mandatory. |
| HIGH | Metrics defined but never invoked | `plugins/metrics.ts`; `routes/push.ts`, `pull.ts` | `/metrics` always returns zeros; no handler calls `observe*()`. |
| HIGH | Conflict-resolve merged payload not re-validated | `sync-server/src/app/sync/routes/conflicts.ts:79-125` | Accepts client `merged` without the push-service validation; can persist invalid state. |
| HIGH | Reports may omit per-entity inventory cost | `reports.ts:118-155` | `total_inventory_consumption_value_iqd` present but per-doctor/operator/check breakdown may not compute it → reconciliation mismatch. |
| MEDIUM (summary) | Cursor decode lacks try/catch → 500 not 422 (`audit-repo.ts:149-160`); `X-Device-Id` unvalidated; locked-visit snapshot validation relies on DB CHECK (500 not 422); adjustment delta-sign relies on CHECK; no central conflict-policy registry server-side; nullable `?? null` consistency. | various | Robustness + contract clarity. |
| LOW (summary) | `compareVersions` ignores semver pre-release; CORS allows localhost in any `NODE_ENV`; `details` optionality in error schema; audit text search is substring (no FTS). | various | |

### 3.6 End-to-End Integration

State-of-play: the contracts are comprehensively declared and most declared gaps are genuinely closed (verified spot-checks on IPC bindings, snapshot columns, back-relations, audit enums). The residual risk is verification debt: round-trip field completeness for all 15 entities is spot-checked, not exhaustively mapped, and a few server handlers were not inspected.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| **CRITICAL** | Unverified ProcessedOp dedupe in resolve route | `sync-server/src/app/sync/routes/conflicts.ts` | Same as §3.5; the contract is the linchpin of resolve idempotency. |
| HIGH | Tenant scoping is manual per-repo, not a Prisma extension | `sync-server/src/app/plugins/tenant.ts`, `sync-services.ts:42-51` | `status.md` implies an auto-injecting Prisma extension; reality is repos must manually `WHERE entityIdTenant`. One missed filter = cross-tenant leak. Audit every repo query. |
| HIGH | Operator/User shift back-relations: code vs status.md ambiguity | `schema.prisma:103-104, 287, 382-384` | Fields are present (fix applied) but status.md (2026-05-13) describes the pre-fix state; reconcile docs to remove "would have failed" ambiguity. |
| MEDIUM (summary) | Round-trip field completeness (15 entities × 8 sync columns) not exhaustively mapped; IPC inner-struct snake/camel non-conversion footgun (`ipc.ts:26-27`); resolver mid-flight idempotency race not fully traced; `healthz` returns 200 even on `status:'fail'`; visit snapshot enforcement at pull-emit unverified. | various | Verification debt, not known breakage. |
| LOW (summary) | 9 dead-code IPC commands; Prisma Visit CHECK constraints live only in `init-custom-sql.sql` (schema doc gap); raw-SQL migration ordering implicit; `CONFLICT_PARKED` code reused for constraint errors; JWT-pin SHA256 not format-validated. | various | |

### 3.7 Release & Operational Readiness

State-of-play: the CI/release machinery is well-designed (atomic version bump across three files, signed minisign bundles, zero-downtime atomic `latest.json` flip, real updater host `idc-release.madebyhaithem.com`). But real-world deploy is unproven: secrets/VPS/nginx are "assumed," E2E is 1/19 specs, and prod env validation is weak.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| HIGH | HS256 reachable in production-tagged deploy | `sync-server/.env.template:42` | (Cross-ref §3.5.) No boot-time "RS256-only in production" enforcement. |
| HIGH | Silent data loss on unset `DATABASE_URL` | `.env.template:10-24` | (Cross-ref §3.5.) |
| HIGH | E2E automation is 1 of 19 specs | `e2e/` | CI runs only `app-shell` smoke; 16 specs gated by `RUN_FULL_E2E=true` (manual binary rebuild + `clinical-day.sql` seed), 2 multi-device gated by `MULTI_DEVICE=true` and never run. Lock/void/conflict workflows have no automated E2E. |
| MEDIUM (summary) | Deploy secrets unverified (no `verify-release-secrets.sh`); nginx config not version-controlled/IaC; no post-deploy `latest.json` health check; `prisma db push --accept-data-loss` used instead of `migrate deploy` with no backup-before-migrate; open defects are deferred features mislabeled "open." | `.github/workflows/release.yml`, `Dockerfile.dev:26`, `docs/UPDATER-SETUP.md` | Ops hardening + pre-flight gates. |
| LOW (summary) | RTL visual regression is manual; domain-rule acceptance tests (inventory-never-negative, daily-close totals) absent; stale `DEPLOY-HANDOFF.md`; two release paths documented with equal weight. | `docs/idc-system/testing/` | |

### 3.8 Feature Completeness

State-of-play: functionally complete for the core lifecycle — reception (check-in, lock with atomic snapshots, receipt, void), inventory (consume-on-lock, adjustments, recompute), shifts, accounting (KPIs, drill-downs, daily close, CSV/PDF), admin CRUD, audit, conflict resolution. Gaps are scope decisions and one schema-versioning hole, not missing core.

| Severity | Finding | Location | Detail |
|-|-|-|-|
| HIGH | No sync schema-version negotiation | `src-tauri/src/sync/engine.rs`, `sync-server/src/app/sync/routes/` | Only `app_version` (426) is compared. If server migrations add required fields, old clients send payloads missing them with no protocol-version guard → silent loss. Add `schema_version` to push header / pull response. |
| MEDIUM (summary) | No patient visit-history/dedupe (Horizon-1, free-form name only); sync persistence not exercised in a staging round trip; daily-close "provisional" export not blocked when pushes pending; conflict resolution has no "retry now" (waits for next cycle); no patient consent/privacy workflow; hardcoded Baghdad TZ. | various | Mix of deferred scope and verification. |
| LOW (summary) | "Sign and freeze" disabled (v0.2); thermal printer writes to disk only (no device integration); no printer enumeration UI; no backup/restore UI; no rate-limit on lock IPC; parked outbox ops need manual recovery; no in-app data-retention notice. | various | Mostly Horizon-1. |

---

## 4. Integration Contract Mismatches

The highest-value section. Each mismatch cites **both sides**.

### 4.1 Conflict policy: declared Manual vs applied LWW (CRITICAL — verified)
- **Declared:** `src-tauri/src/sync/conflict/mod.rs:77` — `"settings" | "visits" => Policy::Manual`; roadmap matrix `roadmap.md:100-113`.
- **Enforced:** `src-tauri/src/sync/puller_entities.rs:43-44` — `WHERE settings.version < excluded.version AND settings.dirty = 0` (and `:736-745` for visits). LWW gate, not manual parking.
- **Gap:** the policy registry (`policy_for()`) is referenced only by tests (`mod.rs:89-120`); no production code dispatches through it (admitted in `mod.rs:47-60`). Server-side manual parking is the only thing that makes Manual work, and the client has no fallback. **Fix:** route pull-apply through `policy_for()`; for Manual entities, reject version divergence and park locally rather than LWW-overwrite.

### 4.2 Auth refresh/logout not bound to subject (CRITICAL)
- **Client side:** Rust reads the cached `refresh_token` from `AppState` and POSTs it (`src-tauri/src/domains/auth/commands.rs:223-250`).
- **Server side:** `sync-server/src/app/auth/routes/auth.ts:94-118` validates only that the token hash exists, not that `refresh_tokens.user_id == jwt.sub`. **Fix:** add the `user_id` predicate; reject mismatches with 401.

### 4.3 Missing server identity endpoint (CRITICAL)
- **Client expects:** `auth_current_user` / profile semantics surfaced via `useCurrentUser` (`src/features/auth/queries.ts`).
- **Server provides:** login/refresh/logout/change-password only (`auth.ts`). No `GET /auth/profile`. **Fix:** add it, matched against `users` by `jwt.sub`.

### 4.4 HS256/RS256 selection vs deploy env (HIGH)
- **Server selection:** `auth-jwt.ts:32-60` requires RS256 only when `NODE_ENV==='production'`; otherwise HS256 with `JWT_SECRET`.
- **Public key exposure:** `GET /auth/public-key` serves the key publicly; the Rust client pins its SHA256 (`auth_bootstrap_jwt_key`). **Mismatch:** a non-`production` `NODE_ENV` deploy signs HS256 while clients verify against a pinned RS256 public key — verification will fail, or worse, an HS256-mode server lets a public-key holder forge symmetric tokens. **Fix:** enforce RS256 whenever not explicitly dev.

### 4.5 IPC inner-struct case normalization (MEDIUM)
- **Wrapper assumption:** `src/lib/ipc.ts:26-27` — Tauri auto-converts top-level `op_id`→`opId`, but **not nested struct fields**.
- **Hand-reshaped command:** `sync_resolve_conflict` manually reshapes nested `{opId, choice, merged}` in Rust (`ipc.ts:34`). A future nested-arg command that forgets this breaks the Zod contract silently. **Fix:** document/enforce; consider a serde rename layer for inner structs.

### 4.6 SQLite ↔ Prisma constraint drift (LOW/doc)
- **Local:** Visit snapshot CHECK + adjustment delta-sign CHECK live in `src-tauri/migrations/005_patients_visits_adjustments.sql`.
- **Server:** Mirrored in `sync-server/prisma/init-custom-sql.sql` (lines 49-76), **not** in `schema.prisma` (which only declares the columns at `:433-439`). The Prisma DSL omits constraints that exist in the DB — a documentation/visibility gap, not a runtime drift. **Fix:** add comments in `schema.prisma` pointing to the raw-SQL file, and document `init-custom-sql.sql` section ordering (functions → triggers, partial → unique indexes).

### 4.7 i18n error contract coverage (LOW)
- **Rust:** 12 `AppError` variants, `error.rs::code()` → `{code, message}`.
- **Frontend:** `errors.codes.*` in `src/i18n/locales/en/errors.json` (and `ar`), parsed by `src/lib/errors.ts::formatIpcError`.
- **Server:** ~40+ `DomainError` throws using the same code strings, not exhaustively cross-audited against the 12 declared codes. Also `CONFLICT_PARKED` is reused for plain DB constraint violations (semantic over-broadening, `error.rs:67`). **Fix:** audit all server codes against the client registry; split constraint errors from sync-parked.

---

## 5. Release Blockers (Prioritized)

Deduplicated across all surveys. CRITICAL first.

| Rank | Severity | Blocker | Surface | Why it blocks real-life usage |
|-|-|-|-|-|
| 1 | CRITICAL | Manual-policy entities (`settings`, `visits`) applied with LWW on pull | Sync Engine | Silent overwrite of unsynced local edits to money-driving settings and financial visit records. Breaks the core offline-first promise. (`conflict/mod.rs:77` vs `puller_entities.rs:43-44`) |
| 2 | CRITICAL | `/auth/refresh` & `/auth/logout` not bound to JWT subject | Sync Server | A leaked refresh token can hijack or log out any user. Auth security hole. (`auth.ts:94-118`) |
| 3 | CRITICAL | Confirm ProcessedOp dedupe in `POST /sync/conflicts/{opId}/resolve` | Sync Server / Integration | If absent, a retried resolve double-applies a conflict resolution, corrupting state. (`conflicts.ts`) |
| 4 | CRITICAL | Missing `GET /auth/profile` ground truth | Sync Server | Client cannot verify server-side identity; locally tampered JWT has no server cross-check. (`auth.ts`) |
| 5 | HIGH | Dirty-flag stale-read race in pull-apply | Sync Engine | A pull can clobber a mid-flight local mutation. Data loss under concurrency. (`puller_entities.rs:15-24`) |
| 6 | HIGH | HS256 fallback reachable in mis-tagged prod deploy | Sync Server / Ops | Public key holder can forge tokens. (`auth-jwt.ts:32-60`) |
| 7 | HIGH | Unset `DATABASE_URL` boots silent in-memory store | Sync Server / Ops | A misconfigured prod loses all data on restart. (`prisma.ts`, `.env.template:10-24`) |
| 8 | HIGH | No sync schema-version negotiation | Sync Engine / Features | Server migration adding required fields → old clients silently drop data. (`engine.rs`) |
| 9 | HIGH | Multi-device sync round-trip never run end-to-end against real Postgres | Integration / Ops | Offline-create → reconnect → second device pulls is the product's reason to exist; unverified. |
| 10 | HIGH | Conflict-resolve `merged` payload not re-validated server-side | Sync Server | Invalid merged state can be persisted. (`conflicts.ts:79-125`) |
| 11 | HIGH | No page-level error boundary | Frontend | One thrown exception crashes the whole clinic terminal. (`src/pages/`) |
| 12 | HIGH | `serde_json` `.unwrap()` in catalog event emitter | Tauri/Rust | Panic risk on a non-test path. (`catalog/events.rs:62-74`) |
| 13 | HIGH | E2E automation 1/19; multi-device specs never run in CI | Release | Critical workflows ship untested per change. (`e2e/`) |
| 14 | HIGH | Tenant scoping is manual per-repo, not enforced centrally | Integration | One missed `WHERE entityIdTenant` = cross-tenant leak. (`tenant.ts`) |
| 15 | HIGH | Reports/daily-close per-entity inventory cost may be incomplete | Sync Server | Financial reconciliation can mismatch. (`reports.ts:118-155`) |
| 16 | MEDIUM | `prisma db push --accept-data-loss` in deploy, no backup-before-migrate | Ops | A bad schema overwrites prod history with no rollback. (`Dockerfile.dev:26`) |
| 17 | MEDIUM | Deploy secrets / VPS / nginx unverified; no pre-flight & post-deploy health check | Ops | First real release can fail opaquely. (`release.yml`, `UPDATER-SETUP.md`) |
| 18 | MEDIUM | Provisional daily-close export not blocked when pushes pending | Features/UI | Accountant exports numbers that change after sync. (`daily-close.tsx:125-134`) |
| 19 | MEDIUM | `window.alert()` + full-width status-pill errors | UI/UX | Off-brand, jarring in a clinical tool. |
| 20 | MEDIUM | Patient consent/privacy workflow + data-retention notice (legal review) | Features | May be legally required before capturing patient data in-jurisdiction. |

---

## 6. Workstreams to Release-Ready

### WS-1 — Sync round-trip correctness (the gating workstream)
**Goal:** make the declared offline-first/conflict contract actually true and prove it with a real two-device round trip.
**Covers:** "Conflict Policy Mismatch: Manual Entities Applied with LWW" (#1), "Pull-Apply LWW Bypasses Dirty Flag Guard" (#5), "Conflict Policy Enforcement Not Wired Client-Side", "Unreported Ops Hot-Loop", "No Outbox Reconciliation on Boot", "Missing schema version negotiation" (#8), "Sync server persistence not fully tested" (#9), "ProcessedOp dedup in resolve endpoint" (#3), "merged payload not re-validated" (#10).
**Sequence:** (1) wire `policy_for()` as the single dispatch site in `puller_entities`/`pusher`; Manual → park, never LWW. (2) Move the dirty/version read into the apply tx. (3) Add `schema_version` to push header + pull response with a hard reject. (4) Confirm/implement ProcessedOp short-circuit + server-side `merged` validation. (5) Wire boot-time `lookup_op` reconciliation. (6) **Gate:** stand up Postgres + run the `MULTI_DEVICE=true` E2E suite to green. Depends on WS-3 (a real DB) and WS-4 (CI automation).

### WS-2 — Auth & security
**Goal:** close server-side auth holes and remove the symmetric-key footgun.
**Covers:** "Auth bypass on refresh/logout" (#2), "Missing /auth/profile" (#4), "HS256 fallback reachable in production" (#6).
**Sequence:** bind refresh/logout to `jwt.sub`; add `GET /auth/profile`; add boot-time assertion that production requires both RS256 keys and rejects HS256. Independent; can run parallel to WS-1.

### WS-3 — Sync-server persistence & data safety
**Goal:** no silent data loss; safe migrations.
**Covers:** "In-memory fallback risks silent data loss" (#7), "prisma db push vs migrate deploy" (#16), "healthz hardcodes redis ok", "metrics not instrumented", "cursor decode 500 not 422", "tenant scoping manual" (#14).
**Sequence:** make `DATABASE_URL` required in production via `@fastify/env`; switch to `prisma migrate deploy` + pre-deploy `pg_dump`; instrument metrics; wrap cursor decode in try/catch (422); audit every Prisma repo for the tenant `WHERE`. Prerequisite for WS-1's round-trip gate.

### WS-4 — Release / ops & updater hardening
**Goal:** a first real release that fails loud, not silent.
**Covers:** "Deployment secrets assumed not verified", "VPS/nginx not version-controlled", "No production env validation test", "No post-deploy health check", "E2E automation minimal" (#13).
**Sequence:** `tools/verify-release-secrets.sh` + a `workflow_dispatch` secrets-validation job; version-control nginx (or IaC); add post-deploy `curl latest.json` + version assert; automate E2E binary rebuild + seed and move `RUN_FULL_E2E` into the default pipeline (nightly for multi-device). Feeds WS-1's verification gate.

### WS-5 — Frontend resilience & UI/UX polish
**Goal:** no whole-app crashes; on-brand error/empty/loading.
**Covers:** "No page-level error boundary" (#11), "window.alert() tab warning" (#19), "Error as full-width status-pill", "Skeleton not standardized", "Provisional daily-close export not blocked" (#18), "Untyped auth:changed mode", "broad query invalidation".
**Sequence:** add a route-level `ErrorBoundary`; replace `window.alert`; extract `<ErrorBanner>`/`<SkeletonLoader>`; block/confirm provisional close export; type the `auth:changed` payload. Independent.

### WS-6 — Rust backend hardening
**Goal:** remove panic paths, tighten invariants.
**Covers:** "Unwrapped serde_json in catalog/events.rs" (#12), "Receipt render inside tx temp-file cleanup", "Table-name allowlist audit".
**Sequence:** replace the catalog `.unwrap()`s with `map_err`; verify receipt temp-file cleanup; audit `is_syncable_table`. Independent; small.

### WS-7 — Test coverage (domain + E2E)
**Goal:** prove the money and inventory math, and the critical flows, automatically.
**Covers:** "Domain validation tests missing", "E2E coverage minimal" (#13), "RTL visual regression manual", round-trip field-completeness mapping.
**Sequence:** add domain acceptance tests (inventory never negative, void audit trail, daily-close grand total = sum of visits); add an exhaustive 15-entity × 8-column round-trip map test; add RTL visual-regression. Overlaps WS-4.

### WS-8 — Feature gaps & legal (product-gated)
**Goal:** decide and close the Horizon-1 items that affect real clinic use.
**Covers:** "Patient consent/privacy workflow" (#20), "No patient visit history/dedupe", "Thermal printer device integration", "Timezone configurable", "Conflict resolve retry-now", "Parked-op recovery UI", "Sign and freeze (v0.2)".
**Sequence:** driven by §8 answers. Legal review for consent gates this from being a hard blocker vs deferred.

**Dependency summary:** WS-3 → WS-1 (round-trip needs a real DB); WS-4 → WS-1 (gate needs automated E2E). WS-2, WS-5, WS-6 are independent and parallelizable. WS-7 overlaps WS-1/WS-4. WS-8 is product-gated.

---

## 7. Working Agreement (the Contract)

Standing rules for all future work in this effort, derived from `CLAUDE.md` and `.claude/rules/`. These are non-negotiable.

1. **Offline-first invariant.** Every read goes to local SQLite; every write commits locally first (audit-first tx ordering via `AuditWriter::with_audit`); the background engine ships changes later. A network outage NEVER breaks or corrupts the user's workflow. No transaction is held across an HTTP call.
2. **Conflict policy is declared and enforced.** Every syncable entity declares its policy in its phase file, and the engine must dispatch through that declaration (the fix in WS-1). No silent LWW on Manual entities.
3. **Context7 first (MANDATORY).** Before writing any code touching a library/plugin/framework — Tauri plugins, Tokio, sqlx, Axum, Fastify, Prisma, TypeBox, jsonwebtoken, React 19, React Router v7, TanStack Query, Zustand, Zod, framer-motion, react-i18next, shadcn — call `resolve-library-id` then `query-docs` and build on the returned docs. Never from memory.
4. **No Claude authorship in commits.** No `Co-Authored-By: Claude`, no Anthropic emails, no `git config` changes. All commits appear solely human-made. (Overrides any default harness behavior.)
5. **No emojis.** Never in code, comments, docs, commit messages, or user-facing strings.
6. **pnpm only.** Never `npm`/`yarn`. Never hand-edit `package.json` deps (use `pnpm add/remove`) or `Cargo.toml` `[dependencies]` (use `cargo add`).
7. **Pre-push validation gate (MANDATORY).** Before any push: `pnpm lint`, `pnpm build`, `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`, plus the sync-server's lint/typecheck/tests. Mirror `tools/pre-push-check.sh` + `tools/preship-guardrails.sh`. Never `--no-verify`.
8. **DDD layering everywhere.** Domain (pure entities/services/interfaces) → infrastructure (sqlx/Prisma repos, adapters) → presentation (IPC commands registered in `lib.rs`, Fastify routes with full Swagger/TypeBox, React pages/queries). Keep boundaries clean.
9. **Schema discipline.** Local SQLite migrations and Prisma models are forward-only and idempotent and must change together for any syncable entity. A server-side syncable change without the matching local migration (or vice-versa) is a defect.
10. **status.md upkeep (MANDATORY).** Before committing a phase: flip the phase row, refresh Cumulative Totals (tables/models/IPC/routes/pages/policies/locales), and append a completion note. Never let `docs/idc-system/status.md` drift behind the code.
11. **No destructive actions.** No `docker rm/prune`, no `git push --force` to `main`, no `git reset --hard` on shared branches, no `git branch -D`, no `--no-gpg-sign`.
12. **Subagent rule.** When launching subagents, paste the relevant rule content directly (subagents don't auto-load `.claude/rules/`), always including "no Claude authorship" and "Context7 first."

---

## 8. Open Questions for the User

1. **Launch shape: single-site only, or multi-device from day one?** The CRITICAL sync conflict bug (#1) and tenant-scoping audit (#14) matter far more if two devices sync concurrently. If launch is single-device, WS-1 can be de-risked (still must fix, but the round-trip gate is simpler).
2. **Target launch date / risk appetite?** This sets how much of WS-7/WS-8 we cut. A hard date argues for: fix CRITICAL/HIGH (WS-1, WS-2, WS-3), run one real round-trip, defer polish and Horizon-1.
3. **Patient consent / privacy: is legal review required before go-live?** This is the only blocker we cannot resolve in code (#20). If Iraqi data-protection counsel requires a consent screen and retention notice, that gates launch; if not, it defers to Horizon-1.
4. **Priority order: sync correctness vs ops hardening vs UI polish?** Our recommendation is WS-1/WS-2/WS-3 first (correctness + security + data safety), then WS-4 (ops), then WS-5/WS-6 (resilience), with WS-7/WS-8 as parallel/deferred. Confirm or reweight.
5. **Provisional daily-close: block export when pushes are pending, or warn-only?** A financial-correctness call only the accountant/owner should make (#18).
6. **Thermal printer integration depth for v1:** is "save receipt text to a watched folder" acceptable for go-live, or must we ship direct device printing? Affects WS-8 scope and the deployment runbook.
7. **Timezone:** stay hardcoded Baghdad `+03:00` for v1 (simplest, single-site-correct), or make it a setting now? Affects frontend store, Rust `tz.rs`, and server reports.
8. **Version reconciliation:** the working tree is `0.1.2` while this study and the last release commit say `0.1.1`. Confirm the intended current version before the next release tag, so `pnpm release` does not produce drift.
