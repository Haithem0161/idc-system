# IDC System v0.1.x Testing Status

_Last updated: 2026-05-13 (**Gap Analysis cycle CLOSED.** Pass 5 found 0 gaps across all 9 phases. Trajectory 112 -> 132 -> 64 -> 26 -> 0. 334 total gap-derived test rows landed across §9 / §10 / §11 / §12 of each phase plan. No tests authored or tooling installed yet -- test execution begins per phase under `.claude/rules/testing.md` §15). Source: [.claude/rules/testing.md](../../../.claude/rules/testing.md)._

This file tracks the testing cycle the same way `status.md` tracked the build cycle. A phase flips to `complete` only when its `phase-XX-test.md` §8 Definition of Done is fully checked. See `.claude/rules/testing.md` §5 + §11.

## Phase Status Table

| # | Phase | Status | Started | Completed | Unit | Integration | Contract | E2E | Manual | Coverage % | Open Defects |
|-|-|-|-|-|-|-|-|-|-|-|-|
| 01 | Foundation & Sync Plumbing | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 02 | Authentication & Users | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 03 | Catalog & Reference Data | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 04 | Operator Shifts | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 05 | Reception & Visit Lock | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 06 | Inventory Operations | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 07 | Accounting & Reports | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 08 | Audit, Conflict Resolver & Polish | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 09 | Pre-Ship Hardening | in_progress | 2026-05-13 | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |

Status values: `not_started`, `in_progress`, `complete`. Counts are absolute (number of tests or scenarios), not boolean. Coverage % is the lowest across the phase's owned source paths.

Existing test inventory (pre-cycle baseline, to be folded into the per-phase counts as plans are written):
- Rust unit (`#[cfg(test)]`): 18 modules.
- Rust integration (`src-tauri/tests/`): 7 files (`sync_phase01.rs`, `catalog_phase03.rs`, `shifts_phase04.rs`, `visits_phase05.rs`, `inventory_phase06.rs`, `reports_phase07.rs`, `audit_phase08.rs`). 74 tests total reported by the build cycle.
- Sync server: `test/helper.ts` only; `test/plugins/` and `test/routes/` empty.
- Frontend: zero.
- E2E: zero.
- Contract: zero.

## Cumulative Totals

| Metric | Before | Current | Target |
|-|-|-|-|
| Rust unit tests | -- | 0 | TBD per phase plan |
| Rust integration tests | -- | 0 | TBD per phase plan |
| TS unit tests (Vitest) | 0 | 0 | TBD per phase plan |
| Component tests (RTL) | 0 | 0 | TBD per phase plan |
| Server route tests | 0 | 0 | 16 (one per route) |
| Contract tests (Swagger + IPC + envelope) | 0 | 0 | TBD per phase plan |
| E2E specs (WebdriverIO + tauri-driver) | 0 | 0 | TBD per phase plan |
| Persona scripts passing | 0 | 0 | 5 (see `personas.md`) |
| Snapshot / golden files | 0 | 0 | A5 receipt + thermal + daily-close + sync envelope samples |
| Rust domain coverage % | -- | -- | >= 90 |
| Rust sync engine coverage % | -- | -- | >= 95 |
| Rust infra coverage % | -- | -- | >= 75 |
| Frontend domain coverage % | -- | -- | >= 90 |
| Frontend presentation coverage % | -- | -- | >= 60 |
| Sync server domain coverage % | -- | -- | >= 90 |
| Sync server routes coverage % | -- | -- | >= 85 |
| Open defects P0 | 0 | 0 | 0 |
| Open defects P1 | 0 | 0 | 0 |
| Open defects P2 | 0 | 0 | TBD |
| Open defects P3 | 0 | 0 | TBD |

`--` indicates "not yet measured." `Target` is set per `.claude/rules/testing.md` §8 + §9; per-phase plans MAY override perf SLOs with a documented reason but MAY NOT relax coverage gates silently.

## Gap Analysis Summary

A test-coverage "gap" is a scenario that SHOULD exist according to the phase build spec but does NOT exist in the phase test plan or its referenced suites. Gap passes mirror the build cycle's gap analysis (`.claude/rules/planning.md` §Gap Analysis Methodology).

| Pass | Date | Gaps Found | Critical | High | Medium | Low | Status |
|-|-|-|-|-|-|-|-|
| 1 | 2026-05-13 | 112 | 12 | 35 | 44 | 21 | complete (additions landed as §9.x) |
| 2 | 2026-05-13 | 132 | 8 | 48 | 57 | 19 | complete (additions landed as §10.x) |
| 3 | 2026-05-13 | 64 | 1 | 19 | 34 | 10 | complete (additions landed as §11.x) |
| 4 | 2026-05-13 | 26 | 0 | 6 | 13 | 7 | complete (additions landed as §12.x) |
| 5 | 2026-05-13 | 0 | 0 | 0 | 0 | 0 | **complete (CYCLE CLOSED -- no additions needed)** |

**Gap Analysis cycle FORMALLY CLOSED.** Pass 5 found zero true gaps across all 9 phases. The methodology gate from `.claude/rules/planning.md` Gap Analysis Methodology -- "Continue passes until a pass finds 0 true gaps" -- is satisfied. Full Pass 5 log: [`gap-analysis-pass-5.md`](gap-analysis-pass-5.md). Pass-counter trajectory: **112 -> 132 -> 64 -> 26 -> 0**. 334 total gap-derived test rows landed across §9 / §10 / §11 / §12 of each phase plan (counts per phase: 42 / 38 / 41 / 28 / 45 / 22 / 39 / 42 / 37). Every build-spec promise across §3 (DDD), §4 (Business Logic), §6 (Verification), and §7 (PRD Gap Additions) for all 9 phases has at least one verification scenario in the union of §1-§6 + §9 + §10 + §11 + §12. **Test execution begins per phase under `.claude/rules/testing.md` §15.**

### Per-Phase Distribution (Pass 1)

| Phase | Gaps | Critical | High | Medium | Low |
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
| Total | 112 | 12 | 35 | 44 | 21 |

The 12 Critical gaps cluster in three areas: (1) sync / runtime invariants the plans assert structurally but not behaviourally (P01-G01 JWT key pinning, P01-G02 capability shape, P09-G01 autoload deps, P09-G02 prisma onClose, P09-G04 conflict resolve tx-API split); (2) audit-row payload completeness vs ordering -- ordering is tested everywhere, payload `delta` shape is missed (P05-G01 void audit-first, P06-G01 adjustment delta shape, P07-G01 daily_close_run delta payload); (3) idempotency and persistence end-states (P07-G02 PDF filename no-overwrite, P08-G01 server resolve ProcessedOp short-circuit, P08-G02 soak report capture, P09-G03 refresh-token survives restart). These 12 gate first test authoring; the remaining 100 may be addressed in line as phases execute.

### Per-Phase Distribution (Pass 2)

| Phase | Gaps | Critical | High | Medium | Low |
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
| Total | 132 | 8 | 48 | 57 | 19 |

Pass 2's 8 Critical gaps cluster in two areas: (1) Pass 1 §9.x rows that themselves carry holes or contradict the build spec (P01-G19 §9.7 contract-tests the WRONG SyncCursor compound PK; P01-G20 §9.1 emits a `metrics_events.kind` literal not in the §7.28 CHECK list); (2) phase-09 Prisma-swap invariants that the §9.x rows tested as wired but not as correct (P09-G15 cursor upsert composite-PK race, P09-G16 init-custom-sql vs prisma-db-push ordering, P09-G17 single $transaction per push batch). Two more in phase-08 (P08-G15 vacuum self-audit single-row atomicity, P08-G16 client compute_resolve_op_id canonical JSON) and one in phase-06 (P06-G08 SyncPullService hook REGISTRATION vs behaviour). These 8 gate first test authoring on their respective phases.

### Per-Phase Distribution (Pass 3)

| Phase | Gaps | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing | 7 | 0 | 2 | 4 | 1 |
| 02 Authentication & Users | 8 | 0 | 4 | 3 | 1 |
| 03 Catalog & Reference Data | 8 | 1 | 2 | 4 | 1 |
| 04 Operator Shifts | 3 | 0 | 0 | 3 | 0 |
| 05 Reception & Visit Lock | 12 | 0 | 3 | 7 | 2 |
| 06 Inventory Operations | 4 | 0 | 2 | 2 | 0 |
| 07 Accounting & Reports | 10 | 0 | 2 | 5 | 3 |
| 08 Audit, Conflict Resolver & Polish | 8 | 0 | 3 | 4 | 1 |
| 09 Pre-Ship Hardening | 4 | 0 | 1 | 2 | 1 |
| Total | 64 | 1 | 19 | 34 | 10 |

Pass 3's single Critical gap is **P03-G32** -- server `/sync/push` MUST inject `entityIdTenant` from the JWT claim rather than payload; a regression accepting payload-supplied `entity_id` is a cross-tenant data leak. Pull-side filtering is covered (§2.3); push-side cross-tenant injection is the load-bearing multi-tenant invariant. Every other phase landed with zero Pass-3 criticals after §9 + §10 -- the methodology is converging. Pass 3's 19 High gaps cluster into three cross-phase patterns documented in `gap-analysis-pass-3.md`: (1) **server-side parity of local audit-first invariants** (P05-G30 visit-push audit-first, P06-G18 adjustment two-audit-rows, P03-G33 dirty=1 / version+1 across catalog mutations); (2) **closed-enum / registry exhaustiveness still incomplete** (P01-G33 6-of-7 plugin registrations, P01-G34 2-of-3 audit-log composite indexes, P05-G32 4-of-7 LockBlocker variants, P07-G34 3-of-4 CSV slugs, P05-G31 TENANT_MODELS new entries, P02-G29 users::list filter rule); (3) **service-layer responsibilities deferred to UI test plans by phase boundary** (P01-G38 ConflictResolveService audit emission, P03-G32 server-side `entityIdTenant` injection ownership).

### Per-Phase Distribution (Pass 4)

| Phase | Gaps | Critical | High | Medium | Low |
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
| Total | 26 | 0 | 6 | 13 | 7 |

Pass 4 finds **zero criticals** -- a decisive convergence signal. The 6 Highs are all server-side parity rows mirroring local-side invariants already tested (P05-G42 server-void audit-first, P06-G22 server delta payload shape, P02-G35 pull-apply preserves password_hash, P03-G40 server-side role gate on /sync/push, P01-G40 X-Device-Id / X-App-Version headers, P08-G39 soak 5-minute drain wallclock). The remaining 20 gaps (13 Medium + 7 Low) are leaf-level invariant pins: registry / handler-macro registrations (P05-G44 shell plugin, P08-G40 phase-08 commands), exhaustiveness residuals (P04-G27 useShiftEdit invalidation, P07-G37 top operators card), DB-CHECK / typed-error / index parity (P05-G45, P03-G41, P05-G43), and tertiary leaves (P09-G37 prisma log level, P07-G39 percentage rendering, P01-G42 SyncPill tnum).

### Per-Phase Distribution (Pass 5 -- CYCLE CLOSED)

| Phase | Gaps | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing | 0 | 0 | 0 | 0 | 0 |
| 02 Authentication & Users | 0 | 0 | 0 | 0 | 0 |
| 03 Catalog & Reference Data | 0 | 0 | 0 | 0 | 0 |
| 04 Operator Shifts | 0 | 0 | 0 | 0 | 0 |
| 05 Reception & Visit Lock | 0 | 0 | 0 | 0 | 0 |
| 06 Inventory Operations | 0 | 0 | 0 | 0 | 0 |
| 07 Accounting & Reports | 0 | 0 | 0 | 0 | 0 |
| 08 Audit, Conflict Resolver & Polish | 0 | 0 | 0 | 0 | 0 |
| 09 Pre-Ship Hardening | 0 | 0 | 0 | 0 | 0 |
| Total | 0 | 0 | 0 | 0 | 0 |

Pass 5 is the closing pass. All 9 phases report zero remaining gaps. The cycle is formally complete per the methodology gate. No §13.x additions land. Test execution begins per phase under `.claude/rules/testing.md` §15.

### Cumulative Per-Phase Coverage After Cycle Closure

| Phase | Pass 1 (§9) | Pass 2 (§10) | Pass 3 (§11) | Pass 4 (§12) | Pass 5 | Total Subsections | Last Gap ID |
|-|-|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing | 18 | 14 | 7 | 3 | 0 | 42 | P01-G42 |
| 02 Authentication & Users | 12 | 14 | 8 | 4 | 0 | 38 | P02-G38 |
| 03 Catalog & Reference Data | 15 | 16 | 8 | 2 | 0 | 41 | P03-G41 |
| 04 Operator Shifts | 9 | 14 | 3 | 2 | 0 | 28 | P04-G28 |
| 05 Reception & Visit Lock | 11 | 18 | 12 | 4 | 0 | 45 | P05-G45 |
| 06 Inventory Operations | 7 | 10 | 4 | 1 | 0 | 22 | P06-G22 |
| 07 Accounting & Reports | 12 | 14 | 10 | 3 | 0 | 39 | P07-G39 |
| 08 Audit, Conflict Resolver & Polish | 14 | 16 | 8 | 4 | 0 | 42 | P08-G42 |
| 09 Pre-Ship Hardening | 14 | 16 | 4 | 3 | 0 | 37 | P09-G37 |
| **Total** | **112** | **132** | **64** | **26** | **0** | **334** | -- |

## Blockers & Notes

- 2026-05-13: **Gap Analysis cycle FORMALLY CLOSED.** Pass 5 found 0 true gaps across all 9 phases. The methodology gate from `.claude/rules/planning.md` Gap Analysis Methodology -- "Continue passes until a pass finds 0 true gaps" -- is satisfied. Full Pass 5 log at [`gap-analysis-pass-5.md`](gap-analysis-pass-5.md). Pass-counter trajectory: **112 -> 132 -> 64 -> 26 -> 0**. 334 total gap-derived test rows landed across §9 / §10 / §11 / §12 of each phase plan. Every build-spec promise across §3 (DDD), §4 (Business Logic), §6 (Verification), and §7 (PRD Gap Additions) for all 9 phases has at least one verification scenario in the union of §1-§6 + §9 + §10 + §11 + §12.
  - **Five-pass convergence pattern observed**: (a) Pass 1 surfaces obvious missing coverage and high-leverage invariants (12 criticals). (b) Pass 2 GROWS the count because §9 additions opened new surface to audit -- this is the methodology working, not failing. (c) Pass 3 halves the count as enumeration blind spots close (1 critical). (d) Pass 4 cuts another 59%, lands at zero criticals -- decisive convergence signal. (e) Pass 5 hits termination threshold.
  - **Three structural blind spots documented**: (1) server-side parity of local-side invariants (every audit-first ordering test that lived in Rust integration needed a Prisma-side mirror -- closed via §11 + §12); (2) closed-set / registry exhaustiveness (enumerating 1 of N variants needs an exhaustiveness pair -- "the one of three / four / seven not tested" pattern surfaced in every phase through Pass 4); (3) Pass 1 §9 rows that themselves opened new surface need explicit Pass 2 audit.
  - **What remains**: test execution. 0 tests authored, no tooling installed. All 9 phases are `in_progress` (started 2026-05-13). Per `.claude/rules/testing.md` §11 DoD, each phase advances to `complete` only when its §8 DoD checkbox list is fully checked -- including running every Pass 1-4 row that landed under it. The single highest-priority critical from Pass 3 (P03-G32 server-side `entityIdTenant` injection on /sync/push -- a cross-tenant data-leak invariant) gates first test authoring on phase-03.
  - **Pass 5 produced no §13.x additions.** Pass 5's role was to verify termination, not to add new rows. The cycle is closed.
- 2026-05-13: **Gap Analysis Pass 4 complete.** 26 gaps logged across the 9 phase test plans (0 Critical, 6 High, 13 Medium, 7 Low). Full per-gap log at [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md); per-phase distribution table above. Pass 4 cut residual gaps by 59% versus Pass 3 (26 vs 64) AND -- decisively -- found **zero criticals**. Cross-phase patterns surfaced (see `gap-analysis-pass-4.md` "Cross-Phase Patterns"):
  1. **Server-side parity of local-side invariants (HIGH-tier)** -- P06-G22 (server audit `delta` payload), P05-G42 (server audit-first for void push), P02-G35 (pull-apply preserves local password_hash). Each is a leaf-level mirror of an already-tested local invariant. A shared test helper walking every entity's audit-emitting acceptPush path would close the family in one stroke.
  2. **Closed-set exhaustiveness still surfacing residuals** -- P04-G27 (useShiftEdit, the one mutation hook of four with no invalidation row), P07-G37 (Top Operators, the one of three top cards without a click test), P08-G40 (handler-macro registrations for phase-08 commands), P05-G44 (plugin registration for tauri-plugin-shell). The methodology's recurring blind spot from Passes 2-3 generates one more residual per phase.
  - **Pass 4 finds zero criticals.** First-test-authoring gates from Passes 1-3 remain in effect (Pass 3's single critical P03-G32 server-side `entityIdTenant` injection is still the highest-priority outstanding item).
  - **Pass 4 additions landed (2026-05-13)**: §12.x sections appended to every phase test plan, one subsection per gap, total 26 subsections matching Pass 4 totals exactly (phase-01: 12.1-12.3; phase-02: 12.1-12.4; phase-03: 12.1-12.2; phase-04: 12.1-12.2; phase-05: 12.1-12.4; phase-06: 12.1; phase-07: 12.1-12.3; phase-08: 12.1-12.4; phase-09: 12.1-12.3).
  - **Pass-counter trajectory: 112 -> 132 -> 64 -> 26.** Pass 5 is expected to find under 10 residuals and may hit zero. The "0 true gaps" cycle-termination gate from `.claude/rules/planning.md` Gap Analysis Methodology remains in effect.
  - **Pass 5 trigger condition**: re-run gap analysis now that §12.x additions exist; a pass that finds zero true gaps ends the cycle. Pass 5 should re-compare phase-build-spec §3/§4/§7 against the union of `phase-XX-test.md` §1-§6 AND §9 AND §10 AND §11 AND §12. New gaps would land as §13.x additions on a sixth pass.
- 2026-05-13: **Gap Analysis Pass 3 complete.** 64 gaps logged across the 9 phase test plans (1 Critical, 19 High, 34 Medium, 10 Low). Full per-gap log at [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md); per-phase distribution table above. Pass 3 shrunk vs Pass 2 (64 vs 132) for two reasons that prove the methodology is converging: (a) Pass 2's exhaustiveness sweep already caught most enumeration holes -- Pass 3's remaining enumeration misses (e.g. plugin registrations 6-of-7, LockBlocker 4-of-7, CSV filename slugs 3-of-4) are smaller and clustered; (b) the focus areas that opened new surface in Pass 2 (hook REGISTRATION vs hook BEHAVIOUR, snapshot canonicalisation, push/pull symmetry) are mostly closed by §10.x rows -- Pass 3 surfaces only their residuals. Cross-phase patterns surfaced (see `gap-analysis-pass-3.md` "Cross-Phase Patterns"):
  1. **Server-side parity of local audit-first invariants** -- P05-G30, P06-G18, P03-G33. Local Rust `with_audit` audit-first ordering is asserted; the Prisma side on `/sync/push` is consistently uncovered. A shared `assert_server_acceptPush_writes_audit_first` helper invoked per syncable entity closes the family.
  2. **Closed-enum / registry exhaustiveness still incomplete** -- P01-G33, P01-G34, P05-G32, P07-G34, P05-G31, P02-G29. Pass 2's cross-phase pattern #1 ("closed-set enumeration needs exhaustiveness pair") repeats here -- it is the recurring blind spot of the methodology.
  3. **Service-layer responsibilities deferred to UI test plans by phase boundary** -- P01-G38 (ConflictResolveService.step 6 audit emission deferred to phase-08 but service-layer test still owns it), P03-G32 (server-side `entityIdTenant` injection lives in phase-01 sync plumbing but only tested in phase-03 catalog scope). When phase-N owns the service code, phase-N's test plan owns its assertions -- phase-M+ UI/E2E does not absorb them.
  - **1 Critical Pass 3 gap gates first test authoring**: P03-G32 (server `/sync/push` rewrites `entityIdTenant` from JWT, not payload). All remaining 63 gaps may land in line as phases execute.
  - **Pass 3 additions landed (2026-05-13)**: §11.x sections appended to every phase test plan, one subsection per gap, total 64 subsections matching Pass 3 totals exactly (phase-01: 11.1-11.7; phase-02: 11.1-11.8; phase-03: 11.1-11.8; phase-04: 11.1-11.3; phase-05: 11.1-11.12; phase-06: 11.1-11.4; phase-07: 11.1-11.10; phase-08: 11.1-11.8; phase-09: 11.1-11.4). Each subsection encodes the source build-spec section, the target test-plan section the new row(s) merge into during authoring, the gap category and severity, and a copy-paste-ready test scenario.
  - **Pass-counter trajectory: 112 -> 132 -> 64.** Pass 4 is likely to find under 25 gaps and zero criticals (extrapolating from the 51% drop in residual gap count). The "0 true gaps" cycle-termination gate from `.claude/rules/planning.md` Gap Analysis Methodology remains in effect; the predicted Pass 4 ceiling is informational, not a license to skip.
  - **Pass 4 trigger condition**: re-run gap analysis now that §11.x additions exist; a pass that finds zero true gaps ends the cycle. Pass 4 should re-compare phase-build-spec §3/§4/§7 against the union of `phase-XX-test.md` §1-§6 AND §9 AND §10 AND §11. New gaps would land as §12.x additions on a fifth pass.
- 2026-05-13: **Gap Analysis Pass 2 complete.** 132 gaps logged across the 9 phase test plans (8 Critical, 48 High, 57 Medium, 19 Low). Full per-gap log at [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md); per-phase distribution table above. Pass 2 found MORE gaps than Pass 1 (132 vs 112) for two intentional reasons: (a) the Pass 1 §9 additions themselves opened new surface -- eight Pass 2 rows attach directly to a Pass 1 row (e.g. P01-G19 catches that §9.7 contract-tests the wrong compound PK; P01-G20 catches that §9.1 emits a `metrics_events.kind` literal not in the CHECK list); (b) Pass 2 dug into focus areas Pass 1 historically misses -- field-by-field schema completeness, push/pull symmetry, per-entity policy registry enumeration, hook REGISTRATION (vs hook behaviour), and snapshot canonicalisation. Cross-phase patterns surfaced:
  1. **Pass 1 §9 rows that enumerate a closed set are paired with Pass 2 rows asserting exhaustiveness.** P01-G19, P01-G20, P02-G14, P08-G23. Every closed-set enumeration in §9 needs a paired "list is exhaustive" check.
  2. **Hook REGISTRATION vs hook BEHAVIOUR.** P06-G08, P04-G12, P02-G19. Pass 1 verified what each hook does when called; Pass 2 verifies the hook is actually wired at startup. A shared `asserts_callback_registered_in_sync_pull_service(callback_name, entity)` helper closes the family.
  3. **Per-entity policy / TENANT_MODELS / index enumeration.** P03-G27, P05-G26, P09-G23, P04-G18. One parametrised contract test per registry walking the full enumeration closes them all in one row.
  4. **Snapshot completeness.** P01-G31, P02-G26, P03-G29, P04-G14, P04-G23, P07-G20, P08-G29, P09-G29 -- every phase has at least one missing snapshot. Canonical-error-envelope snapshot (P01-G31) is the highest-leverage item since every phase's error paths inherit it.
  5. **Push round-trip on phase-local entities that piggyback on `audit_log`.** P07-G16 (`daily_close_run` audit row pushes despite "reports don't push"), P02-G18 (`logout` audit), P04-G15 (shifts `soft_delete` outbox shape). Worth a one-time "every audit-emitting action round-trips" test wired in phase-08 or phase-09.
  - **8 Critical Pass 2 gaps gate first test authoring**: P01-G19, P01-G20, P06-G08, P08-G15, P08-G16, P09-G15, P09-G16, P09-G17. P01-G19 is special -- apply as a §10.x *replacement* of §9.7's compound-PK clause, not as a new contradicting row.
  - **Pass 2 additions landed (2026-05-13)**: §10.x sections appended to every phase test plan, one subsection per gap, total 132 subsections matching Pass 2 totals exactly (phase-01: 10.1-10.14; phase-02: 10.1-10.14; phase-03: 10.1-10.16; phase-04: 10.1-10.14; phase-05: 10.1-10.18; phase-06: 10.1-10.10; phase-07: 10.1-10.14; phase-08: 10.1-10.16; phase-09: 10.1-10.16). Each subsection encodes the source build-spec section, the target test-plan section the new row(s) merge into during authoring, the gap category and severity, and a copy-paste-ready test scenario. P01-G19 is special: §9.7 of phase-01-test.md was AMENDED in place (with a dated correction note) to fix the wrong SyncCursor compound-PK invariant the Pass 1 row originally tested; §10.1 of the same file is a short tracking entry pointing back at the amended §9.7.
  - **Pass 3 trigger condition**: re-run gap analysis now that §10.x additions exist; a pass that finds zero true gaps ends the cycle. Pass 3 should re-compare phase-build-spec §3/§4/§7 against the union of `phase-XX-test.md` §1-§6 AND §9 AND §10. New gaps would land as §11.x additions on a fourth pass.
- 2026-05-13: **Gap Analysis Pass 1 complete.** 112 gaps logged across the 9 phase test plans (12 Critical, 35 High, 44 Medium, 21 Low). Full per-gap log at [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md); per-phase summary and methodology reference in the Gap Analysis section above. Each gap is keyed `P<NN>-G<NN>`, references the build-spec section that promised the behaviour, and names the test-plan section that should host the new test row. Three cross-cutting patterns surfaced:
  1. **Audit-payload completeness vs ordering** -- every audit-first test asserts ROW ORDER (audit before business before outbox) but no test asserts the audit row's `delta: { before, after, ... }` payload SHAPE. Affects phase-05 (void), phase-06 (adjustment), phase-07 (daily_close_run). A shared `assert_audit_payload_shape` helper exercised once per phase closes the family in one stroke.
  2. **TENANT_MODELS membership** -- phase-03, phase-04, phase-08 each lack a contract test that the new entities are registered in the server's tenant-isolation array. A single phase-08-test contract test ranging across the v0.1.0 final 15-entry list closes them in one row.
  3. **Structural-vs-behavioural assertions** -- many tests assert function signatures, file presence, or schema-row existence but not the runtime behaviour those declarations imply. Examples: P01-G01 (JWT pin signature exists but never exercised), P09-G05 (HealthSchema widened but never falsified), P09-G06 (Memory* allowed in tree but no static-analysis test that production paths avoid it). Plan §9.x additions should explicitly test the behaviour, not the declaration.
  - **12 Critical gaps gate first test authoring**: P01-G01, P01-G02, P05-G01, P06-G01, P07-G01, P07-G02, P08-G01, P08-G02, P09-G01, P09-G02, P09-G03, P09-G04. These cover key safety/correctness invariants (JWT pinning, capability shape, audit-first delta payload, PDF no-overwrite, ProcessedOp idempotency on resolve, soak report capture, autoload dependency ordering, prisma onClose, refresh-token survival across restart, conflict-resolve tx-API split). They MUST land as `§9.x` additions before the corresponding phase begins test execution.
  - **Pass 1 additions landed (2026-05-13)**: §9.x sections appended to every phase test plan, one subsection per gap, total 112 subsections matching Pass 1 totals exactly (phase-01: 9.1-9.18; phase-02: 9.1-9.12; phase-03: 9.1-9.15; phase-04: 9.1-9.9; phase-05: 9.1-9.11; phase-06: 9.1-9.7; phase-07: 9.1-9.12; phase-08: 9.1-9.14; phase-09: 9.1-9.14). Each subsection encodes the source build-spec section, the target test-plan section the new row(s) merge into during authoring, the gap category and severity, and a copy-paste-ready test scenario.
  - **Pass 2 trigger condition**: re-run gap analysis against every phase test plan now that §9.x additions exist; a pass that finds zero true gaps ends the cycle. Pass 2 should re-compare phase-build-spec §3/§4/§7 against the union of `phase-XX-test.md` §1-§6 AND §9. New gaps would land as §10.x additions on a third pass.
- 2026-05-13: `phase-01-test.md` through `phase-09-test.md` -- the remaining 6 plans (`01, 02, 03, 07, 08, 09`) drafted in one pass after the template stabilized post-phase-06. All 9 phases are now `in_progress`. Test counts remain `0` -- no tests authored, no tooling installed. Open defects: 0.
  - **Phase 01 (Foundation & Sync Plumbing, L)** -- canonical persona **P3 Mariam the Superadmin**. 9 IPCs (`sync::status`, `sync::trigger_push/pull`, `sync::list_conflicts`, `sync::resolve_conflict`, `sync::outbox_count` from §7.4, `device::info`, `config::set/get_sync_server_url` from §7.22). Key invariants tested: audit-first ordering (`with_audit` step sequence pinned in §1.1 + §2.1), outbox `op = 'upsert'` only in v1 per §7.15, `parked` flag stops retry storms per §7.17, delete-vs-edit reconciliation per §7.16 + audit_log carve-out per §7.31, `audit_log` strict-additive (server rejects `deleted_at != null`) per §7.21, `ProcessedOp` byte-identical replay, `/sync/lookup-op` startup reconcile per §7.20, RedactionLayer at the domain layer (NOT just tracing) per §7.14, JWT-key pinning bootstrap stub per §7.10. Tooling bootstrap: cargo-llvm-cov, vitest stack, WebdriverIO + tauri-driver, Ajv stack, wiremock, testcontainers (NEW for ephemeral Postgres). Phase-01 owns the test-infrastructure baseline for every later plan.
  - **Phase 02 (Authentication & Users, L)** -- canonical persona **P3 Mariam the Superadmin**. 19 IPCs (7 auth + 6 users + 3 settings + `users::create_first_admin` from §7.21). Key invariants tested: Argon2id online+offline login round-trip, stronghold creds-cache derived from email only (NOT email:password) per §1.1 helper, refresh-token atomic rotation in one tx, `change_password` revokes ALL refresh tokens for user + invalidates stronghold cache, `users::list` response strips `password_hash` (type-level proof via `UserResponse` per §7.20), `users::create` payload conditionally includes `password_hash` per §7.24 (TypeBox `Type.Never` on update path), required-key delete protection at three layers per §7.2 + §7.33 (UI + IPC + server), settings `manual` conflict policy parks 409, `<RequireRole>` component introduced for use by every later phase per §7.8, first-launch ar-forcing detector per §7.11, `formatIqd` / `formatIQD` helpers from §7.12 + §7.30. Tool additions: `jsonwebtoken` Rust dep.
  - **Phase 03 (Catalog & Reference Data, XL)** -- canonical persona **P3 Mariam the Superadmin**. 27 IPCs across 8 entities (CRUD + `doctor_pricing::upsert/soft_delete` + `operator_specialties::upsert/soft_delete` + `inventory_consumption::upsert/soft_delete` + `doctors::set_active` per §7.23 + `operators::set_active` per §7.24 + `pricing::resolve_effective_price` for phase-05 lock consumption per §7.26). Key invariants tested: `check_types.has_subtypes` XOR enforced at UI + Rust entity + server `acceptPush` per §7.1, paired partial unique indexes for `doctor_check_pricing` + `inventory_consumption_map` to block Postgres NULL-uniqueness gotcha per §7.20 + §7.21 (raw-SQL migrations), FTS5 triggers filter soft-deleted rows per §7.33, `effective_price` resolver pure function never mutates state per §7.26, `catalog:pricing_changed` event emission per §7.27 with §7.35 payload schema, `operator::soft_delete` cascades specialties in one tx per §7.22, `inventory_consumption.on_dye_only` requires parent `dye_supported=1` per §7.34, `/admin/*` wrapped in `<RequireRole superadmin>` per §7.36, `resolveLocaleName` helper per §7.16, search debounce 250ms / min-2-chars per §7.14, `pulledAt` on every server Prisma model per §7.19. No new tooling.
  - **Phase 07 (Accounting & Reports, L)** -- canonical persona **P1 Asma the Accountant**. 11 IPCs (`reports::dashboard_kpis` + `reports::dashboard_tops` from §7.22 + `reports::visits` + `reports::doctor_earnings` + `reports::doctor_drilldown` + `reports::operator_earnings` + `reports::operator_drilldown` + `reports::daily_close` + 3 CSV exports + PDF export). Key invariants tested: 7 group-by modes for visits report per §7.14 with tagged-union response shape per §7.24, CSV UTF-8 BOM + CRLF + RFC 4180 + deterministic sort per §7.7 + §7.25, "House" pseudo-row aggregates `doctor_id IS NULL` visits per §7.4, hours-on-shift joins closed `operator_shifts` per §7.5, Daily Close BLAKE3 `input_hash` deterministic across runs per §7.12 + §7.19 + idempotent re-run, per-doctor + per-operator + per-check-type breakdowns per §7.9 + §7.21, provisional watermark when `outbox > 0` per §7.20, `[Sign and freeze]` hard-gated on `pendingSync === 0` per §7.11, 90-day boundary routes visits to server but clamps doctor/operator/dashboard locally with banner per §7.16, daily-close audit row `action='daily_close_run'` per §7.18 + phase-01 §7.36 enum extension, all aggregates read snapshot columns NOT live joins per §4 + PRD §4.1, `<RequireRole accountant|superadmin>` on `/accounting/*` per §7.28, void button visible only to superadmin per §7.17 (cross-ref phase-05 §7.24). Phase-07 ships the canonical `fixtures/scale/12-months.sql` consumed by P5 + `performance-soak.md`. No new tooling.
  - **Phase 08 (Audit, Conflict Resolver & Polish, M)** -- canonical persona **P3 Mariam the Superadmin** (+ **P4 Two-Device Conflict** as primary reinforcement). 5 NEW IPCs (`audit::query` + `audit::vacuum_now` + updated `sync::list_conflicts` return type per §7.11 + `sync::resolve_conflict` audit-emission behavior + `diagnostics::summary` per §7.17). Key invariants tested: cross-90-day-boundary merge-paginator per §7.4 with `<ServerBackedBadge>` per §7.25, conflict resolver round-trip end-to-end (parked -> resolve -> audit row server-side + outbox unpark + re-push), resolver idempotency on `resolve_op_id` per §7.22 with `409 ALREADY_RESOLVED` for diff resolution, `<SyncPill>` onClick wiring per §7.14, daily audit vacuum + metrics_events vacuum composite per §7.21 with `vacuum_unsynced_safe` predicate type-level proof per §7.1 + missed-run handling per §7.2 + self-audit row with zero-UUID sentinel per §7.3 + NEVER flips dirty=1 per phase-01 §7.31, soak harness 8h offline with quantitative criteria per §7.16 (>=50 ops/sec, <=800 outbox steady-state, p95 lock < 30s, < 50MB memory, vacuum < 10s, zero auto_resolved conflicts), `pnpm lint:i18n` AST walker per §7.9 + `pnpm lint:rtl` icon scanner per §7.18 + `pnpm a11y` full sweep per §7.13, `/metrics` Prometheus gated by `X-Internal-Token` per §7.17, enriched `/healthz`, entity_id_prefix substring filter per §7.24, `<ConflictList>` durable across app restart per §7.11. Tool additions: `@axe-core/cli`, `@babel/parser`+`traverse`+`types` (i18n + rtl AST), `tracing-flame` + `procfs` (soak memory profiling), `prom-client` (server-side). Phase-08 OWNS `i18n-rtl.md` finalization + the lint gates that block every subsequent commit.
  - **Phase 09 (Pre-Ship Hardening, L)** -- canonical persona **P3 Mariam the Superadmin** + ALL personas as regression gates. ZERO new IPCs; this phase MODIFIES behavior. Addresses the 2026-05-12 pre-ship audit: 6 BLOCKERs (Memory→Prisma swap for sync + auth stores; `JWT_SECRET ?? 'dev-only-secret'` removal; healthz hardcoded ok; missing Dockerfile/compose; manual conflict resolve audit row missing) + 5 SHIP-CONCERNs (env template + memory-store raw throws + console.log + MVP defaultValue + setup subtitle i18n) + 3 NITs (unreachable!() in inventory + stale phase-04 comment + eprintln! in lib.rs). Defining E2E: `prisma-persistence-survives-container-restart.e2e.ts` -- the BLOCKER fix verifier. Conflict-resolve audit row written in same `prisma.$transaction` as resolve commit per §3. JWT plugin refuses production boot without `JWT_PUBLIC_KEY`. `init-custom-sql.sql` applies all phase-03/05/06 raw-SQL pieces idempotently after `prisma db push`. `@fastify/env` validates at boot. Tool additions: `docker` + `docker compose` (NEW -- first phase to require Docker; CI runners), `@testcontainers/postgresql`, `@fastify/env`. Perf gate is regression-only (within 20% of in-memory baseline).
  - Template considered stable across all 9 plans -- the post-phase-06 dogfood passes hold. No new template gaps surfaced during plans 01/02/03/07/08/09.
  - Forward-references: `performance-soak.md` aggregates §7 perf SLOs from every plan + ships the 8-hour soak harness owned by phase-08 §5. `security.md` owns refresh-token replay matrix + JWT tampering rows across all phases. `sync-conflicts.md` owns the 3xN matrix; phase-08-test ships the resolver UI that exercises it. `i18n-rtl.md` owns the page-by-page sweep; phase-08-test ships the lint script that gates it.
- 2026-05-13: `phase-06-test.md` drafted (Inventory Operations -- M scope: 5 IPC commands, no new entities, partial-index status filters from §7.10, audit-first ordering per §7.11, pull-time recompute hook per §7.9, server raw-SQL CHECK migration per §7.14, count_correction defence-in-depth at UI + IPC + server layers per §7.6, sanity-cap warning per §7.8). Status flipped to `in_progress`. Test counts remain `0`. Canonical persona: P2 Mehdi the Receptionist (already gates phase-04 and phase-05; phase-06 inventory work lives inside the same day-script). Open defects: 0.
  - Third plan written against the refined template; no template gaps surfaced -- the post-phase-05 refinements (Out of scope / Cross-phase commands / `Default?` column / fixed `AppError` row / canonical persona labelling / RTL `describe.each` invariant) carried over cleanly. Template considered stable for the remaining 6 phases.
  - No new tooling: phase-06 inherits the full toolchain installed during phase-04 / phase-05 execution (cargo-llvm-cov, vitest stack, WebdriverIO + tauri-driver, Ajv stack, msw@2, pdf-extract).
  - Cross-phase notes: phase-06 has `Cross-phase commands: none`. Catalog-side `inventory_catalog_*` and `inventory_consumption_*` (registered in `lib.rs` alongside phase-06 commands) belong to phase-03 and are tested in `phase-03-test.md` once that plan is written. Phase-05's `consume_visit` writer (inside `visits::lock`) is tested in `phase-05-test.md` §2.
  - Forward-references: `performance-soak.md` for the 1k-item / 50k-adjustment scale fixture, `i18n-rtl.md` for the page-by-page `/inventory/*` sweep, `security.md` for JWT-role-tamper replay against `/sync/push` for count_correction, `sync-conflicts.md` for the consume-vs-void cross-product cell, phase-08 test for the conflict-resolver round-trip (N/A for additive-only inventory but receipt only).
- 2026-05-13: testing suite scaffold initialized. `.claude/rules/testing.md`, `_template/phase-XX-test.md`, this `testing-status.md`, `defects.md`, `personas.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`, `security.md`, `fixtures/README.md`, `fixtures/clinical-day.sql` all created. No phase test plans written yet; no tooling installed yet.
- 2026-05-13: `phase-05-test.md` drafted (Reception & Visit Lock -- XL scope: 3 entities, 22 IPC commands including the cross-phase `shifts_lines_run_today`, FTS5, manual conflict policy, receipt generation, name-snapshot invariants, audit-first multi-step lock, telemetry events). Status flipped to `in_progress`. Test counts remain `0`. Canonical persona: P2 Mehdi the Receptionist (already exercises this surface end-to-end). Open defects: 0.
  - First plan written against the refined template -- gaps #1-11 applied to `_template/phase-XX-test.md` and `.claude/rules/testing.md` §3 before this draft.
  - Forward-references: `performance-soak.md` for the 12-month scale fixture, `i18n-rtl.md` for the page-by-page sweep, `security.md` for refresh-token replay, `sync-conflicts.md` for the 3xN matrix beyond the 3 manual/LWW/additive cells exercised here, phase-08 test for the conflict resolver round-trip.
  - Tooling already installed in phase-04-test execution (cargo-llvm-cov, vitest stack, WebdriverIO + tauri-driver, Ajv stack) is sufficient. New for phase-05: `wiremock` (Rust dev-dep, offline sync server scenarios), `msw@2` (TS dev-dep, IPC mock parity for component tests), `pdf-extract` + `pdfium-render` (Rust dev-dep, A5 PDF text-layer + bitmap hash comparison).
- 2026-05-13: `phase-04-test.md` drafted as the template dogfood (smallest surface: 7 owned IPC commands + 1 cross-referenced from phase-05). Status flipped to `in_progress`. Test counts remain `0` -- the plan is written; no tests have been authored, no tooling installed. Persona run record still empty. Open defects: 0.
  - Tooling install deferred to first execution of this plan: `cargo-llvm-cov` (Rust coverage), `vitest` + `@testing-library/react` + `jsdom` + `@vitest/coverage-v8` (frontend unit/component), `webdriverio` + `tauri-driver` (E2E), `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser` (contract).
  - Cross-phase note: `shifts_lines_run_today` IPC (registered in `lib.rs` but conceptually phase-05 per phase-04 §7.7) is tested in `phase-05-test.md`, not here. Phase-04 §2.2 references the cross-ref row.
  - Next: review template refinements surfaced during this draft, apply to `_template/phase-XX-test.md`, then start `phase-05-test.md` (most complex surface).
- WebdriverIO + tauri-driver, Vitest, and cargo-llvm-cov installation is the first concrete step when phase-04 test execution begins.
