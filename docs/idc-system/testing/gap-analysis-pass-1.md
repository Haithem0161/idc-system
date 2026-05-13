# Gap Analysis Pass 1 -- Test Plans vs Build Specs

_Date: 2026-05-13_

Compares every `docs/idc-system/phase-XX.md` build spec against the matching `docs/idc-system/testing/phase-XX-test.md` test plan. A gap is a scenario, command, route, schema rule, sync contract, conflict policy, or edge case that the build spec promises but the test plan does not verify. Methodology per `.claude/rules/planning.md` Gap Analysis Methodology (Pass 1) applied to the testing surface defined in `.claude/rules/testing.md` §3-§6.

Cross-cutting items legitimately delegated to `security.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`, or another phase plan (and listed in the owning test plan header's `Out of scope` line) are NOT counted.

## Pass 1 Totals

| Phase | Total | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing | 18 | 2 | 6 | 7 | 3 |
| 02 Authentication & Users | 12 | 0 | 4 | 5 | 3 |
| 03 Catalog & Reference Data | 15 | 0 | 4 | 8 | 3 |
| 04 Operator Shifts | 9 | 0 | 3 | 4 | 2 |
| 05 Reception & Visit Lock | 11 | 1 | 3 | 5 | 2 |
| 06 Inventory Operations | 7 | 1 | 2 | 3 | 1 |
| 07 Accounting & Reports | 12 | 2 | 4 | 4 | 2 |
| 08 Audit, Conflict Resolver & Polish | 14 | 2 | 5 | 5 | 2 |
| 09 Pre-Ship Hardening | 14 | 4 | 4 | 3 | 3 |
| **Total** | **112** | **12** | **35** | **44** | **21** |

Severity rubric per phase prompt: CRITICAL = missing test of a safety/correctness invariant; HIGH = missing test of a major user-facing flow or business rule; MEDIUM = missing test of an edge case the build spec calls out; LOW = cosmetic, advisory, or a coverage-gate / snapshot listing omission.

## Critical Gaps (12)

| ID | Phase | Build spec ref | Gap |
|-|-|-|-|
| P01-G01 | 01 | "Proves" + §6.7 | JWT public-key pinning at boot only asserts "function signature exists" -- no behavioural test of bootstrap fetch+pin, refuse-on-mismatch, or `--reset-jwt-pin` override. |
| P01-G02 | 01 | §3 Tauri capabilities | No test asserts `capabilities/default.json` lacks `http:default` and declares `store/stronghold/os/path/dialog/log`. |
| P05-G01 | 05 | §7.11 | No test asserts void writes audit rows BEFORE the visits update / offset inserts / item recomputes (lock has the audit-first test; void mirror is absent). |
| P06-G01 | 06 | §7.11 | `create_adjustment_writes_audit_first_then_business_then_outbox` asserts ordering, but NOT that the audit row carries the explicit `delta: { before, after, reason }` payload shape §7.11 mandates. |
| P07-G01 | 07 | §7.18 | `audit_log.delta` payload contents (input_hash, generated_at, total_revenue_iqd, locked_count, voided_count, pending_sync_count, provisional) not asserted on the `daily_close_run` row. |
| P07-G02 | 07 | §7.19 + §7.23 | No assertion PDF filename embeds `daily-close_<targetDate>_<inputHashPrefix>.pdf` AND that re-runs with new locks produce a NEW file (no overwrite). |
| P08-G01 | 08 | §7.22 + §3 server resolve | No assertion server `POST /sync/conflicts/:opId/resolve` short-circuits via ProcessedOp idempotency on duplicate `resolve_op_id` (returns prior cached body exactly). |
| P08-G02 | 08 | §7.16 | No assertion that the soak harness writes `target/soak-report.md` with all 6 quantitative criteria captured. |
| P09-G01 | 09 | §3 sync-services rewrite | No test asserts autoload dependency ordering -- `prisma.ts` MUST initialize before `auth-services` / `sync-services` via `fp(..., { dependencies: ['prisma'] })`. |
| P09-G02 | 09 | §3 prisma.ts plugin | No test asserts `prisma.$disconnect` runs on Fastify `onClose`; restart tests kill the process and miss the graceful path. |
| P09-G03 | 09 | §6 step 7 | Defining persistence E2E lists doctor + visit + audit but never asserts an OPEN REFRESH TOKEN survives restart and can refresh post-restart. |
| P09-G04 | 09 | §3 ConflictParkedRepository | No test asserts the `resolve` vs `resolveTx` API split -- ConflictResolveService must always use `resolveTx` inside `prisma.$transaction`. |

## Phase 01 Gaps (18)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P01-G01 | CRITICAL | Incomplete Coverage | "Proves" + §6.7 | §6.7 / §2.1 | JWT public-key pinning at boot only asserts "function signature exists" -- no behavioural test of bootstrap fetch+pin, refuse-on-mismatch, or `--reset-jwt-pin` override. |
| P01-G02 | CRITICAL | Missing Integration Test | §3 Tauri capabilities | §2.1 / §6.7 | No test asserts `capabilities/default.json` lacks `http:default` and declares `store/stronghold/os/path/dialog/log`. |
| P01-G03 | HIGH | Missing Integration Test | §7.1 tauri-plugin-fs + fs:scope | §2.1 / §6.8 | Plugin registration and log fs-scope capability never asserted; receipts/log writes will silently break in shipped builds. |
| P01-G04 | HIGH | Missing Edge Coverage | §4 SyncEngine emits `sync:progress` | §2.4 / §4.1 | `sync:progress { pushed, total }` event emission during drain has no test row anywhere. |
| P01-G05 | HIGH | Missing Edge Coverage | §4 SyncPushService step 5 LWW tiebreak | §2.3 / §6.4 | LWW `(version =, updated_at =)` tiebreak via `originDeviceId` lex ordering never exercised. |
| P01-G06 | HIGH | Missing Integration Test | §4 SyncPushService step 1.ii | §2.3 | No test posts a non-`audit_log` entity to `/sync/push` and asserts 422 unknown-entity rejection. |
| P01-G07 | HIGH | Missing Contract Test | §7.19 SyncCursor compound `@@id` | §2.3 / §3.3 | Compound PK on `SyncCursor` never schema-asserted; regression to single-key `@id` would break tenant-scoped cursors. |
| P01-G08 | HIGH | Missing Performance SLO | §4 SyncPullService step 2 | §7 | 500-row per-entity cap on pull aggregation has no test. |
| P01-G09 | MEDIUM | Missing Integration Test | §7.33 AuditLog server indexes | §2.3 | Server-side `@@index([entityIdTenant, at(sort: Desc)])` not asserted via Prisma migration / EXPLAIN. |
| P01-G10 | MEDIUM | Missing Coverage Gate | §7.11 `pnpm a11y` axe-core | §1.3 / §8 | a11y CI script not wired into DoD checklist or coverage table; only manual NVDA step. |
| P01-G11 | MEDIUM | Missing Integration Test | §7.18 ProcessedOp daily vacuum | §2.3 / §6.8 | Behavioural assertions missing: job runs at 03:30, exactly one `audit_log` vacuum row per run, idempotent re-run. |
| P01-G12 | MEDIUM | Missing Integration Test | §7.3 store carries only UI prefs | §2.1 | No test pins that cursor writes go only to SQLite (no double-write to plugin-store). |
| P01-G13 | MEDIUM | Missing Integration Test | §4 manual policy parks on server | §2.3 | Server-side parking-on-push for `manual` policy entity has no test; only local engine parking on conflict response. |
| P01-G14 | MEDIUM | Missing Integration Test | §3 `createMany skipDuplicates` | §2.3 | Duplicate-skip idempotency on `audit_log` `createMany` never directly tested with known-duplicate `id`. |
| P01-G15 | MEDIUM | Missing Edge Coverage | §4 with_audit step 6 | §2.1 | Ordering asserted, but no assertion that BOTH outbox rows (business + audit) are enqueued in same tx. |
| P01-G16 | LOW | Missing Coverage Gate | §7.29 Husky + lint-staged | §8 | `.husky/pre-commit` running `pnpm lint-staged` not validated. |
| P01-G17 | LOW | Missing Integration Test | §7.23 no `notification:default` | §2.1 / §6.7 | Capability lint should explicitly assert absence of `notification:default`. |
| P01-G18 | LOW | Missing Edge Coverage | §7.25 focus-visible overrides | §2.4 | Automated assertion of `focus-visible` classes on shadcn overrides absent. |

## Phase 02 Gaps (12)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P02-G01 | HIGH | Missing Integration Test | §4 `AuthService::refresh` step 2 | §2.1 auth_phase02.rs | `auth:refreshed` event emission on successful `/auth/refresh` 200 is never asserted (only 401 branch is). |
| P02-G02 | HIGH | Missing Contract Test | §7.18 audit action TypeBox union | §3.1 | Server TypeBox literal-union enforcement on `/sync/push` audit rows not validated; only SQLite enforcement covered. |
| P02-G03 | HIGH | Missing Unit Test | §3 Frontend hooks `useUserUpdate` | §2.4 | `useUserUpdate` mutation has no row (create/softDelete/resetPassword tested; update omitted). |
| P02-G04 | HIGH | Missing Integration Test | §3 `RefreshToken.deviceId` + §4 login step 4 | §2.3 | `deviceId` persistence and round-trip through `/auth/login` and `/auth/refresh` never asserted. |
| P02-G05 | MEDIUM | Missing Integration Test | §4 login step 4 "30-day lifetime" | §2.3 | Refresh-token TTL (`expiresAt - createdAt == 30d`) not asserted; only access-token 15-min is. |
| P02-G06 | MEDIUM | Missing Integration Test | §7.14 atomic UserContext+settings_cache replace | §2.1 | Atomic single-write-lock replacement on re-login never asserted. |
| P02-G07 | MEDIUM | Missing Coverage Gate | §7.30 IQD grep | §6.2 / §8 DoD | The hard-coded IQD grep not wired as CI check or §8 checkbox. |
| P02-G08 | MEDIUM | Missing Contract Test | §5 `jsonwebtoken` client-side RS256 | §3.2 / §2.1 | Client-side RS256 JWT signature verification using pinned stronghold key never asserted. |
| P02-G09 | MEDIUM | Missing Integration Test | §3 `auth::current_user` cache-bust | §2.4 | `useCurrentUser` not asserted to update when `auth:refreshed` / `settings:changed` events fire. |
| P02-G10 | LOW | Missing E2E Scenario | §4 `<IdleWatcher>` touchstart | §4 / §6.3 | `touchstart` activity reset never exercised in an E2E touch-input spec. |
| P02-G11 | LOW | Missing Persona / Manual Step | §7.13 `<UserMenu>` red dot | §5.1 | Visual review of the red-dot threshold not in §5.1 manual script list. |
| P02-G12 | LOW | Missing Snapshot | §3 `LoginResponseSchema` | §3.3 / §8 | No canonical snapshot of the login response envelope committed. |

## Phase 03 Gaps (15)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P03-G01 | HIGH | Missing Contract Test | §7.30 Prisma back-relations | §3.1 | No assertion `pnpm prisma validate` succeeds with required back-relations on CheckType/CheckSubtype/Doctor/Operator. |
| P03-G02 | HIGH | Missing Integration Test | §7.27 emit coverage | §2.1 | `catalog:pricing_changed` emit not asserted for `CheckTypeService::update`, `CheckSubtypeService::create|update|soft_delete`, `DoctorPricingService::soft_delete`. |
| P03-G03 | HIGH | Missing Integration Test | §7.27 emit-after-audit ordering | §2.1 | Ordering invariant "emit fires only AFTER with_audit commits" not asserted (crash mid-commit must not emit). |
| P03-G04 | HIGH | Missing Integration Test | §7.18 audit delta payload | §2.1 | Audit row before/after JSON delta capture not asserted; ordering checked only. |
| P03-G05 | MEDIUM | Missing Integration Test | §7.23 `<DoctorAutocomplete>` include_id | §2.2 / §2.4 | `doctors::list({active_only, include_id})` "include current draft inactive doctor" branch uncovered. |
| P03-G06 | MEDIUM | Missing Integration Test | §7.8 step 2 informational warning | §2.1 | Informational warning path on `inventory_items::soft_delete` (adjustments>0 within 90d, does NOT block) not tested. |
| P03-G07 | MEDIUM | Missing Integration Test | §7.15 LIKE-prefix query | §2.2 | `check_types::list` and `inventory_catalog::list` LIKE-prefix search not exercised; only min-2-chars validation. |
| P03-G08 | MEDIUM | Missing Integration Test | §7.3 step 5 outbox enqueue | §2.1 | "Enqueue outbox row" step on every catalog create/update mutation not asserted per service. |
| P03-G09 | MEDIUM | Missing Contract Test | §5 TENANT_MODELS | §3.3 | No test that server `TENANT_MODELS` contains the 8 new catalog table names. |
| P03-G10 | MEDIUM | Missing Coverage Gate | §7.29 + §7.32 i18n error keys | §1.3 / §6.2 | Phase-03 error key inventory (`errors:catalog.*`, `errors:consumption.*`, etc.) existence on en+ar not asserted. |
| P03-G11 | MEDIUM | Missing E2E Scenario | §7.28 handle.crumb per admin detail route | §4.1 | `handle.crumb` per admin detail route not verified; only one breadcrumb e2e exists. |
| P03-G12 | MEDIUM | Missing Integration Test | §3 list `includeInactive` flag | §2.2 | `doctors::list({includeInactive: true})` / operators / inventory_catalog inactive-inclusion not tested. |
| P03-G13 | LOW | Missing Integration Test | §7.13 `<InventoryAdminTable>` audit-log join | §2.4 | Audit-log join for last-edit-author column not asserted. |
| P03-G14 | LOW | Missing Contract Test | §2 `CutKind` enum | §3.1 | Prisma `CutKind { pct, fixed }` enum-value mapping over the wire not asserted in Ajv schema. |
| P03-G15 | LOW | Missing Snapshot | §10 catalog pull canonicals | §3.3 | §3.3 lists 9 push canonicals but no pull canonical snapshots. |

## Phase 04 Gaps (9)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P04-G01 | HIGH | Missing Integration Test | §7.8 step 2 | §2.1 | No test asserts `ShiftService::edit` rejects `new_check_in_at > now` with `ShiftError::CheckInInFuture`. |
| P04-G02 | HIGH | Missing Integration Test | §1 CHECK constraint | §2.1 | No DB-layer test exercises raw `CHECK (check_out_at IS NULL OR check_out_at >= check_in_at)`; only domain-entity layer. |
| P04-G03 | HIGH | Missing Unit Test | §3 Frontend Zod `ClockOutInputSchema` | §1.2 | `ClockOutInputSchema` has no unit test (only ClockIn / Edit / SoftDelete covered). |
| P04-G04 | MEDIUM | Missing Contract Test | §5 TENANT_MODELS `operator_shifts` | §3.3 | No assertion that `'operator_shifts'` is present in server's `TENANT_MODELS` array. |
| P04-G05 | MEDIUM | Missing Edge Coverage | §7.5 `<ShiftsPage>` ErrorState | §2.4 | `<ShiftsPage>` ErrorState ("Retry") not asserted; only skeleton + empty. |
| P04-G06 | MEDIUM | Missing Integration Test | §3 `shifts::list_open` joined specialties | §2.1 / §2.2 | No test asserts `list_open` returns joined operator specialties; only name + phone covered. |
| P04-G07 | MEDIUM | Missing Integration Test | §4 `<ClockInDialog>` eligible-operators filter | §2.4 | "Active operators NOT currently on an open shift" filter logic untested against real SQLite. |
| P04-G08 | LOW | Missing Unit Test | §7.15 `<EditShiftRowAction>` role gate | §2.4 | No component test asserts edit action gated by `useCurrentUser().role === 'superadmin'`; only E2E. |
| P04-G09 | LOW | Missing Snapshot | §3.3 envelope_version | §10 / §8 DoD | No negative `envelope_version: 999` rejection snapshot / fixture committed. |

## Phase 05 Gaps (11)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P05-G01 | CRITICAL | Missing Integration Test | §7.11 void audit-first | §2.1 (visits_phase05.rs) | No test asserts void writes audit rows BEFORE the visits update / offset inserts / item recomputes (lock has the test; void mirror is absent). |
| P05-G02 | HIGH | Missing Integration Test | §7.54 receipt_print_ok/fail metrics | §2.1 | No test asserts `ReceiptGenerator::render_pdf` / `render_thermal` write `receipt_print_ok` / `receipt_print_fail` rows to `metrics_events`. |
| P05-G03 | HIGH | Missing Integration Test | §7.18 inline-patient outbox fan-out | §2.1 | Lock step 6.7 must enqueue an outbox row for an inline-created patient; no test asserts patient row co-lands with visit + adjustment + audit outbox entries from one lock fan-out. |
| P05-G04 | HIGH | Missing Contract Test | §7.26 LockError tagged union | §3.2 / §3.3 | No contract row diffs the serde-tagged `LockError` variants against the server's i18n key registry / TS Zod LockError schema; only `LockBlocker` is contract-checked. |
| P05-G05 | HIGH | Missing Unit Test | §7.46 numerals format functions | §1.1 | TS port has digit-format unit test; Rust `numerals` module has only end-to-end snapshot coverage, no direct unit test for the lookup map / locale gate. |
| P05-G06 | MEDIUM | Missing Integration Test | §7.37 audit action enum + pruner | §2.1 | No test asserts the audit action enum accepts `lock` / `void` and rejects unknown actions, nor that pruner ownership invariant holds. |
| P05-G07 | MEDIUM | Missing Edge Coverage | §4 step 6.7 + §7.10 | §6.5 | Client-side double-click Lock idempotency (two near-simultaneous `visits::lock` calls on same draft) not covered; only server-side `op_id` replay is. |
| P05-G08 | MEDIUM | Missing Integration Test | §7.51 raw-SQL migration ordering | §2.3 / §6.8 | No assertion that server-side `inventory_adjustments_no_update_pg` trigger ships in a `005_*` raw-SQL file and `prisma migrate status` returns clean. |
| P05-G09 | MEDIUM | Missing Persona / Manual Step | §7.45 shell:allow-execute scoping | §3 / §6.7 | No contract or security test validates `capabilities/main.json` restricts shell-allow to exactly `lpstat -p` and `wmic printer get name`. |
| P05-G10 | MEDIUM | Missing Integration Test | §7.57 PRD UI affordances | §2.4 | Five PRD-mandated UI affordances (`<ChecksGridHeader>`, `<WorkspaceHeader>`, `<NewVisitHeader>`, `<NewVisitActionsBar>`, `<VoidButton>`) have no component tests. |
| P05-G11 | LOW | Missing Coverage Gate | §2 server routes | §1.3 | Sync-server routes coverage gate (>= 85%) not declared; only domain (90%) and conflict-policy (95%). |

## Phase 06 Gaps (7)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P06-G01 | CRITICAL | Missing Integration Test | §7.11 audit-first ordering | §2.1 | Ordering asserted, but NOT that the audit row carries the explicit `delta: { before, after, reason }` payload shape §7.11 mandates. |
| P06-G02 | HIGH | Missing Integration Test | §4 frontend step 3 / §7.6 NotUserSelectable | §2.1 / §6.4 | No test asserts `inventory_create_adjustment` IPC rejects a caller-supplied `reason='consume_visit'`; only role-gate cases at IPC integration scope. |
| P06-G03 | HIGH | Missing Integration Test | §7.6 server defence-in-depth positive | §2.3 | Server `acceptPush` only tested to reject `count_correction` from receptionist; missing positive confirmation that receive/writeoff from any role are accepted. |
| P06-G04 | MEDIUM | Missing Integration Test | §7.14 Postgres CHECK migration | §6.8 / §2.3 | No server-side replay of the raw-SQL migration `inventory_adjustments_delta_sign/migration.sql` against populated Postgres. |
| P06-G05 | MEDIUM | Missing E2E Scenario | §7.5 / §3 `<StockStatusPill>` threshold cross | §4.1 | No E2E asserts that writing off below `low_stock_threshold` triggers the LOW pill on list + `<ItemOverview>` badge update. |
| P06-G06 | MEDIUM | Missing Edge Coverage | §7.8 sanity-cap warning UX | §6.3 / §2.4 | Unit covers boundary; no component/E2E asserts warning toast text + dismissibility + qty retained ("warn, do not block" UX). |
| P06-G07 | LOW | Missing Snapshot | §7.15 reversal-pair payload | §3.3 / §8 | No snapshot for `InventoryAdjustmentWithMeta` `is_reversal: true` row shape; schema declared but not hashed. |

## Phase 07 Gaps (12)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P07-G01 | CRITICAL | Missing Integration Test | §7.18 daily_close_run audit delta | §2.1 | `audit_log.delta` payload contents not asserted on the `daily_close_run` row. |
| P07-G02 | CRITICAL | Missing Integration Test | §7.19 + §7.23 PDF filename invariant | §2.1 | No assertion PDF filename embeds `daily-close_<targetDate>_<inputHashPrefix>.pdf` AND re-runs with new locks produce a NEW file (no overwrite). |
| P07-G03 | HIGH | Missing Integration Test | §4 server step 2 + §7.24 cursor | §2.3 | `/reports/visits` "cap at 10000 rows (paginate beyond)" not tested; nextCursor-null semantics unverified. |
| P07-G04 | HIGH | Missing Contract Test | §7.16 long-range banner | §3.1 | `accounting.banner.long_range_local_only` i18n key + explicit "Authoritative" toggle not snapshotted/contract-tested. |
| P07-G05 | HIGH | Missing Integration Test | §7.15 drill-down link query strings | §2.4 / §4.1 | Drill-down link query strings (`/accounting/visits?from=...&doctorId=...&checkTypeId=...` + per-shift `/reception/shifts?focus=<shift_id>`) not verified. |
| P07-G06 | HIGH | Missing E2E Scenario | §7.17 superadmin acceptance path | §4.1 | Server-side role gate on `/reports/*` for superadmin JWT not exercised; only receptionist 403 + accountant happy path covered. |
| P07-G07 | MEDIUM | Missing E2E Scenario | §7.17 void button visibility | §4.1 | Superadmin sees void button on `/accounting/visits/:id` -- only accountant-hidden case in §4.1. |
| P07-G08 | MEDIUM | Missing Performance SLO | §6.6 / §7.22 top-cards refresh | §7 | `dashboard_tops` "Top-5 cards < 200 ms p95" listed in edge coverage, not as typed SLO row tied to §9 default-comparison. |
| P07-G09 | MEDIUM | Missing Edge Coverage | §7.10 void-vs-inventory interplay | §6.8 | "Voided rows render with negative tint AND do NOT subtract from revenue BUT inventory consumption IS reflected" not asserted. |
| P07-G10 | MEDIUM | Missing Integration Test | §7.16 "Authoritative" toggle | §2.1 / §4.2 | Explicit "Authoritative" toggle forcing server-only mode (override local) not tested; only automatic fallback. |
| P07-G11 | LOW | Missing Snapshot | §7.21 per-check-type breakdown | §3.3 | No separate snapshot for per-check-type breakdown section structural hash inside daily-close PDF. |
| P07-G12 | LOW | Missing Coverage Gate | §7.29 imports::* reserved-but-unwired | §1.3 | No negative test ensuring no `imports::*` IPC is registered in `lib.rs::generate_handler!` for v1. |

## Phase 08 Gaps (14)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P08-G01 | CRITICAL | Missing Integration Test | §7.22 + §3 server resolve idempotency | §2.3 | No assertion server `POST /sync/conflicts/:opId/resolve` short-circuits via ProcessedOp idempotency on duplicate `resolve_op_id` (returns prior cached body exactly). |
| P08-G02 | CRITICAL | Missing Edge Coverage | §7.16 soak report | §6.6 | No assertion that the soak harness writes `target/soak-report.md` with all 6 quantitative criteria captured. |
| P08-G03 | HIGH | Missing Integration Test | §3 server resolve audit | §2.3 | `conflict_resolve` audit row in same `prisma.$transaction` asserted, but no positive assertion that `before_json/after_json` capture pre/post resolved entity state. |
| P08-G04 | HIGH | Missing Integration Test | §7.17 /metrics named labels | §2.3 | No test asserts presence of named metrics (`sync_push_duration_seconds`, `sync_conflict_total`, `outbox_depth_gauge`, `audit_query_duration_seconds`); only generic regex. |
| P08-G05 | HIGH | Missing Contract Test | §7.17 /healthz fields | §3.1 | `HealthSchema` widened to `'ok'|'fail'` but no schema-level enumeration of 5 required keys (`status, db, redis, migrationsApplied, version`). |
| P08-G06 | HIGH | Missing E2E Scenario | §7.17 ConflictResolverPanel header counters | §4.1 | 7-day rolling counters (conflicts opened, resolved, oldest unresolved age) not verified in any E2E. |
| P08-G07 | HIGH | Missing E2E Scenario | §7.10 / §7.18 husky pre-commit | §5 / §6.2 | `.husky/pre-commit` running `lint:i18n` + `lint:rtl` on staged files not verified. |
| P08-G08 | MEDIUM | Missing Integration Test | §7.12 ARIA icon labels | §2.4 | No assertion every icon-only button uses `aria-label={t('a11y.icons.<name>')}` with the 13 enumerated keys. |
| P08-G09 | MEDIUM | Missing E2E Scenario | §7.20 breadcrumbs | §4.1 | `/audit` and `/sync/conflicts` static crumbs via `breadcrumbs.*` i18n keys not verified. |
| P08-G10 | MEDIUM | Missing Edge Coverage | §6.12 end-to-end story | §5 / §4.1 | Consolidated persona spec for the verify-step-12 superadmin journey absent; only fragmented references. |
| P08-G11 | MEDIUM | Missing Integration Test | §7.26 final TENANT_MODELS list | §2.3 | No test asserts the final 15-entry TENANT_MODELS at v0.1.0 (excluding local-only / server-only entries). |
| P08-G12 | MEDIUM | Missing Performance SLO | §7.17 rolling-7d counter query | §7 | No SLO row for rolling-7d conflict counter query (used in resolver header). |
| P08-G13 | LOW | Missing Snapshot | §6.9 RTL visual diff | §8 / §10 | RTL "screenshots of every page" visual diff invariant not in snapshot artifact list. |
| P08-G14 | LOW | Missing Coverage Gate | §7.14 + §7.17 sync-pill / diagnostics-modal | §1.3 | `sync-pill.tsx` + `diagnostics-modal.tsx` not in any §1.3 coverage glob row. |

## Phase 09 Gaps (14)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P09-G01 | CRITICAL | Missing Integration Test | §3 sync-services rewrite (autoload deps) | §2.3 | No test asserts autoload dependency ordering -- `prisma.ts` MUST initialize before `auth-services`/`sync-services` via `fp(..., { dependencies: ['prisma'] })`. |
| P09-G02 | CRITICAL | Missing Integration Test | §3 prisma.ts onClose hook | §2.3 persistence-phase09 | No test asserts `prisma.$disconnect` runs on Fastify `onClose`; restart tests kill the process and miss the graceful path. |
| P09-G03 | CRITICAL | Missing E2E Scenario | §6 step 7 persistence round-trip | §4.1 | Defining E2E `prisma-persistence-survives-container-restart` lists doctor + visit + audit but never asserts an OPEN REFRESH TOKEN survives restart and can refresh post-restart. |
| P09-G04 | CRITICAL | Missing Integration Test | §3 ConflictParkedRepository resolve / resolveTx split | §2.3 conflict-audit-phase09 | No test asserts the `resolve` vs `resolveTx` API split -- `ConflictResolveService` must always use `resolveTx` inside `prisma.$transaction`. |
| P09-G05 | HIGH | Missing Integration Test | §3 HealthSchema widen | §3.1 contract | Contract row exists for both responses, but no test asserts TypeBox schema was actually widened from `Type.Literal('ok')` to the union. |
| P09-G06 | HIGH | Missing Integration Test | §3 Memory* test-only fixtures | §2.3 | No test/grep asserts production paths NEVER instantiate `MemorySyncStore` / `MemoryUserStore` (static-analysis test scanning `sync-server/src/` excluding `test/`). |
| P09-G07 | HIGH | Missing Integration Test | §3 refresh-token rotation atomicity | §2.3 persistence-phase09 | No test asserts rotation is wrapped in `prisma.$transaction` such that a mid-rotation failure leaves neither both tokens valid nor both revoked. |
| P09-G08 | HIGH | Missing E2E Scenario | §5 docker-compose `sync_db_data` volume | §4.1 | No E2E asserts `sync_db_data` named volume persists Postgres data across `docker compose down` + `docker compose up` (full stack down/up cycle absent). |
| P09-G09 | MEDIUM | Missing Integration Test | §5 CI guardrail (no `sync-server/.env` committed) | §6.7 | DoD checklist references the guardrail but no test runs the one-liner against a fixture commit that includes `.env` to prove it FAILS. |
| P09-G10 | MEDIUM | Missing Integration Test | §3 operator_service.rs:222 doc-comment rewrite | §1.1 / §2.1 | Tests assert cascade behavior + grep on file, but no assertion the rewritten comment STATES the rule directly (grep verifies absence, not presence of new prose). |
| P09-G11 | MEDIUM | Missing Edge Coverage | §3 sidebar.tsx:152 "Coming soon" decision | §1.2 / §2.4 | Test "pins whichever decision lands" but the build spec leaves the choice open -- no gap-closing decision recorded; phase cannot move to `complete` without the pin. |
| P09-G12 | LOW | Missing Snapshot | §3 env schema byte-hash | §3.3 | Snapshot path is listed but no test row asserts the hash is verified in CI (file exists != comparison runs). |
| P09-G13 | LOW | Missing Coverage Gate | §5 sync-server/.dockerignore | §1.3 / §2.3 | No verification `.dockerignore` excludes `node_modules`, `dist`, `.env`, `coverage` from the build context. |
| P09-G14 | LOW | Missing Persona / Manual Step | §7 Open Decision #5 (operator cascade rule) | §5 | Manual scripts don't record WHICH option (a or b) was selected for the operator cascade rule. |

## Next Steps (per `.claude/rules/planning.md` Gap Analysis Methodology)

1. **Apply gaps to the owning `phase-XX-test.md`** as `§9.x` additions (one subsection per gap, copy-paste-ready test row or scenario), mirroring how build-spec gaps land as `§7.x` in `phase-XX.md`. Same severity / category metadata; reference the gap ID. **[DONE 2026-05-13. All 112 subsections appended; counts verified per phase: 18/12/15/9/11/7/12/14/14.]**
2. **Re-run Pass 2** after additions land. The cycle ends when a pass finds zero true gaps.
3. **Critical gaps gate first execution.** Test plans cannot move to first test-authoring until P01-G01, P01-G02, P05-G01, P06-G01, P07-G01, P07-G02, P08-G01, P08-G02, P09-G01, P09-G02, P09-G03, P09-G04 land in their phase plans.
4. **Cross-phase pattern: audit-payload completeness.** P05-G01, P06-G01, P07-G01 all concern audit-row payload shape vs ordering. Worth a one-time `with_audit` helper assertion (asserts both ordering AND `delta` payload structure) that every audit-emitting phase reuses.
5. **Cross-phase pattern: TENANT_MODELS membership.** P03-G09, P04-G04, P08-G11 are all "no test asserts the model is in TENANT_MODELS". A single phase-08-test contract test ranging across all 8 catalog + operator_shifts + visits/visit_items/patients + inventory_adjustments entries closes them in one row.
