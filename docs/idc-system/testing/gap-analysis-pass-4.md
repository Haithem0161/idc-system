# Gap Analysis Pass 4 -- Test Plans vs Build Specs

_Date: 2026-05-13_

Pass 4 re-compares every `docs/idc-system/phase-XX.md` build spec against the matching `docs/idc-system/testing/phase-XX-test.md` test plan AFTER all §9.x (Pass 1), §10.x (Pass 2), AND §11.x (Pass 3) additions landed. The union of §1-§6 + §9 + §10 + §11 is what is "covered." A gap is a scenario, command, route, schema rule, sync contract, conflict policy, snapshot, performance SLO, coverage gate, or edge case that the build spec promises but the union still does not verify.

Cross-cutting items legitimately delegated to `security.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`, or another phase plan (and listed in the owning test plan header's `Out of scope` line) are NOT counted.

Methodology per `.claude/rules/planning.md` Gap Analysis Methodology (Pass 2+) applied to the testing surface defined in `.claude/rules/testing.md` §3-§6.

## Pass 4 Totals

| Phase | Total | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing | 3 | 0 | 1 | 1 | 1 |
| 02 Authentication & Users | 4 | 0 | 1 | 2 | 1 |
| 03 Catalog & Reference Data | 2 | 0 | 1 | 0 | 1 |
| 04 Operator Shifts | 2 | 0 | 0 | 1 | 1 |
| 05 Reception & Visit Lock | 4 | 0 | 1 | 3 | 0 |
| 06 Inventory Operations | 1 | 0 | 1 | 0 | 0 |
| 07 Accounting & Reports | 3 | 0 | 0 | 2 | 1 |
| 08 Audit, Conflict Resolver & Polish | 4 | 0 | 1 | 2 | 1 |
| 09 Pre-Ship Hardening | 3 | 0 | 0 | 2 | 1 |
| **Total** | **26** | **0** | **6** | **13** | **7** |

Pass-counter trajectory: **112 -> 132 -> 64 -> 26.** Pass 4 cut residual gaps by another 59% and -- decisively -- finds **zero criticals**. Every Pass 4 gap is either a leaf-level invariant pin (Low/Medium) or a server-side parity row mirroring a local-side row already covered (High). The methodology is converging cleanly. Pass 5 is expected to find under 10 residuals and the closing pass to find zero.

Severity rubric: CRITICAL = safety/correctness invariant; HIGH = major user-facing flow or business rule; MEDIUM = edge case the build spec calls out; LOW = cosmetic, advisory, or coverage-gate / snapshot omission.

## Phase 01 Gaps (3)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P01-G40 | HIGH | Missing Integration Test | §4 SyncEngine push step 2.2 -- `X-Device-Id` + `X-App-Version` headers | §12 / §2.1 / §2.3 | The two custom request headers `X-Device-Id` and `X-App-Version` that §4 mandates on every push are never asserted. §2.3 row implies server reads `originDeviceId from header` without naming the header; no client-side test pins the engine emits these names with the boot-resolved `sync_state.device_id` and the `Cargo.toml` package version. |
| P01-G41 | MEDIUM | Missing Edge Coverage | §6.7 + §7.20 `/sync/lookup-op` cross-tenant negative | §12 / §2.3 | §6.7 claims cross-tenant isolation is "Asserted in §2.3," but §2.3 row `lookup_op_returns_found_op_ids_only` only says "Tenant-scoped" without a cross-tenant negative scenario. No test seeds an op for tenant B, requests it under tenant A's JWT, and asserts the response excludes it (an existence-oracle leak vector). |
| P01-G42 | LOW | Missing Edge Coverage | §7.4 SyncPill count badge + design-system §5.4 tabular numerals rule | §12 / §2.4 | testing.md §14 anti-pattern "Numeric columns without `tnum` assertion" is mandatory. The `<SyncPill>` count badge in §2.4 is tested for render + color + Arabic-Indic shaping but never asserts `font-feature-settings: 'tnum'` or Geist Mono family on the badge element. A Tailwind drift dropping the mono-numeric class would shift digit width on every state transition undetected. |

## Phase 02 Gaps (4)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P02-G35 | HIGH | Missing Integration Test | §7.24 -- "Pull payload from server EXCLUDES `password_hash` for all consumers. Local row retains its existing hash." | §12 / §2.1 | No test asserts that applying a `users` pull row to a locally-cached user with an existing `password_hash` PRESERVES the local hash byte-for-byte; only the cache-clear-on-server-hash-change branch (§7.27) is covered. A regression that wrote `NULL` into `password_hash` on pull-apply would leave §2.1 green while breaking offline login on the next relaunch. |
| P02-G36 | MEDIUM | Missing Integration Test | §1 settings seed default-value table | §12 / §2.1 | `migration_002_seeds_v1_required_settings_idempotently` asserts the 10 required keys exist but never asserts the seeded default **values** match the §1 table (`dye_cost_iqd=10000`, `report_cost_iqd=10000`, `internal_doctor_pct=30`, `idle_lock_minutes=10`, `arabic_numerals=false`, `currency_symbol='د.ع'`, `thermal_width=32`, `thermal_printer_name=''`, both `clinic_display_name_*` empty). A regression that seeded `dye_cost_iqd=0` would still pass. |
| P02-G37 | MEDIUM | Missing Integration Test | §3 Server `AuthService::login` step 1 -- case-insensitive email lookup | §12 / §2.3 | No `/auth/login` test asserts case-insensitive email match: a user seeded with `email='test@example.com'` MUST log in successfully when the client posts `email='Test@Example.COM'`. The lowercase normalization is asserted at the `User::try_new` write path but the read-path lookup (`getByEmail(entityId, email.toLowerCase())`) has no integration coverage. |
| P02-G38 | LOW | Missing Unit Test | §3 Tauri `users::reset_password { id, newPassword }` + parity with `LoginSchema.password.min(8)` | §12 / §1.2 / §2.2 | No `ResetPasswordSchema` (or `users::reset_password` IPC arg validator) test asserts `newPassword` enforces the same `z.string().min(8)` rule as `LoginSchema`. `<ResetPasswordModal>` test mentions "Zod min-8 input" but no schema-layer assertion exists, so a regression dropping the min-8 rule at the IPC boundary would let `users::reset_password { newPassword: "x" }` succeed. |

## Phase 03 Gaps (2)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P03-G40 | HIGH | Missing Contract Test | §7.36 server-side role gate on `/sync/push` catalog mutations | §12 / §2.3 | §6.7 narrates "Server-side role gate on `/sync/push` mutations -- non-superadmin JWT pushing a `check_types` mutation -> 403. Defence in depth." but no §2.3 row asserts it. IPC `non_superadmin_returns_forbidden` rows cover the client-side commands; the server-side push-time role check has no contract test. A regression collapsing the server's role-gate to authentication-only would silently allow a forged-role JWT to mutate catalog rows during sync. |
| P03-G41 | LOW | Missing Contract Test | §7.21 server-side `DUPLICATE_CONSUMPTION_ROW` typed error code | §12 / §2.3 | §2.3 asserts the doctor_pricing analogue with `error.code = 'DUPLICATE_PRICING_ROW'` (per §7.20) but the consumption_map rows only assert blocking, not the typed error code envelope. §3.2's `AppError` envelope contract requires every named error variant carry its `kind` on the wire; without pinning `error.code = 'DUPLICATE_CONSUMPTION_ROW'`, a regression collapsing the typed error to a generic 422 would slip past every existing row. Asymmetric coverage with §7.20. |

## Phase 04 Gaps (2)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P04-G27 | MEDIUM | Missing Integration Test | §3 Frontend hooks table -- `useShiftEdit` cache invalidation | §12 / §2.4 | Sibling mutation hooks each have an `*_invalidates_all_shifts_keys` row in §2.4 (clock_in / clock_out / soft_delete) but `useShiftEdit` has only note-shape tests; no test asserts a successful retro-edit invalidates `['shifts']` so `<OnShiftTable>` / `<ShiftHistoryToday>` refresh. A regression dropping invalidation on the edit path would leave the on-shift table stale after a superadmin retro-edit. Closed-set exhaustiveness blind spot per Pass 3 cross-phase pattern #2. |
| P04-G28 | LOW | Missing Integration Test | §7.5 `<ShiftsPage>` `<Empty action="Clock in">` CTA | §12 / §2.4 | §7.5 mandates the empty state carries a "Clock in" CTA wired to open `<ClockInDialog>`. §2.4 bullet asserts the EMPTY container renders but never pins the CTA button: its `data-testid`, the `reception.shifts.empty.clock_in` i18n key, or that clicking it opens the dialog. P04-G05 closed the ErrorState Retry button at the same level; the Empty CTA is its symmetric twin and uncovered. |

## Phase 05 Gaps (4)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P05-G42 | HIGH | Missing Integration Test | §4 Sync Server Visit push acceptance step 4 -- audit-first for the `void` push variant | §12 / §2.3 | §11.1 P05-G30 added server audit-first ordering for the `lock` push; the symmetric `void` push acceptance (locked -> voided transition writing `action='void'` audit row + offsetting `inventory_adjustments` inserts in the same Prisma tx) has no parallel scenario. A regression that wired the void server-side acceptPush through a single-pass write (audit after the upsert) would pass every existing test because they all exercise the lock branch. |
| P05-G43 | MEDIUM | Missing Integration Test | §1 visits indexes `visits_doctor` and `visits_operator` (partial WHERE clauses) | §12 / §2.1 | §2.1 row asserts `visits_check_type` index usage; the partial-with-WHERE indexes `visits_doctor` and `visits_operator` (load-bearing for phase-07 doctor/operator drill-downs) have no `EXPLAIN QUERY PLAN` assertion. A migration regression that dropped the WHERE predicate or the index would force phase-07 reports to full-scan and only fail at scale. |
| P05-G44 | MEDIUM | Missing Setup | §5 Infrastructure + §7.45 -- `tauri-plugin-shell` plugin registration in `lib.rs::run()` | §12 / §2.1 | §9.9 P05-G09 asserts the `shell:allow-execute` capability scope; no test asserts the plugin is registered via `.plugin(tauri_plugin_shell::init())`. The capability lint passes even if the plugin is never built into the binary; at runtime `settings::list_printers` / `receipts::print_pdf` would return `plugin not found`. Parallel of phase-01 P01-G33 plugin-registration assertion. |
| P05-G45 | MEDIUM | Missing Edge Coverage | §7.8 DB CHECK `length(trim(void_reason)) >= 5` on `visits` for `status='voided'` | §12 / §2.1 / §6.8 | §1.1 + §1.2 + §7.14 + §2.4 cover the entity / Zod / server-revalidate / UI layers, but §6.8 enumerates CHECK constraints without a raw `INSERT INTO visits (status, void_reason, ...) VALUES ('voided', 'oops', ...)` test exercising the §7.8 DB-layer CHECK. A sync-apply path that bypassed service validators could land a short `void_reason` if the CHECK migration regressed. |

## Phase 06 Gaps (1)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P06-G22 | HIGH | Missing Integration Test | §7.11 step 3.2 + §7.3 -- server-side audit `delta: { before, after, reason }` payload shape | §12 / §2.3 | Server-side audit row written by `acceptPush` for the `inventory_items` update must carry the same `delta: { before, after, reason }` JSON payload that §9.1 (P06-G01) locks for the LOCAL writer; §11.1 P06-G18 only asserts the two audit rows exist in audit-first order and pins action/entity/entity_id, never inspecting the `delta` payload shape. A server emitter that writes stub `delta: {}` (or omits `reason`) passes every existing test, silently breaking local/server audit-log parity across all four reasons. |

## Phase 07 Gaps (3)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P07-G37 | MEDIUM | Missing Integration Test | §7.22 + §7.15 -- Top Operators by Visits card -> `/accounting/operators/:id` drill-down | §12 / §2.4 | §9.5 pins top-doctor and top-check-type card click navigation but the third Top-5 card (Top Operators by Visits) has no click-handler test; a regression that broke `<DashboardTops>` top-operator row routing to `/accounting/operators/:id` (or to a wrong query string) slips through. |
| P07-G38 | MEDIUM | Missing Integration Test | §7.24 cursor format -- "opaque base64 of `{ lockedAt, visitId }`" | §12 / §3.1 | §9.3 verifies the cap (10000) and `nextCursor==null` on final page, but the cursor shape itself -- opacity (base64-encoded JSON), exact key set `{ lockedAt, visitId }`, and round-trip stability (decode -> re-encode yields identical bytes) -- is unverified; a regression emitting plaintext `<visitId>` or extra fields would slip past existing pagination tests. |
| P07-G39 | LOW | Missing Integration Test | §4 Frontend `<DailyCloseLayout>` step 2 -- KPIs side by side with deltas AND percentages | §12 / §2.4 | Component tests assert "today / prior with deltas" but the explicit percentage rendering (e.g. `+14%` next to the absolute delta) on every KPI tile is unpinned; a regression that hid the percent column or rendered raw decimals (0.14 instead of 14%) is invisible. |

## Phase 08 Gaps (4)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P08-G39 | HIGH | Missing Edge Coverage | §5 Soak Harness steps 4-5 -- "all rows arrive on the server within 5 minutes" + "zero outbox rows remain" | §12 / §6.6 / §4.3 | §6.6 caps steady-state depth (<=800) and drain throughput (>=50 ops/sec) and §9.2 captures the report, but no test asserts the BUILD-SPEC SLO that the post-reconnect drain completes WITHIN 5 MINUTES wallclock AND that the outbox terminal state reaches EXACTLY 0 rows after drain. A regression leaving even 1 unsynced row, or completing in 6 minutes, satisfies every existing assertion. |
| P08-G40 | MEDIUM | Missing Integration Test | §3 Tauri "Register the new commands in `src-tauri/src/lib.rs::generate_handler!`" | §12 / §2.2 | The four phase-08 commands (`audit::query`, `audit::vacuum_now`, `diagnostics::summary`, updated `sync::list_conflicts`) need an explicit `lib.rs::generate_handler!` registration assertion (mirror of P01-G33 pattern from Pass 3). §2.2 only tests command behavior; a missing handler-macro entry compiles but fails at runtime on first invocation. |
| P08-G41 | MEDIUM | Missing E2E Scenario | §4 Frontend `<ConflictResolverPanel>` step 4 -- `<MergeEditor>` "edit manually" branch | §12 / §4.1 / §2.4 | E2E specs cover pick-local + pick-server radio paths but neither exercises the "edit manually" typed-input path -- the third branch in MergeEditor's per-field affordance. Users typing a custom value that matches neither side has no coverage; canonicalization at the resolve_op_id boundary (§10.2) is untested for manually-edited merged payloads. |
| P08-G42 | LOW | Missing Edge Coverage | §5 "v1 does NOT introduce BullMQ" | §12 / §6.7 / §3.1 | Negative-scope invariant: phase-08 explicitly forbids BullMQ in v1; the audit vacuum is a Tokio task ONLY. No test grep-asserts `sync-server/package.json` is free of `bullmq` AND that no Fastify plugin registers a queue worker. A regression that imported BullMQ for a "background audit job" would silently drift the server runtime profile. |

## Phase 09 Gaps (3)

| ID | Severity | Category | Build spec ref | Test plan section to add to | One-line gap |
|-|-|-|-|-|-|
| P09-G35 | MEDIUM | Missing Integration Test | §3 Sync Server "Error-handler reach" -- both `AUTH_INVALID_REFRESH` AND `AUTH_EXPIRED_REFRESH` 401 mappings | §12 / §2.3 | P09-G19 (env-schema) asserts the legacy memory path throws `DomainError('AUTH_INVALID_REFRESH', 401)`, but the build spec at §3 names both codes. The expired-not-revoked branch (`expiresAt < now`, `revokedAt IS NULL`) is never independently exercised; a regression collapsing both into a single code (or into a 500) would pass every existing row. |
| P09-G36 | MEDIUM | Missing Contract Test | §3 Sync Server auth-jwt rewrite -- `verify: { algorithms: ['RS256'] }` registration option | §12 / §3.1 / §2.3 | The rewrite explicitly passes `verify: { algorithms: ['RS256'] }` to `fjwt` in the production branch. Existing P09 tests assert RS256 token acceptance and wrong-key rejection at the token layer, but none assert the `algorithms` allowlist option itself was passed -- a regression that registered fjwt without it (default verify permits `none` and HS variants) would still pass token-level rows. A spy on `fastify.register(fjwt, options)` capturing the option object closes the contract. |
| P09-G37 | LOW | Missing Unit Test | §3 Sync Server `prisma.ts` -- `log: NODE_ENV === 'development' ? ['warn','error'] : ['error']` branch | §12 / §1.1 | Build spec pins PrismaClient `log` config by `NODE_ENV`. §1.3 coverage gate and §2.3 plugin tests assert decoration + onClose but never the log-level branch shape. A regression flipping the production branch to `['query','info','warn','error']` would leak SQL into prod logs (privacy + log volume) and lines coverage alone would not catch it -- both branches execute under their respective env. |

## Cross-Phase Patterns

Two patterns dominate Pass 4. Both are tightening passes over coverage already in place; neither names a brand-new surface.

1. **Server-side parity of local-side invariants (HIGH-tier).** P06-G22 (server audit `delta` payload shape mirroring local), P05-G42 (server audit-first for `void` push mirroring `lock` push), P02-G35 (pull-apply preserves local `password_hash`). Each is a leaf-level mirror of an already-tested local-side invariant. A single shared test helper that walks every entity's audit-emitting acceptPush path and asserts (a) ordering, (b) `delta` shape, (c) idempotent payload preservation would close the family.

2. **Closed-set exhaustiveness still surfacing residuals.** P04-G27 (`useShiftEdit` cache invalidation -- the one mutation hook out of four with no invalidation row), P07-G37 (Top Operators card -- the one of three top cards without a click test), P08-G40 (handler-macro registrations for phase-08 commands), P05-G44 (plugin registration for `tauri-plugin-shell`). The methodology's recurring blind spot from Passes 2-3 generates one more residual per phase.

## Critical Gate

**Pass 4 finds zero criticals.** First-test-authoring gates from Passes 1-3 still apply (Pass 3's single critical P03-G32 server-side `entityIdTenant` injection remains the highest-priority outstanding item), but Pass 4 adds none.

## Convergence Note

Pass-counter trajectory: **112 -> 132 -> 64 -> 26.** The cycle has effectively converged. Pass 5 is expected to find:
- Zero criticals (consistent with Pass 4).
- Fewer than 10 High+Medium combined.
- A handful of Lows -- tertiary leaves.

The "0 true gaps" cycle-termination gate from `.claude/rules/planning.md` Gap Analysis Methodology remains in effect. Run Pass 5 as the closing pass; if zero, the cycle is formally complete.

## Next Steps (per `.claude/rules/planning.md` Gap Analysis Methodology)

1. **Apply gaps to the owning `phase-XX-test.md`** as `§12.x` additions (one subsection per gap, copy-paste-ready test row or scenario), mirroring how Pass 1 landed as `§9.x`, Pass 2 as `§10.x`, Pass 3 as `§11.x`. Same severity / category metadata; reference the gap ID. **[DONE 2026-05-13. All 26 subsections appended; counts verified per phase: 3/4/2/2/4/1/3/4/3.]**
2. **Re-run Pass 5** after additions land. The cycle ends when a pass finds zero true gaps.
3. **Apply the two cross-phase helpers** from "Cross-Phase Patterns" once each; they close ~50% of Pass 4 rows in a single sweep.
4. **Update `testing-status.md` Gap Analysis Summary** with the Pass 4 row: `2026-05-13 \| 26 \| 0 \| 6 \| 13 \| 7 \| complete (additions landed as §12.x)`.
