---
paths:
  - "docs/**/testing/**"
  - "**/testing-status.md"
  - "**/phase-*-test.md"
  - "**/defects.md"
  - "**/personas.md"
---

# Testing Plan & Execution Rules

All test plans follow the **5-Layer Test Pyramid + 8-Category Edge Coverage** pattern. Test plans live in `docs/<plan-name>/testing/` and consist of the file set below. The IDC system is **two surfaces** that must be tested jointly: the Tauri offline-first desktop app (React + Rust) and the Fastify sync/backup server. Every phase test plan declares which surfaces it covers.

This document governs WHAT to test, WHERE the plans live, HOW to structure them, and WHEN a phase is "test-complete." It does not describe specific tests — those live in the phase plans.

## §0 When to Write a Test Plan

| Situation | Artifact |
|-|-|
| New phase shipping (or already shipped) | `phase-XX-test.md` from `_template/` |
| Concern spans 2+ phases (sync, i18n, perf, security, persona day) | Cross-cutting plan under `testing/<concern>.md` |
| Defect found in production or in test | Row in `defects.md` + regression test in the relevant phase plan |
| Anything else | No new file -- existing plans absorb it |

Do NOT create per-feature ad-hoc test docs. Either it belongs to a phase plan, or it is a cross-cutting concern. There is no third bucket.

## §1 Testing Suite Structure

Mandatory files for every plan name under `docs/<plan-name>/testing/`:

| File | Purpose |
|-|-|
| `testing-status.md` | Living tracker -- phase status table, cumulative totals, gap-analysis passes. Mirror of `status.md`. |
| `phase-XX-test.md` (one per phase) | Per-phase test plan. Test-pyramid layout + 8 mandatory edge sections. |
| `_template/phase-XX-test.md` | Copy-paste source for new phase test plans. |
| `personas.md` | Cross-cutting persona day-scripts (named actors, sequenced workflows). |
| `sync-conflicts.md` | Cross-cutting 3xN conflict matrix (policies x entities). |
| `i18n-rtl.md` | Cross-cutting en/ar + RTL + Arabic-Indic numerals coverage. |
| `performance-soak.md` | Cross-cutting perf SLOs, soak procedure, scale drills. |
| `security.md` | Cross-cutting auth bypass, JWT tamper, injection, role matrix. |
| `defects.md` | In-repo defect log, P0-P3 severity, links to repro tests. |
| `fixtures/clinical-day.sql` | The single shared realistic seed for E2E and persona runs. |
| `fixtures/README.md` | Fixture regeneration policy and naming convention. |

Per-phase plans are written one at a time as that phase's testing is tackled. They are NOT all scaffolded upfront. Cross-cutting plans, the template, status, and defects ARE scaffolded upfront -- they are the planning surface.

## §2 The 5-Layer Test Pyramid

Every phase test plan organizes scenarios into these five layers, in this order. Each layer names its tool and what does NOT belong in it.

| Layer | Tool | Scope | Does NOT belong here |
|-|-|-|-|
| **1. Unit** | `cargo test` (Rust), Vitest (TS) | Pure functions, value objects, domain services with no I/O. Fast, isolated. | Anything that hits SQLite, the network, the file system, or React rendering. |
| **2. Integration** | `cargo test` (`src-tauri/tests/*.rs`), `node:test` (sync-server), Vitest + RTL (frontend) | Repos with real SQLite, sync engine with real outbox, Tauri IPC handlers, Fastify routes with Prisma against a test DB, React Query hooks with mocked IPC. | Cross-process E2E, headless browsers, real network. |
| **3. Contract** | Custom harness (Ajv + Swagger schema, type-shape diff) | Swagger response validation, IPC TS types vs Rust serde shapes, sync envelope versioning, conflict-resolution policy declarations. | Behavioural tests -- only structural agreement between sides. |
| **4. E2E** | WebdriverIO + `tauri-driver` | The built Tauri binary driven end-to-end. SQLite is real. Sync server runs in Docker. UI is real. Multiple instances for multi-device flows. | Mocked IPC, mocked sync, anything that does not hit the real binary. |
| **5. Manual** | Persona scripts under `personas.md` | Visual/print review (A5 PDF, thermal output), RTL layout sanity, real-world journey replay. | Anything that can be automated. Manual is the last resort, not the default. |

The pyramid is wide at the base. A phase with 50 unit tests and 0 E2E is suspicious; a phase with 5 unit tests and 20 E2E is also suspicious. Aim for the classic shape: many unit, fewer integration, fewer contract, few E2E, fewer manual.

## §3 Phase Test Plan Template

Every `phase-XX-test.md` MUST have these sections in this exact order:

### Header
```
# Phase N: <Name> -- Test Plan

**Proves:** <One sentence: what user-facing capability this plan verifies>

**Surfaces under test:** Frontend | Tauri/Rust | Sync Server | All
**Dependencies (other test plans):** Phase X test, Phase Y test (or "None")
**Test Data:** <which factories, which fixtures>
**Tool prerequisites:** <list every tool that this plan's execution will install or rely on -- see §13 -- e.g. `cargo-llvm-cov`, `webdriverio` + `tauri-driver`, `vitest` + `@testing-library/react`, `ajv@8`, or "none new -- inherits from phase-XX-test">
**Out of scope (cross-cutting tests):** <list anything this phase touches but tests in a cross-cutting plan (`security.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`) or another phase plan. The §6 categories MAY mark these `N/A -- owned by <plan>`. Use `none` when everything is in-scope.>
**Cross-phase commands:** <list IPC commands or routes registered in this phase's modules but conceptually owned by another phase, with a pointer to the test plan that covers them. Example: `shifts_lines_run_today` -- registered in phase-04 `lib.rs`, tested in `phase-05-test.md` §2.2. Use `none` when no such commands exist.>
```

### Section 1: Unit Tests (Pyramid Layer 1)
- §1.1 Rust domain services -- list module paths and what each test asserts.
- §1.2 TS pure functions / value objects.
- §1.3 Coverage targets -- table of `| Path glob | Threshold | Tool invocation |`. Drop rows that don't apply, but never silently lower a threshold; thresholds come from §8 and a documented override requires §8 sign-off.

### Section 2: Integration Tests (Pyramid Layer 2)
- §2.1 Rust integration tests -- `src-tauri/tests/<entity>_phaseXX.rs`. Continue the existing convention (see `sync_phase01.rs`, `shifts_phase04.rs`).
- §2.2 Tauri IPC handler tests -- one test per command in this phase; assert happy path + at least one error path. For any **cross-phase command** declared in the header, add a `(cross-ref)` row pointing at the owning phase plan instead of writing tests here.
- §2.3 Sync server route handlers -- with a real Prisma test DB; tear down per-test.
- §2.4 React Query mutation/query flows -- with mocked IPC; assert cache invalidation and optimistic update behaviour. Every component / hook test that renders DOM MUST run in both `dir=ltr` AND `dir=rtl` (`describe.each([['ltr'],['rtl']])`). Asserting only LTR is incomplete (see §14 anti-pattern "RTL never tested").

### Section 3: Contract Tests (Pyramid Layer 3)
- §3.1 Swagger contract -- for every server route in this phase, validate the actual response against the declared TypeBox schema using Ajv.
- §3.2 IPC shape contract -- diff the Rust `serde` JSON shape against the TS `Zod`/`Type` declaration; fail if they drift. The §3.2 table MUST include a FIXED final row for the shared `AppError` envelope (`AppErrorSchema = z.object({ kind: z.enum([...]), message: z.string() })`); every command's error path references it.
- §3.3 Sync envelope contract -- assert push/pull payloads conform to the versioned envelope schema; conflict-resolution policy is declared and matches the entity's expected policy.

### Section 4: E2E Tests (Pyramid Layer 4)
- §4.1 Happy-path flows -- WebdriverIO specs driving the real binary.
- §4.2 Failure-path flows -- offline, mid-sync token expiry, conflict.
- §4.3 Multi-device flows -- two binaries, shared sync server, conflict scenarios. Use `MULTI_DEVICE=true` env to spin a second app instance.

### Section 5: Manual / Persona Scripts (Pyramid Layer 5)
- §5.1 Scripts owned by this phase -- new manual steps that the persona scripts in `personas.md` reference.
- §5.2 Cross-references to `personas.md` -- which persona scripts exercise this phase end-to-end.

### Section 6: Edge Case Coverage (8 mandatory categories)

Every phase plan MUST include all eight subsections. Acceptable forms per subsection:
- A concrete test or scenario (preferred).
- `N/A -- <one-line reason>` when the phase genuinely has no surface in that category.
- `N/A -- owned by <cross-cutting plan or other phase test>` when the surface exists but is tested elsewhere (`security.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`, or another phase plan). The pointer MUST match the value listed in the header's `Out of scope` line.

"Not applicable" with no reason is forbidden. Empty is forbidden.

- §6.1 Time / Timezone -- Iraq TZ (Asia/Baghdad), DST, midnight rollover, clock skew vs server, day-boundary edges.
- §6.2 i18n & RTL -- en/ar swap, Arabic-Indic numerals toggle, RTL layout, mixed-direction text in inputs/tables.
- §6.3 Offline & Network -- full offline, intermittent connection, token expiry mid-sync, server unreachable, partial-batch push.
- §6.4 Concurrency & Conflicts -- 2-device same row, 3-device chain, conflict policy invocation, resolver round-trip.
- §6.5 Crash & Recovery -- SIGKILL during write, SQLite WAL after crash, disk full, atomicity invariants of multi-step transactions.
- §6.6 Scale & Performance -- 10k visits, 1k patients, FTS5 at scale, large outbox drain, report query on 90-day window.
- §6.7 Security & Permissions -- role bypass attempt, JWT tamper, FTS5 query injection, soft-delete bypass, refresh-token replay.
- §6.8 Data Integrity -- migration replay (forward + on existing data), FK enforcement, soft-delete cascade rules, sync_version monotonicity.

### Section 7: Performance SLOs (this phase's surfaces)

Table:

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|

Concrete numbers, not "fast." Hard pass/fail gates in CI. Default SLOs from §9 apply unless this phase overrides. The `Default?` column declares `yes` when the threshold matches §9 and `no` when the phase overrides it; an override row MUST carry a rationale in the last column. A silent override is forbidden.

### Section 8: Definition of Done

Checkbox checklist mirroring §11. The phase is `complete` in `testing-status.md` only when every box is checked. Two specific items the template enforces and the rule reiterates:

- **Snapshot listing.** The "Snapshot files committed" checkbox MUST list the concrete paths this phase owns (or state `none -- phase adds no snapshot artifacts`). A vague "snapshots committed" without paths is not acceptable.
- **Canonical persona row.** The §8 "Persona run record" table MUST have a first row explicitly labelled the **canonical persona** -- the single `personas.md` script that gates `complete` per §11. Additional persona rows are reinforcement and optional; the canonical row is not.

## §4 Cross-Cutting Plans

A concern earns a cross-cutting plan when it spans two or more phases AND has its own cross-cutting test logic that does not belong inside any single phase plan.

The four mandatory cross-cutting plans for IDC:

| Plan | Owns |
|-|-|
| `personas.md` | Named-actor day-scripts that walk through 3+ phases end-to-end. Reception day, accounting day, multi-device conflict day, year-end audit day. |
| `sync-conflicts.md` | The 3xN matrix: each of the 3 conflict policies (`additive-only`, `last-write-wins`, `manual`) across every entity that declares it. One scenario per cell. |
| `i18n-rtl.md` | en/ar swap on every route, RTL layout invariants, Arabic-Indic numerals toggle behaviour, mixed-script input. Page-by-page checklist. |
| `performance-soak.md` | The 8-hour soak procedure (declared in phase 8), large-data drills, cold-start budgets, outbox drain throughput. Aggregates the §7 SLOs from all phase plans. |
| `security.md` | Auth bypass attempts, JWT tampering, FTS5 query injection, role-route access matrix, refresh-token replay, secret-storage invariants. |

Cross-cutting plans use a layout appropriate to their concern (matrix, checklist, runbook). They are not required to follow the §3 phase template -- only phase plans do.

## §5 testing-status.md Schema

Mirror of `docs/<plan-name>/status.md`. Four sections in this exact order:

1. **Phase Status Table** -- Columns: `#`, `Phase`, `Status`, `Started`, `Completed`, `Unit`, `Integration`, `Contract`, `E2E`, `Manual`, `Coverage %`, `Open Defects`. Status values: `not_started`, `in_progress`, `complete`. Counts are absolute (number of tests/scenarios), not boolean.
2. **Cumulative Totals** -- Columns: `Metric`, `Before`, `Current`, `Target`. Track total tests by layer, total snapshot files, coverage % by surface, total open defects by severity.
3. **Gap Analysis Summary** -- Per-pass tables of test-coverage gaps (a "gap" here is a missing scenario, not a missing build artifact). Same shape as `status.md`'s pass tables: `Pass`, `Date`, `Gaps Found`, `Critical`, `High`, `Medium`, `Low`, `Status`.
4. **Blockers & Notes** -- Prose tail. What is in flight, what is blocked, which defects are escalated.

Status flips to `complete` only when the phase's `phase-XX-test.md` §8 DoD checklist is fully checked. Partial work stays `in_progress`.

## §6 Edge Case Categories (Mandatory 8)

Already enumerated in §3.6. Reiterated here as the canonical list because phase plans reference this section.

| # | Category | Default Test Vehicle |
|-|-|-|
| 6.1 | Time / Timezone | Rust integration with mocked clock; E2E with fake-time hook |
| 6.2 | i18n & RTL | Vitest+RTL with locale param; manual visual review |
| 6.3 | Offline & Network | E2E with `--offline` flag on tauri-driver; integration with stubbed HTTP |
| 6.4 | Concurrency & Conflicts | Multi-device E2E (`MULTI_DEVICE=true`); Rust integration with two SQLite handles |
| 6.5 | Crash & Recovery | Rust integration with `kill -9` between txns; SQLite WAL inspection |
| 6.6 | Scale & Performance | Synthetic fixture (10k rows) loaded via factories; perf assertions |
| 6.7 | Security & Permissions | Contract tests on role-route matrix; E2E with tampered JWT |
| 6.8 | Data Integrity | Migration replay against `clinical-day.sql`; FK violation drills |

Every phase MUST address all eight. `N/A -- <reason>` is acceptable when a phase truly has no surface in that category (rare). Empty section is forbidden.

## §7 Test Data

| Source | When to use | Lives in |
|-|-|-|
| **Factories** | Unit, Integration, most E2E. Build exactly what the test needs. | `src-tauri/tests/support/factories.rs` (Rust), `src/test-utils/factories.ts` (TS), `sync-server/test/support/factories.ts` (server) |
| **The clinical-day fixture** | E2E persona scripts, scale drills, manual review. ONE realistic shared snapshot. | `docs/idc-system/testing/fixtures/clinical-day.sql` |
| **Inline SQL** | Forbidden except in one-off migration tests. | n/a |

Factory rules:
- Composable: `makeVisit({ status: 'locked', patient: makePatient() })`.
- Deterministic by default: same args produce same output (use a seeded RNG for IDs/timestamps unless the test explicitly wants randomness).
- One factory per entity. No "make a doctor with pricing AND a visit" mega-factory -- compose them.

Fixture rules:
- ONE canonical seed: a realistic Tuesday at IDC. 8 doctors with pricing, 200 patients (FTS populated), 30 visits in mixed states (draft/locked/voided), full inventory items + 30 days of adjustments, 2 operator_shifts, 5 days of audit_log.
- Regenerated only when the schema changes. Regeneration policy is in `fixtures/README.md`.
- Never edited by hand to "fix a test" -- regenerate from a script.
- The same fixture is loaded into Postgres (via Prisma) for server-side E2E.

## §8 Coverage Gates

Hard CI gates by layer. Block the merge if any threshold is missed.

| Layer | Threshold | Tool |
|-|-|-|
| Rust domain (`src-tauri/src/domains/*/domain/`) | >= 90% lines | `cargo-llvm-cov` |
| Rust sync engine (`src-tauri/src/sync/`) | >= 95% lines | `cargo-llvm-cov` |
| Rust infrastructure (`src-tauri/src/domains/*/infrastructure/`) | >= 75% lines | `cargo-llvm-cov` |
| Frontend domain hooks/services (`src/lib/`, `src/domains/*/services/`) | >= 90% lines | `vitest --coverage` |
| Frontend presentation (`src/pages/`, `src/components/`) | >= 60% lines | `vitest --coverage` |
| Sync server domain | >= 90% lines | `c8` |
| Sync server routes | >= 85% lines | `c8` |

Aggregate report posts to the PR. A regression below the threshold blocks merge; an explicit phase-test-plan §8 sign-off is required to relax it.

## §9 Performance SLOs

Default SLOs. Phase plans MAY override with a documented reason; they MAY NOT relax silently.

| Surface | Operation | Threshold |
|-|-|-|
| Tauri (SQLite) | Single-record read by PK | < 5 ms p99 |
| Tauri (SQLite) | List query (typical filtered, 50 rows) | < 30 ms p99 |
| Tauri (SQLite) | Visit lock transaction (full) | < 200 ms p99 |
| Tauri (SQLite) | FTS5 patient search (200 chars/word) | < 50 ms p99 |
| Tauri (cold start) | First paint after launch | < 3 s p99 |
| Sync engine | Outbox drain throughput | >= 50 ops/sec |
| Sync engine | Push round-trip (single op) | < 1 s p95 |
| Sync engine | Pull (typical batch of 100 ops) | < 2 s p95 |
| Sync engine | 8-hour soak steady-state outbox depth | <= 800 rows |
| Sync server | Single-route handler latency | < 200 ms p95 |
| Reports | 90-day visits report | < 1 s p95 |
| Reports | Daily-close PDF generation | < 3 s p95 |

Performance tests fail loudly. They are NOT advisory. A flaky perf test is a real bug -- fix the variance, do not raise the threshold.

## §10 Snapshot / Golden Files

The following outputs MUST be locked by hash-based snapshot:

| Artifact | Format | Comparison method |
|-|-|-|
| A5 visit receipt | PDF | Hash of extracted text layer + hash of rendered page-1 bitmap at 150dpi |
| Thermal receipt | UTF-8 + ESC/POS | Byte-exact hash |
| Daily-close PDF | PDF | Hash of extracted text layer + per-section structural hash |
| Sync envelope sample (push & pull) | JSON | Hash of canonicalized JSON |

`expected/` files live next to the test and are committed. Regeneration is explicit (a `--update-snapshots` flag) and requires visual review documented in the PR. Auto-accepting a snapshot change is forbidden.

## §11 Definition of Done (per phase)

A phase test plan is `complete` only when ALL of the following are true:

- [ ] All 5 pyramid layers green in CI.
- [ ] All 8 edge categories addressed in §6 (no empty subsections; `N/A -- reason` is allowed).
- [ ] §7 Performance SLOs met for every row.
- [ ] At least one persona script in `personas.md` exercises this phase's surfaces end-to-end and passes.
- [ ] Coverage gates per §8 met for every code path this phase added.
- [ ] No open defects with severity P0 or P1 against this phase.
- [ ] `phase-XX-test.md` §8 checklist is fully checked.
- [ ] `testing-status.md` row updated with started/completed dates, test counts per layer, coverage %.
- [ ] Snapshot files committed (where §10 applies).
- [ ] Lint, typecheck, build pass.

Missing any box = `in_progress`. There is no partial complete.

## §12 Defect Log

`defects.md` is an in-repo bug log. Table columns:

| Column | Notes |
|-|-|
| `ID` | `DEF-NNN`, monotonic, never reused |
| `Phase` | Owning phase (the one whose surface broke) |
| `Severity` | P0 (data loss / crash / corruption), P1 (workflow blocker), P2 (degraded UX), P3 (cosmetic) |
| `Surface` | Frontend / Tauri / Sync Server / Cross-surface |
| `Found by` | Test name OR persona script OR human inspection |
| `Repro test` | Path + test name of the deterministic repro |
| `Status` | `open`, `fix_in_progress`, `fixed_verified`, `wontfix` |
| `Fix commit` | Short SHA, once landed |
| `Date` | Logged date in ISO format |

Rules:
- Every defect that survives review MUST have a deterministic repro test before being `fixed_verified`.
- P0 and P1 block the next phase's `testing-status.md` flip to `complete`.
- `wontfix` requires an explanation in the row (link to an inline note).

## §13 Tooling Stack

| Layer | Tool | Status | Action on first use |
|-|-|-|-|
| Rust unit + integration | `cargo test` | Already in use (74 tests) | None |
| Rust coverage | `cargo-llvm-cov` | Not yet | `cargo install cargo-llvm-cov` |
| Tauri true E2E | `WebdriverIO` + `tauri-driver` | Not yet | `pnpm add -D webdriverio @wdio/cli @wdio/local-runner @wdio/spec-reporter`; install `tauri-driver` binary per OS; `pnpm test:e2e` script |
| Frontend component | `Vitest` + `@testing-library/react` + `jsdom` | Not yet | `pnpm add -D vitest @testing-library/react @testing-library/jest-dom jsdom @vitest/coverage-v8`; `vitest.config.ts`; `pnpm test` script |
| Sync server | `node --test` + `c8` + `ts-node` | Already in `sync-server/package.json` | Continue. Populate `test/plugins/` and `test/routes/`. |
| Swagger contract | Ajv + ref-parser | Not yet | `sync-server/test/contract/` harness reading `/documentation/json` |
| Snapshot / golden | Hash-based byte comparison | Not yet | Lightweight helpers in `src-tauri/tests/support/snapshot.rs` and `src/test-utils/snapshot.ts` |
| CI orchestration | GitHub Actions matrix (linux/macos/windows for E2E) | Not yet | Workflow defined as part of the E2E phase plan |

Tool installation happens when the phase that first needs it is tackled, not all at once. Each phase test plan declares its tool prerequisites in the header.

## §14 Anti-Patterns

Reject these in review. Each is a smell with a known fix.

| Smell | Why it is wrong | Fix |
|-|-|-|
| Mocking the database | Tests pass while migrations break in prod. We lived this. | Use a real SQLite in-memory or temp file; use Prisma test DB for the server. |
| Shared mutable test state | Order-dependent tests; impossible to parallelize. | Each test sets up its own data via factories; tear down after. |
| Brittle CSS-selector E2E | Snowflakes on every UI tweak. | `data-testid` attributes; never select by class or DOM position. |
| Ad-hoc `sleep(500)` waits | Slow + flaky. | Wait on explicit conditions (element visible, IPC promise resolved). |
| Snapshot accepted without review | Locks in bugs. | Snapshot updates require a human in the PR; CI never auto-accepts. |
| Testing implementation, not behaviour | Refactor breaks tests; bug ships. | Assert on observable outcomes (DOM, DB row, IPC return), not internal calls. |
| RTL never tested | Ships broken Arabic layouts. | Every component test runs in both `dir=ltr` and `dir=rtl`. |
| Numeric columns without `tnum` assertion | Receipts and tables shift on render. | Component tests assert `font-feature-settings` on numeric cells. |
| One mega-test "covers everything" | Hard to diagnose, slow to run. | One scenario per test; share setup via factories, not by stuffing more into one body. |
| Skipping the persona script | E2E green, real-world broken. | At least one persona script must exercise every phase's surfaces. |
| Tests in the same commit as the code they verify, never run together | False confidence. | CI runs the full suite on every PR; pre-push hook runs the affected slice. |
| "I will add tests later" | No you will not. | Tests land in the same PR as the feature or fix. No exceptions. |

## §15 Workflow

When tackling a phase's tests (mirror of the build dev-workflow):

1. **Open the phase build spec** (`docs/idc-system/phase-XX.md`) and read §3 + §6.
2. **Copy `_template/phase-XX-test.md`** to `phase-XX-test.md`.
3. **Research tools (MANDATORY).** Query Context7 for every test library you will touch (WebdriverIO, Vitest, c8, Ajv, etc.). Do NOT write test code from memory.
4. **Fill §1-§4** of the phase plan first (the automated layers). Get them green.
5. **Fill §6 edge categories** -- every one of the 8, no skipping without a written reason.
6. **Fill §5 manual scripts and §7 SLOs.**
7. **Wire CI gates** (coverage threshold, perf threshold). Verify the gate triggers when violated.
8. **Run the persona script(s)** in `personas.md` that touch this phase. They MUST pass.
9. **Update `testing-status.md`** with counts, dates, coverage %, open defects.
10. **Check the §8 DoD checklist.** If all green, flip the status row to `complete`.
11. **NEVER push without the local equivalent of CI passing.** Mirrors the build pre-push rule.

## §16 Git Hygiene (Testing-Specific)

- **NEVER commit with Claude authorship or co-authorship.** Same rule as the build cycle.
- **Tests land with the feature in the same PR.** Never a separate "added tests for X" PR weeks later.
- **Snapshot updates require a written justification** in the PR body. "Renderer changed because <reason>".
- **Coverage drops require explicit acknowledgement** in the PR body. "Coverage dropped 2% because <reason>; phase plan §8 sign-off attached."
- **A red CI is the merge gate.** No `--skip-ci`, no `[skip ci]`, no green-screen-from-rerunning-until-it-passes.

## §17 Subagent Rules

When launching subagents (Agent tool) for test work, include relevant rule content directly in the agent prompt -- subagents do NOT auto-load `.claude/rules/`. For Rust integration test work include `tauri.md`, `rust.md`, `offline-first.md`, plus this file's §2, §6, §8. For server work include `sync-server.md`, `auth.md`, `ddd.md`, plus §3, §6. For frontend test work include `frontend.md`, `design-system.md`, plus §2, §6. Always include "no Claude authorship", "Context7 first", and "no emojis in code or comments".
