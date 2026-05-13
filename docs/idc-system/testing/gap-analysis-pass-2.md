# Gap Analysis Pass 2 -- Test Plans vs Build Specs

_Date: 2026-05-13_

Pass 2 re-compares every `docs/idc-system/phase-XX.md` build spec against the matching `docs/idc-system/testing/phase-XX-test.md` test plan AFTER the 112 Pass 1 §9.x additions landed. A gap is a scenario, command, route, schema rule, sync contract, conflict policy, snapshot, performance SLO, coverage gate, or edge case that the build spec promises but the updated test plan still does not verify. New exposure introduced by the Pass 1 §9 additions themselves (a `§9.x` row that names a helper or invariant that itself has no test, or contradicts the build spec) is in scope for Pass 2.

Cross-cutting items legitimately delegated to `security.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`, or another phase plan (and listed in the owning test plan header's `Out of scope` line) are NOT counted.

Methodology per `.claude/rules/planning.md` Gap Analysis Methodology (Pass 2+) applied to the testing surface defined in `.claude/rules/testing.md` §3-§6, with extra weight on the focus areas the methodology calls out: state machines, field completeness, sync contracts, conflict resolution, integration points (events, capabilities, autoload deps), setup/config screens, report drill-down.

## Pass 2 Totals

| Phase | Total | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing | 14 | 2 | 5 | 5 | 2 |
| 02 Authentication & Users | 14 | 0 | 6 | 7 | 1 |
| 03 Catalog & Reference Data | 16 | 0 | 5 | 8 | 3 |
| 04 Operator Shifts | 14 | 0 | 5 | 7 | 2 |
| 05 Reception & Visit Lock | 18 | 0 | 8 | 7 | 3 |
| 06 Inventory Operations | 10 | 1 | 3 | 4 | 2 |
| 07 Accounting & Reports | 14 | 0 | 5 | 7 | 2 |
| 08 Audit, Conflict Resolver & Polish | 16 | 2 | 5 | 7 | 2 |
| 09 Pre-Ship Hardening | 16 | 3 | 6 | 5 | 2 |
| **Total** | **132** | **8** | **48** | **57** | **19** |

Pass 1 surfaced 112 gaps (12 critical, 35 high, 44 medium, 21 low). Pass 2 surfaces 132 -- the count grew rather than shrank for two reasons that both prove the pass is doing its job:

1. **Pass 1 §9 additions themselves opened new surface.** Eight Pass 2 rows directly attach to a Pass 1 row (e.g. P01-G19 catches that §9.7 contract-tests the wrong compound PK; P01-G20 catches that §9.1 emits a `metrics_events.kind` literal not in the CHECK list). Without Pass 2 these would ship false-green tests.
2. **Pass 2 dug into the focus areas Pass 1 historically misses.** Field-by-field schema completeness, push/pull symmetry, per-entity policy registry enumeration, hook REGISTRATION (vs hook behaviour), and snapshot canonicalisation. These are exactly the categories `.claude/rules/planning.md` flags as common Pass 1 blind spots.

Severity rubric per phase prompt: CRITICAL = missing test of a safety/correctness invariant; HIGH = missing test of a major user-facing flow or business rule; MEDIUM = missing test of an edge case the build spec calls out; LOW = cosmetic, advisory, or a coverage-gate / snapshot listing omission.

## Critical Gaps (8)

| ID | Phase | Build spec ref | Gap |
|-|-|-|-|
| P01-G19 | 01 | §7.19 SyncCursor `@@id([deviceId, entityIdTenant])` | Pass 1 §9.7 contract-tests the WRONG compound PK shape (`[entityIdTenant, entity]`); build spec line 650 declares `@@id([deviceId, entityIdTenant])`. The test as written verifies a non-existent invariant. |
| P01-G20 | 01 | §7.28 `metrics_events.kind` CHECK vs §9.1 emitter | Pass 1 §9.1 inserts `kind='jwt_pin_offline'` but the §7.28 `metrics_events.kind` CHECK list excludes that literal. The insert fails at runtime; no test covers CHECK-vs-emitter symmetry across all kinds the codebase emits. |
| P06-G08 | 06 | §7.9 SyncPullService hook registration | No test asserts `on_pull_applied_inventory` callback is REGISTERED in phase-01 `SyncPullService` at startup. Pull-apply behaviour is tested, but an unwired hook would silently no-op and recomputes would never fire on pulled adjustments. |
| P08-G15 | 08 | §7.21 vacuum step 5 single-row atomicity | No assertion the composite vacuum job writes EXACTLY ONE self-audit row covering BOTH `audit_log` + `metrics_events` purges in the same transaction; two rows or a torn write breaks audit-of-audit invariants. |
| P08-G16 | 08 | §7.22 client `compute_resolve_op_id` canonicalisation | No contract test that client `sha256(opId\|choice\|merged_canonical_json)` uses canonical JSON (sorted keys, normalised). Two clients computing different hashes for the same logical merge defeats the server idempotency cache. |
| P09-G15 | 09 | §3 `PrismaSyncCursorRepo` upsert composite PK | No test asserts `bumpCursor` uses `upsert` (not insert-then-update) against composite PK `(entityId, deviceId)`. A race between two devices on the same cursor would silently break monotonicity. |
| P09-G16 | 09 | §3 `init-custom-sql.sql` ordering | No test asserts `Dockerfile.dev` CMD runs `prisma db push` BEFORE `psql -f init-custom-sql.sql`. Reversed order ships with triggers and CHECK constraints unapplied and no boot error. |
| P09-G17 | 09 | §3 `PrismaEntityRepo` single `$transaction` per batch | No test asserts `dispatchEntity` wraps a push batch in `prisma.$transaction([...])`. Partial-batch failure must roll the whole batch back to preserve phase-01 idempotency invariants. |

## Phase 01 Gaps (14)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P01-G19 | CRITICAL | Missing Contract Test | §7.19 SyncCursor `@@id([deviceId, entityIdTenant])` | §3.3 / §2.3 | §9.7 test asserts wrong compound PK `[entityIdTenant, entity]`; spec line 650 declares `@@id([deviceId, entityIdTenant])` -- contract test verifies wrong invariant. |
| P01-G20 | CRITICAL | Missing Integration Test | §7.28 `metrics_events.kind` CHECK vs §9.1 `jwt_pin_offline` | §2.1 | §9.1 inserts `kind='jwt_pin_offline'` but §7.28 CHECK list excludes it -- insert fails at runtime; no test covers CHECK-vs-emitter symmetry. |
| P01-G21 | HIGH | Missing Integration Test | §7.22 `IDC_SYNC_SERVER_URL` env override | §2.1 / §2.2 | Env-var dev override for sync server URL has no test row; only persisted-store path is asserted. |
| P01-G22 | HIGH | Missing Integration Test | §7.16 delete-vs-edit on SyncPushService step 5 | §2.3 | Server-side delete-vs-edit rule on `/sync/push` untested; tie-goes-to-deletion verified only on engine pull. |
| P01-G23 | HIGH | Missing Integration Test | §7.20 `/sync/lookup-op` JWT required | §2.3 / §6.7 | No 401 negative test on `/sync/lookup-op` despite §3 declaring all non-healthz routes require JWT. |
| P01-G24 | HIGH | Missing Edge Coverage | §7.14 RedactionLayer regex `patient_name\|email` | §1.1 / §2.1 | Redaction asserted for `password\|token\|hash` but `patient_name` and `email` (also in regex) untested. |
| P01-G25 | HIGH | Missing Contract Test | §7.26 ErrorResponseSchema `traceId` required | §3.1 | Error-response captures never assert `traceId` is present and non-empty on every error path. |
| P01-G26 | MEDIUM | Missing Integration Test | §7.10 i18n namespace files exist | §2.1 / §6.2 | No test asserts `src/i18n/locales/{ar,en}/{common,errors,receipts}.json` files exist and load via i18next. |
| P01-G27 | MEDIUM | Missing Contract Test | §7.32 `pulledAt` server-only field | §2.3 / §3.1 | No assertion `/sync/pull` response omits `pulledAt` -- PRD mandates it is never returned to clients. |
| P01-G28 | MEDIUM | Missing Integration Test | §7.35 `tauri.conf.json` bundle env `IDC_EMBEDDED_MODE=0` | §2.1 | No test asserts `tauri.conf.json` `bundle.{windows,macOS}.env` declares `IDC_EMBEDDED_MODE=0` for shipped builds. |
| P01-G29 | MEDIUM | Missing Edge Coverage | §7.11 axe-core on `/login` and `/no-access` routes | §1.3 / §6.7 | §9.10 wires axe against `components/shell/**` but §7.11 specifies routes `/login` + `/no-access`; route-level a11y unverified. |
| P01-G30 | MEDIUM | Missing Integration Test | §7.17 `outbox.parked` column schema | §2.1 | No schema assertion `parked` column has `DEFAULT 0` and `CHECK (parked IN (0,1))`; partial index relies on it. |
| P01-G31 | LOW | Missing Snapshot | §7.26 ErrorResponseSchema canonical shape | §3.3 / §10 | No snapshot of canonical ErrorResponseSchema instance (`{code,message,details,traceId}`) committed -- shared error shape drift goes uncaught. |
| P01-G32 | LOW | Missing Integration Test | §7.25 shadcn override files | §2.4 | §9.18 walks `components/ui/` generically; no test asserts the four specific override files named in §7.25 (`button/icon-button/link/tabs.tsx`) exist. |

## Phase 02 Gaps (14)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P02-G13 | HIGH | Missing Contract Test | §7.24 `UserResetPasswordPushSchema` variant | §3.1 / §3.3 | Push payload for `users::reset_password` (password_hash REQUIRED) is never schema-validated; §3.1 only covers create + update. |
| P02-G14 | HIGH | Missing Contract Test | §7.18 12-value closed audit-action union | §9.2 / §3.1 | §9.2 validates only 6 of 12 action literals; missing `logout`, `lock`, `void`, `clock_in`, `clock_out`, `conflict_resolve`, `vacuum`. |
| P02-G15 | HIGH | Missing Integration Test | §7.21 server Prisma `seed.ts` BOOTSTRAP_SUPERADMIN env | §2.3 | Server bootstrap seed (env-driven, Argon2id, idempotent on existing superadmin) has no `seed.ts` integration test. |
| P02-G16 | HIGH | Missing Integration Test | §7.28 `settings::set_locale` role-gated IPC | §2.2 | `settings::set_locale` listed as gated command but absent from the §2.2 IPC table; no happy/error path. |
| P02-G17 | HIGH | Missing Integration Test | §4 client `AuthService::login` audit-`login` action | §2.1 | Online-login audit row written but no test asserts `action='login'` specifically (only generic "audit row" assertion). |
| P02-G18 | HIGH | Missing Integration Test | §7.18 `logout` audit action | §2.1 / §2.3 | `auth::logout` is in the closed audit-action union but no test asserts a `logout` audit row is written. |
| P02-G19 | MEDIUM | Missing Integration Test | §5 + §7.26 plugin registrations: `tauri-plugin-os`, `jsonwebtoken` | §2.1 | Plugin registrations are declared but no test asserts they are actually wired in `lib.rs::run()`. |
| P02-G20 | MEDIUM | Missing Integration Test | §5 server `@fastify/jwt` with RS256 keypair | §2.3 | `@fastify/jwt` plugin registration with the loaded RS256 keypair never asserted server-side. |
| P02-G21 | MEDIUM | Missing Integration Test | §7.10 login no longer overwrites JWT pin | §2.1 | No test asserts a successful login does NOT mutate `jwt/publicKey` in stronghold (only `bootstrap_jwt_key` may). |
| P02-G22 | MEDIUM | Missing Unit Test | §7.1 `thermal_printer_name` membership validation | §1.1 / §2.1 | `thermal_printer_name` value validation against `settings::list_printers()` result on save has no test. |
| P02-G23 | MEDIUM | Missing E2E Scenario | §7.22 single-transaction atomic save | §4.1 | `<SettingsForm>` atomic multi-key save (partial writes impossible on failure) never exercised. |
| P02-G24 | MEDIUM | Missing Integration Test | §4 Sync Semantics `users` `op_id` idempotency | §2.3 | `users` push idempotency on duplicate `op_id` (ProcessedOp cached envelope replay) not asserted. |
| P02-G25 | MEDIUM | Missing Unit Test | §3 `SettingSchema` value coerced by valueType | §1.2 | Zod `SettingSchema` value coercion per `valueType` (string -> int/decimal/bool) has no round-trip coercion test. |
| P02-G26 | LOW | Missing Snapshot | §3.3 + §7.24 reset_password push envelope | §3.3 / §8 | Snapshot list omits `user-reset-password-push-canonical.json.sha256` (distinct from `user-create-push` per §7.24 semantics). |

## Phase 03 Gaps (16)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P03-G16 | HIGH | Missing Integration Test | §7.10 consumption-map audit deltas | §2.1 / §9.4 | G04 covers consumption-map soft_delete delta but NOT `consumption::upsert` create+update audit before/after JSON capture per §7.10 step 5. |
| P03-G17 | HIGH | Missing Integration Test | §7.27 `<ActiveDraftsBadge>` listener | §2.4 | §7.27 names two listeners (`<PricingChangedBanner>` AND `<ActiveDraftsBadge>`); only the banner is wired in test plan. |
| P03-G18 | HIGH | Missing Contract Test | §7.31 raw-SQL migration `prisma migrate status` clean | §3.1 | §2.3 asserts apply-in-order but not the §7.31 invariant `pnpm prisma validate` + `pnpm prisma migrate status` clean after each of the §7.20/§7.21 drop-then-create pairs. |
| P03-G19 | HIGH | Missing Integration Test | §7.23 SQL `WHERE deleted_at IS NULL AND (is_active=1 OR id=:includeId)` | §2.2 | G05 covers inactive-include; no test that soft-deleted (`deleted_at != NULL`) doctor with matching `include_id` is STILL excluded. |
| P03-G20 | HIGH | Missing Edge Coverage | §6.3 partial-batch push (op 27 XOR violation) | §6.3 | Server's per-op result envelope shape (`{op_id, status, error?}` array) for mixed-success batch not asserted; only outcome stated. |
| P03-G21 | MEDIUM | Missing Contract Test | §7.19 `pulledAt` not exposed to clients | §3.3 | §7.19 mandates `pulledAt` is diagnostic-only; G15 snapshots include it -- contradiction needs explicit test asserting absence from client response. |
| P03-G22 | MEDIUM | Missing Contract Test | §3.2 `operators::get` / `inventory_catalog::get` composite returns | §3.2 | Composite shapes `{operator,specialties}` and `{item,consumption}` listed but no Zod schema name (`OperatorWithSpecialtiesSchema`, `InventoryItemWithConsumptionSchema`) declared. |
| P03-G23 | MEDIUM | Missing Integration Test | §3 IPC `check_types::list({includeDeleted: true})` | §2.2 | Happy-path `includeDeleted=true` branch (returning soft-deleted rows) not exercised; only `optionally_including_deleted` mentioned without explicit deleted-row assertion. |
| P03-G24 | MEDIUM | Missing Integration Test | §6.1 pulled `updated_at` server-authoritative | §2.1 / §6.1 | §6.1 declares "pulled `updated_at` is server-authoritative" but no scenario asserts local clock-skewed write overwritten by server timestamp on pull. |
| P03-G25 | MEDIUM | Missing Edge Coverage | §6.4 delete-vs-edit cross-row | §6.4 | §6.4 mentions delete-vs-edit (incoming edit T1 vs local soft-delete T2 -> deletion wins) but no concrete scenario row; only narrative. |
| P03-G26 | MEDIUM | Missing Integration Test | §1 raw `unit` CHECK constraint | §2.1 | G06 covers empty-string unit; the §7.5 `length(trim(unit)) > 0` CHECK against `\t`/`\n`/zero-width whitespace not covered. |
| P03-G27 | MEDIUM | Missing Integration Test | §7.17 LWW tiebreak per-entity policy registration | §3.3 | §3.3 declares "all 8 entities are `last-write-wins`" but no contract test enumerates the policy registry returns `'last-write-wins'` for each of the 8 entity table names. |
| P03-G28 | MEDIUM | Missing E2E Scenario | §7.27 `catalog:pricing_changed` `kind='settings'` | §4.1 | No §4.1 spec exercises the phase-02 settings -> phase-03 emitter -> phase-05 banner cross-phase chain end-to-end. |
| P03-G29 | LOW | Missing Snapshot | §10 envelope versioning `envelope_version: 1` | §3.3 | §3.3 mentions `envelope_version: 1` but no snapshot of a full push envelope (header + ops array) is committed; only per-entity payloads. |
| P03-G30 | LOW | Missing Persona / Manual Step | §5.1 `<AdminShell>` 7-area visual | §5.1 | §7.11 mandates exact area order (Users, Check Types, Doctors, Operators, Inventory, Settings, Audit); §5.1 says "7 items" but no manual checklist enumerates names + order. |
| P03-G31 | LOW | Missing Integration Test | §3 `useCheckSubtypesByType` / `useOperator(id)` | §2.4 | §2.4 lists hook names but both have empty test entries (`-- \| --`); no asserted behaviour. |

## Phase 04 Gaps (14)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P04-G10 | HIGH | Missing Integration Test | §7.6 server `ShiftService::acceptPush` insert-vs-update branch | §2.3 | No test asserts server distinguishes insert vs update by row presence before applying LWW. |
| P04-G11 | HIGH | Missing Integration Test | §7.11 audit-first for `shifts::edit` and `shifts::soft_delete` | §2.1 | §2.1 covers audit rollback for clock_in/out generically, but no test pins audit-first ordering for `edit` and `soft_delete` specifically. |
| P04-G12 | HIGH | Missing Integration Test | §7.13 `pulledAt` set by `SyncPullService` after ship | §2.3 | §2.3 asserts `pulledAt` present in pulled row but no test asserts it is written/updated server-side after each successful pull batch. |
| P04-G13 | HIGH | Missing Edge Coverage | §7.1 `<ResolveOverlappingShifts>` rollback on failure | §2.4 | No test asserts UI rollback when second step (soft_delete) fails after first step (clock_out) succeeded inside `<ResolveOverlappingShifts>`. |
| P04-G14 | HIGH | Missing Snapshot | §10 sync envelope golden files | §10 / §8 DoD | DoD lists push/pull SHA256 snapshots but no soft-delete-canonical or update-clockout-canonical hash snapshot, despite §3.1 listing those payloads. |
| P04-G15 | MEDIUM | Missing Integration Test | §7.10 `shifts::soft_delete` outbox under additive contract | §2.1 | No test asserts the `soft_delete` outbox row carries the additive-contract envelope (vs. tombstone). |
| P04-G16 | MEDIUM | Missing Contract Test | §3 `NoteUpdate` tagged enum (`Replace { value }` / keep) | §3.2 | `ShiftEditSchema` `NoteUpdate` tagged-union shape not diffed Rust serde vs TS Zod in §3.2. |
| P04-G17 | MEDIUM | Missing Integration Test | §7.14 FK `ON DELETE RESTRICT` on `check_out_by_user_id` | §6.8 | §6.8 `restrict_user_hard_delete_when_referenced_by_shift` does not differentiate between `check_in_by_user_id` and `check_out_by_user_id` FKs; only one column tested. |
| P04-G18 | MEDIUM | Missing Integration Test | §7.2 `operator_shifts_today` index creation | §2.1 / §6.8 | `history_today_index_used_by_query_plan` asserts plan uses index but no test asserts the index is actually created by migration 004. |
| P04-G19 | MEDIUM | Missing Persona / Manual Step | §5.1 `<ShiftsPageHeader>` clock-in button | §5.1 | No manual review of `<ShiftsPageHeader>` top-right `[+ Clock in operator]` button position/RTL mirror from §7.15. |
| P04-G20 | MEDIUM | Missing Integration Test | §4 `<ClockInDialog>` optimistic update of two caches | §2.4 | §2.4 covers `clock_in_invalidates_all_shifts_keys` but not the optimistic-update path (insert into `['shifts','open']` and `['shifts','today']` before IPC resolves) or rollback on failure. |
| P04-G21 | MEDIUM | Missing Contract Test | §3.3 `ProcessedOp` idempotency cache shape | §3.3 | §3.3 + §2.3 verify push idempotency behaviour but no contract test asserts the `ProcessedOp` cached-response JSON shape returned on replay. |
| P04-G22 | LOW | Missing Coverage Gate | §1.3 / §8 sync-server route coverage gate | §8 DoD | §8 DoD lists ">= 85% sync server route handlers for operator_shifts slice" but no §1.3 row enumerates the path glob / `c8 --include` invocation. |
| P04-G23 | LOW | Missing Snapshot | §10 fixture catalogue | §10 | Snapshot section lists `operator-shift-push-canonical` and pull-row but `fixtures/payloads/operator-shift-push-update-clockout.json` and `...push-soft-delete.json` (cited in §3.1) have no hash snapshot listed. |

## Phase 05 Gaps (18)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P05-G12 | HIGH | Missing E2E Scenario | §7.58 `/inventory/*` role guard | §4.1 | `reception-route-role-guard` only covers `/reception/*`; no E2E asserts `/inventory/*` rejects receptionist (symmetric gate per phase-06). |
| P05-G13 | HIGH | Missing Contract Test | §7.40 visits manual-policy 4-step algorithm | §3.3 / §2.3 | No test exercises all four `accept_push` branches (absent INSERT, lower-version park, equal-version snapshot-diff park, higher-version accept) as a covered matrix. |
| P05-G14 | HIGH | Missing Integration Test | §7.3 sync-apply per-entity validator hook | §2.1 | No assertion that `SyncEngine::apply_pull` runs the visits re-validate routine and rejects malicious pulls violating invariants 2-5 (subtype/dye/report). |
| P05-G15 | HIGH | Missing Integration Test | §7.17 + §7.52 server-side pull name-snapshot enforcement | §2.3 | No server test rejects a `status=locked` push whose name-snapshot columns are null; only the local SQLite CHECK is asserted. |
| P05-G16 | HIGH | Missing Integration Test | §7.42 `<DraftStaleBanner>` shared base | §2.4 | No test asserts pricing-changed and settings-changed banners share the `<DraftStaleBanner>` base component (single dismissable instance, not double-stacked). |
| P05-G17 | HIGH | Missing Integration Test | §7.50 `visits:locked` cache TTL 30s | §2.4 | `useLinesRunToday` declared with 30s TTL but no test asserts the cache TTL nor explicit invalidation path on `visits:locked` event. |
| P05-G18 | HIGH | Missing Integration Test | §7.46 `numerals` reads from `settings_cache` | §1.1 / §2.1 | Numerals unit tests pass a settings struct; no test asserts the lock-time receipt path reads `arabic_numerals` from the live `settings_cache`, not a stale closure. |
| P05-G19 | HIGH | Missing Unit Test | §7.45 platform-appropriate command selection | §1.1 / §6.7 | `settings::list_printers` chooses `lpstat`/`wmic` at runtime per OS; no unit test asserts the platform-dispatch function picks the correct binary per `cfg!(target_os)`. |
| P05-G20 | MEDIUM | Missing Edge Coverage | §6.5 + §7.16 boot sweeper 5-min threshold | §6.5 | `crash_after_receipt_write_before_commit_cleans_tmp_files` asserts cleanup but not the explicit 5-min age threshold for the boot sweeper. |
| P05-G21 | MEDIUM | Missing Edge Coverage | §7.27 banner dismissable until next event | §2.4 | No test asserts `<PricingChangedBanner>` dismiss state resets on next `catalog:pricing_changed` event arrival. |
| P05-G22 | MEDIUM | Missing Edge Coverage | §7.23 `thermal_printer_name=null` prompts on first print | §2.2 / §4 | No test exercises the first-print path where `thermal_printer_name` is null, prompting the user to pick a printer before sending bytes. |
| P05-G23 | MEDIUM | Missing Integration Test | §7.20 `<ChecksGridCard>` FTS recent-usage ordering | §2.4 | Test asserts 3-chip overflow but not that sample subtypes are ordered by recent-usage (FTS), not alphabetically. |
| P05-G24 | MEDIUM | Missing Integration Test | §7.21 cursor encoding (created_at, id) | §2.1 | `list_workspace_paginates_by_created_at_id_cursor` exists but no test asserts the cursor token is a stable base64 `(created_at, id)` pair (decoded round-trip parity). |
| P05-G25 | MEDIUM | Missing Integration Test | §7.5 `visits_drafts` partial index used | §2.1 | `list_drafts_by_check_returns_drafts_regardless_of_date` checks output rows but does not `EXPLAIN QUERY PLAN` to confirm `visits_drafts` partial-index hit. |
| P05-G26 | MEDIUM | Missing Contract Test | §7.4 server Prisma `Visit` indexes | §3.3 / §6.6 | No test asserts the three required `@@index` rows on server `Visit` (`status,lockedAt`, `patientId`, `checkTypeId,status`) actually exist via Prisma introspection. |
| P05-G27 | LOW | Missing Edge Coverage | §7.39 `created_at` immutability across void | §6.8 | `Visit::void::preserves_created_at_and_locked_at` is a unit test; no integration scenario snapshots `created_at` before/after a full void flow including DB write. |
| P05-G28 | LOW | Missing Persona / Manual Step | §7.55 Document Center deferral | §5.1 | No manual or contract step verifies receipts stay local-only (no Document Center upload), the §7.55 deferral receipt. |
| P05-G29 | LOW | Missing Coverage Gate | §7.54 metrics_events retention follows phase-08 §7.21 | §6.5 / §8 | No test or DoD check asserts the `metrics_events` retention/pruner cross-reference is wired so phase-05 emissions get pruned per phase-08's policy. |

## Phase 06 Gaps (10)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P06-G08 | CRITICAL | Missing Integration Test | §7.9 SyncPullService hook registration | §2.1 / §3.3 | No test asserts `on_pull_applied_inventory` callback is REGISTERED in phase-01 `SyncPullService` at startup; behaviour tested but an unwired hook would silently no-op. |
| P06-G09 | HIGH | Missing Contract Test | §7.6 `NotUserSelectable` variant | §3.2 AppError envelope | New `AdjustmentError::NotUserSelectable` variant listed in §3.2 prose but no row asserts its `kind` literal is in the shared `AppErrorSchema` enum. |
| P06-G10 | HIGH | Missing Integration Test | §7.6 server `acceptPush` consume_visit caller bypass | §2.3 | Server-side `/sync/push` test for caller-supplied `reason='consume_visit'` without a real `visit_id` rejection (mirror of §9.2 IPC test) absent at server layer. |
| P06-G11 | HIGH | Missing Integration Test | §7.11 audit-first ordering on `create` row | §2.1 | `create` audit row on `inventory_adjustments` delta payload shape not asserted -- §9.1 only covers the `update` row on `inventory_items`. |
| P06-G12 | MEDIUM | Missing Edge Coverage | §7.5 search debounce 250ms | §2.4 | No test asserts `<InventoryItemsTable>` search input actually debounces at 250ms (only that 2-char min is enforced). |
| P06-G13 | MEDIUM | Missing Integration Test | §6.7 soft-delete bypass + §7.2 SUM tombstone | §2.1 / §6.8 | No test soft-deletes a reversal-pair sibling and asserts `quantity_on_hand` recomputes excluding it (only generic SUM-with-tombstone covered). |
| P06-G14 | MEDIUM | Missing Coverage Gate | §1.3 sync-server presentation gate | §1.3 | `domains/inventory/presentation/**` >= 85% coverage row is unsatisfiable since §2.3 declares no new routes; gate should be dropped or scoped to push/pull branches. |
| P06-G15 | MEDIUM | Incomplete Coverage | §3.2 `last_adjusted_at` nullable | §2.1 / §2.2 | No test asserts `inventory_list_items` returns `last_adjusted_at: null` for items with zero live adjustments (Zod declares nullable, Rust must emit). |
| P06-G16 | LOW | Missing Persona / Manual Step | §5.1 active/inactive opacity convention | §5.1 | "muted style (`opacity: 0.6`)" cited but no automated component assertion in §2.4 -- only manual step. |
| P06-G17 | LOW | Missing E2E Scenario | §4.1 `adjust-unusually-large-warns-but-saves` vs §9.6 `adjust-warning-toast-text-and-retention` | §4.1 / §4.2 | Two E2E specs target §7.8 UX overlapping; neither explicitly asserts the `<Alert role="alert">` accessibility wiring announced via screen reader. |

## Phase 07 Gaps (14)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P07-G13 | HIGH | Missing Integration Test | §7.18 + phase-01 §7.8 audit-action enum | §2.1 / §6.8 | No test asserts `daily_close_run` is accepted by the application-enforced audit-action enum (rejected before §7.8 expansion). |
| P07-G14 | HIGH | Missing Integration Test | §7.22 dashboard_tops `include_voided` param + role gate | §2.2 | `reports::dashboard_tops` happy-path test omits `include_voided` toggle behaviour and the error-path role-gate row is `(role-gate mirror)` placeholder only. |
| P07-G15 | HIGH | Missing Contract Test | §7.14 + §7.24 groups-mode response shape | §3.1 | Tagged union `{ mode: 'groups', groups: [{ key, label, count, revenue, doctor_cut, operator_cut, net }], totals }` keys not asserted against TypeBox schema. |
| P07-G16 | HIGH | Missing Integration Test | §6.4 + §7.18 daily_close_run sync push | §6.4 / §3.3 | §6.4 declares "reports don't push" but the `daily_close_run` audit row DOES push via additive-only policy; no round-trip test on server. |
| P07-G17 | HIGH | Missing Integration Test | §9.4 Authoritative toggle IPC param wiring | §2.4 | Snapshot pins DOM only; no test that toggling `[Authoritative]` actually forwards `authoritative=true` to every reports IPC (visits, daily-close, doctor, operator). |
| P07-G18 | MEDIUM | Missing Edge Coverage | §7.10 voided rendered as negative-tinted row below totals on PDF | §3.3 / §1.1 | `DailyCloseGenerator::render_pdf` voided-row placement under totals (informational, not subtracted) not snapshotted or unit-tested. |
| P07-G19 | MEDIUM | Missing Integration Test | §7.25 doctor/operator CSV footer format | §1.1 | Only `csv_writer_visits` footer asserted; doctor and operator CSV `TOTAL,...` footer column-count and house-row-last invariant not tested. |
| P07-G20 | MEDIUM | Missing Snapshot | §7.26 + §7.30 i18n keys for breakdown table columns | §3.3 | No snapshot for `accounting.doctors.breakdown.columns.*`, `accounting.operators.shifts.columns.*`, `accounting.actions.export_csv` key namespace stability. |
| P07-G21 | MEDIUM | Missing E2E Scenario | §7.13 Print buttons retained in readonly mode | §4.1 | `accountant-readonly-visit-detail.e2e.ts` asserts Edit/Void/Discard absent but not that Print buttons remain visible/enabled. |
| P07-G22 | MEDIUM | Missing Integration Test | §7.19 "Last run at" timestamp UI update | §2.4 | Idempotent re-run "Last run at" timestamp refresh on `<DailyCloseLayout>` after re-click not asserted. |
| P07-G23 | MEDIUM | Missing Integration Test | §7.20 `audit_log.delta.provisional` flag toggle | §2.1 | Test asserts artifact `provisional` field but not that the audit row's `delta.provisional` flips true when outbox > 0. |
| P07-G24 | MEDIUM | Missing Contract Test | §3.2 new `ReportsError` variants enumeration | §3.2 | `Forbidden`, `DateRangeInvalid`, `RangeAbove90Days`, `EmptyDay` listed in AppError row but no contract test enumerates these variant kinds match Rust serde tags. |
| P07-G25 | LOW | Missing Integration Test | §7.11 `[Sign and freeze]` tooltip text | §2.4 | Tooltip "Available in v0.2" text on disabled `[Sign and freeze]` button not asserted (i18n key + rendering). |
| P07-G26 | LOW | Missing Edge Coverage | §6.5 atomic-rename pattern for CSV write | §6.5 | "Aborted write -> temp file removed" claim has no named test; only PDF tmp-rename test exists. |

## Phase 08 Gaps (16)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P08-G15 | CRITICAL | Missing Edge Coverage | §7.21 vacuum step 5 single-row atomicity | §6.5 / §2.1 | No test asserts the composite vacuum job writes EXACTLY ONE self-audit row covering BOTH audit_log + metrics_events purges in the same transaction (not two rows). |
| P08-G16 | CRITICAL | Missing Contract Test | §7.22 client compute_resolve_op_id canonicalisation | §3.2 / §3.3 | No contract test asserts client `sha256(opId\|choice\|merged_canonical_json)` uses canonical JSON (sorted keys, normalised) so server idempotency cache matches across clients. |
| P08-G17 | HIGH | Missing Integration Test | §7.21 vacuum step 6 ordering | §2.1 | No assertion that `sync_state.last_audit_vacuum_at` update is the FINAL step (after audit-row write) so partial-run rollback leaves cursor untouched. |
| P08-G18 | HIGH | Missing E2E Scenario | §4 ConflictResolverPanel emit `sync:conflict` event | §4.1 / §2.4 | No test verifies `<ConflictResolverPanel>` reacts to incoming `sync:conflict` events mid-session (live list update without manual refetch). |
| P08-G19 | HIGH | Missing Integration Test | §7.4 cursor encoding `{ at, id, source }` base64url | §3.1 / §2.3 | No assertion server `/audit/query` next_cursor decodes to `{at,id}` base64url AND merged-paginator cursor adds `source` field -- symmetry between client + server cursor schemas. |
| P08-G20 | HIGH | Missing Persona / Manual Step | §5.1 manual scripts list | §5.1 | No manual visual review row for `<EntityIdSubstringInput>` (§7.24) placeholder rendering + RTL in `<AuditFilters>`. |
| P08-G21 | HIGH | Missing Edge Coverage | §6 verify step 11 receipt_print_success >99% surfacing | §6.6 / §6.7 | No test asserts `diagnostics::summary.receipt_print_success_rate_30d` SURFACES the >99% threshold visually (red below, green above). |
| P08-G22 | MEDIUM | Missing Integration Test | §7.17 `<UserMenu>` Diagnostics entry visibility | §2.4 | No test asserts `<UserMenu>` Diagnostics entry hides for non-superadmin per §7.23 role-link hide pattern. |
| P08-G23 | MEDIUM | Missing Contract Test | §7.6 AuditQuerySchema `action` 12-value vs §1.1 14-value drift | §3.1 | Schema declares 12 action values (§7.6); §1.1 unit test asserts 14 (incl. `daily_close_run`); no contract test reconciles server TypeBox vs final phase-01 §7.36 enum. |
| P08-G24 | MEDIUM | Missing Integration Test | §7.18 RTL icon `DirectionalChevron` wrapper | §2.4 | No test that `<DirectionalChevron direction="forward">` actually renders correct icon (ChevronLeft vs ChevronRight) and rotation class per direction. |
| P08-G25 | MEDIUM | Missing Performance SLO | §7.17 `/metrics` cardinality bound | §7 | No SLO/assertion bounding `/metrics` exposition body size or label cardinality to prevent scrape blow-up across tenants. |
| P08-G26 | MEDIUM | Missing Edge Coverage | §7.21 step 4 metrics_events FK / sync_version invariants | §6.8 | No assertion `metrics_events` hard-delete doesn't violate any FK and that `sync_version` on `audit_log` increments through soft-delete normally. |
| P08-G27 | MEDIUM | Missing Edge Coverage | §6 verify step 11 telemetry derivation | §6.6 | No test asserts `diagnostics::summary.receipt_print_success_rate_30d` is computed from `metrics_events.kind='receipt_print_success' / 'receipt_print_fail'` (precise kind names). |
| P08-G28 | MEDIUM | Missing E2E Scenario | §4 `<MergeEditor>` for `settings` entity | §4.1 | Only `visits` merge tested explicitly; `settings` merge per §3.Frontend `<MergeEditor>` declaration not exercised end-to-end. |
| P08-G29 | LOW | Missing Snapshot | §3 server `AuditQueryResponseSchema` golden | §10 / §3.3 | No snapshot for a typical 50-row `/audit/query` response with mixed actions/entities to lock the canonical row shape. |
| P08-G30 | LOW | Missing Coverage Gate | §7.18 `src/lib/rtl/icons.ts` module | §1.3 | `src/lib/rtl/icons.ts` (the `DirectionalChevron` helper) isn't on its own explicit coverage row separate from broad `src/lib/**`. |

## Phase 09 Gaps (16)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P09-G15 | CRITICAL | Missing Integration Test | §3 PrismaSyncCursorRepo upsert composite PK | §2.3 persistence-phase09 | No test asserts `bumpCursor` uses `upsert` (not insert-then-update) against composite PK `(entityId, deviceId)` -- a race between two devices on the same cursor would silently break monotonicity. |
| P09-G16 | CRITICAL | Missing Integration Test | §3 init-custom-sql.sql ordering after prisma db push | §2.3 persistence-phase09 | No test asserts `Dockerfile.dev` CMD runs `prisma db push` BEFORE `psql -f init-custom-sql.sql` -- reversed order leaves triggers/CHECKs unapplied with no boot error. |
| P09-G17 | CRITICAL | Missing Integration Test | §3 PrismaEntityRepo single $transaction per batch | §2.3 persistence-phase09 | No test asserts `dispatchEntity` wraps a push batch in `prisma.$transaction([...])` -- partial-batch failure must roll the whole batch back (phase-01 idempotency invariant). |
| P09-G18 | HIGH | Missing Contract Test | §3 healthz response shape adds migrationsApplied + version | §3.1 | Contract widening row covers `status` union only; no row validates the response includes `migrationsApplied: boolean` and `version: '0.1.0'` fields. |
| P09-G19 | HIGH | Missing Integration Test | §3 PrismaUserStore preserves sha256 hashing of refresh tokens | §2.3 persistence-phase09 | No test asserts refresh tokens are sha256'd BEFORE persistence (phase-02 §7.21 invariant) -- swap could regress to plaintext storage silently. |
| P09-G20 | HIGH | Missing Integration Test | §3 PrismaUserStore preserves Argon2id password hashes | §2.3 persistence-phase09 | No test asserts password hashes survive the swap as Argon2id (not bcrypt/plaintext) -- bootstrap path could silently downgrade. |
| P09-G21 | HIGH | Missing E2E Scenario | §3 pull ordering: updatedAt asc then id asc | §4.1 / §2.3 | No test asserts Prisma pull queries use the stable secondary `id asc` sort -- ties on identical `updatedAt` could paginate non-deterministically. |
| P09-G22 | HIGH | Missing Integration Test | §3 auth-services bootstrap path persists to Postgres | §2.3 | No test asserts the bootstrap superadmin NOW persists to Postgres after swap and is idempotent across restarts -- a second boot must not create duplicate admins. |
| P09-G23 | HIGH | Missing Integration Test | §3 tenant isolation under PrismaEntityRepo | §6.7 / §2.3 | §6.7 mentions static-analysis check for `entityIdTenant` on every findMany but no concrete test row enumerates the 15 syncable models and asserts each one's where clause. |
| P09-G24 | MEDIUM | Missing Edge Coverage | §5 Postgres image pinned to 16.4-alpine | §6.6 / §2.3 | No test asserts `docker-compose.yaml` pins `postgres:16.4-alpine` (per §7.1 decision) -- floating `:16-alpine` tag drift would not be caught. |
| P09-G25 | MEDIUM | Missing Integration Test | §3 metrics.ts hide:true rationale documented | §2.3 | No test asserts `routes/metrics.ts` carries the in-source comment explaining `hide:true` rationale -- future audits will re-flag without it. |
| P09-G26 | MEDIUM | Missing Integration Test | §5 docker-compose volume mounts for hot-reload | §2.3 / §4.1 | No test asserts compose mounts `./src:/app/src` and `./prisma:/app/prisma` so dev edits propagate -- silent loss makes dev iteration painful. |
| P09-G27 | MEDIUM | Missing Edge Coverage | §3 refresh-token retention vacuum | §6.8 / §2.3 | No test asserts revoked tokens older than `JWT_REFRESH_TTL_SECONDS` are pruned -- unbounded growth on the `RefreshToken` table is a leak. |
| P09-G28 | MEDIUM | Missing Persona / Manual Step | §9.11 §1.2 sidebar decision recorded | §5 / §1.2 | The §9.11 doc-check test reads phase-09 §7 for the recorded decision, but no manual script lists "open phase-09.md §7 #5 and verify decision row populated" -- helper invariant has no manual gate. |
| P09-G29 | LOW | Missing Snapshot | §3 docker-compose canonical snapshot | §3.3 | No snapshot path locks `docker-compose.yaml` byte-hash -- a stealth edit to env vars or volumes ships without review. |
| P09-G30 | LOW | Missing Coverage Gate | §3 PrismaEntityRepo `lwwShouldApply` branch coverage | §1.3 | §1.3 lists Prisma repo files at 90% but no explicit row asserts branch coverage on the LWW helper `lwwShouldApply`. |

## Cross-Phase Patterns

These show up in 3+ phases and are worth closing once with a shared helper rather than 9 times in 9 files:

1. **Pass 1 §9.x rows that themselves carry holes or contradictions.** P01-G19 (P01-G07's compound PK is wrong), P01-G20 (P01-G01 emits a `metrics_events.kind` not in the CHECK list), P02-G14 (P02-G02 enumerates 6 of 12 audit-action literals), P08-G23 (P08-G04 schema declares 12 values while §1.1 asserts 14). Lesson: every Pass 1 §9 row that *enumerates* a closed set must be paired with a Pass 2 row asserting the set is *exhaustive*.
2. **Hook REGISTRATION vs hook BEHAVIOUR.** P06-G08 (`SyncPullService` inventory callback), P04-G12 (`pulledAt` write-back), and P02-G19 (Tauri plugin registrations). Pass 1 verified what the hook does when called; Pass 2 verifies the hook is wired. A single helper-style integration test `asserts_callback_registered_in_sync_pull_service(callback_name, entity)` would close them.
3. **Per-entity policy / TENANT_MODELS / index enumeration.** P03-G27 (8-entity LWW registry), P05-G26 (3 Visit indexes), P09-G23 (15-model tenant isolation), P04-G18 (operator_shifts_today index). One parametrised contract test per registry that walks the full enumeration would close every row.
4. **Snapshot completeness.** P01-G31, P02-G26, P03-G29, P04-G14, P04-G23, P07-G20, P08-G29, P09-G29 -- every phase has at least one missing snapshot. The canonical-error-envelope snapshot (P01-G31) is the highest leverage: every phase's error paths inherit it.
5. **Push round-trip on phase-local entities that piggyback on `audit_log`.** P07-G16 (`daily_close_run` audit row pushes despite "reports don't push"), P02-G18 (`logout` audit), P04-G15 (shifts `soft_delete` outbox shape). Worth a one-time "every audit-emitting action round-trips" test wired in phase-08 or phase-09.

## Critical Gate

The 8 Pass 2 critical gaps must land in their phase plans before first test-authoring on that phase:

- Phase 01: P01-G19, P01-G20.
- Phase 06: P06-G08.
- Phase 08: P08-G15, P08-G16.
- Phase 09: P09-G15, P09-G16, P09-G17.

P01-G19 is special: it does not just add a missing test, it CORRECTS a Pass 1 §9.7 row whose contract assertion would falsely pass against a non-existent schema invariant. Apply it as a §10.x *replacement* of §9.7's compound-PK clause, not as a new addition.

## Next Steps (per `.claude/rules/planning.md` Gap Analysis Methodology)

1. **Apply gaps to the owning `phase-XX-test.md`** as `§10.x` additions (one subsection per gap, copy-paste-ready test row or scenario), mirroring how Pass 1 landed as `§9.x`. Same severity / category metadata; reference the gap ID. For P01-G19, amend §9.7's compound-PK clause in place rather than adding a new contradicting row. **[DONE 2026-05-13. All 132 subsections appended; counts verified per phase: 14/14/16/14/18/10/14/16/16. P01-G19 amended in §9.7 in place with a dated correction note and tracked as §10.1 for audit-trail purposes.]**
2. **Re-run Pass 3** after additions land. The cycle ends when a pass finds zero true gaps.
3. **Critical gaps gate first execution.** Test plans cannot move to first test-authoring until P01-G19, P01-G20, P06-G08, P08-G15, P08-G16, P09-G15, P09-G16, P09-G17 land in their phase plans. (Landed in their §10.x rows.)
4. **Apply the four cross-phase helpers** from "Cross-Phase Patterns" once each; they close ~25% of Pass 2 rows in a single sweep.
5. **Update `testing-status.md` Gap Analysis Summary** with the Pass 2 row: `2026-05-13 \| 132 \| 8 \| 48 \| 57 \| 19 \| complete (additions landed as §10.x)`.
