# Gap Analysis Pass 5 -- Test Plans vs Build Specs

_Date: 2026-05-13_

**Pass 5 is the closing pass. All 9 phases report zero true gaps.** The cycle is formally complete per `.claude/rules/planning.md` Gap Analysis Methodology -- a pass that finds zero true gaps ends the cycle.

Pass 5 re-compared every `docs/idc-system/phase-XX.md` build spec against the matching `docs/idc-system/testing/phase-XX-test.md` test plan UNION of `§1-§6 + §9 + §10 + §11 + §12`. The union covers every build-spec promise across all 9 phases.

## Pass 5 Totals

| Phase | Total | Critical | High | Medium | Low |
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
| **Total** | **0** | **0** | **0** | **0** | **0** |

## Pass-Counter Trajectory

| Pass | Date | Total Gaps | Critical | High | Medium | Low | Cumulative §x.y Subsections |
|-|-|-|-|-|-|-|-|
| 1 | 2026-05-13 | 112 | 12 | 35 | 44 | 21 | §9 (112) |
| 2 | 2026-05-13 | 132 | 8 | 48 | 57 | 19 | §9 + §10 (244) |
| 3 | 2026-05-13 | 64 | 1 | 19 | 34 | 10 | §9 + §10 + §11 (308) |
| 4 | 2026-05-13 | 26 | 0 | 6 | 13 | 7 | §9 + §10 + §11 + §12 (334) |
| **5** | **2026-05-13** | **0** | **0** | **0** | **0** | **0** | **CYCLE CLOSED** |

**Trajectory: 112 -> 132 -> 64 -> 26 -> 0.** Pass 2 grew because §9 additions opened new exposure; Passes 3 and 4 cut residuals by 51% then 59%; Pass 5 hits the cycle-termination threshold. 334 total gap-derived test rows landed across the 9 phase test plans.

## Convergence Analysis

The cycle converged in five passes from 112 initial gaps to zero. The trajectory exhibits the canonical pattern documented in `.claude/rules/planning.md`:

1. **Pass 1 (initial sweep)**: Surfaces obvious missing coverage and high-leverage invariants. 12 criticals.
2. **Pass 2 (re-comparison)**: Grows the count because §9 additions themselves opened new surface to audit. 8 criticals -- the remaining truly safety-critical items.
3. **Pass 3 (exhaustiveness sweep)**: Halves the count as registry / enumeration blind spots close. 1 critical -- a multi-tenant data-leak invariant.
4. **Pass 4 (server-side parity sweep)**: Cuts another 59%, lands at zero criticals -- a decisive convergence signal.
5. **Pass 5 (closing pass)**: Zero gaps across all 9 phases. Cycle terminates.

The methodology's "0 true gaps" gate is satisfied. Every build-spec promise across §3 (DDD), §4 (Business Logic), §6 (Verification), and §7 (PRD Gap Additions) for all 9 phases has at least one verification scenario in the union of §1-§6 + §9 + §10 + §11 + §12.

## Cumulative Coverage Summary

| Phase | Pass 1 | Pass 2 | Pass 3 | Pass 4 | Pass 5 | Total Subsections | Last Gap ID |
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

## What the Phase Test Plans Now Cover

Each `phase-XX-test.md` has accumulated four generations of gap-derived subsections (§9 / §10 / §11 / §12) on top of the original §1-§6 + §7 + §8 + §10 base plan. The union covers, per phase:

- **Schema invariants** -- every CREATE TABLE / Prisma model field, every CHECK constraint, every partial index DDL with WHERE clause assertion, every FK with ON DELETE policy, every composite PK shape via contract test.
- **State machines** -- every legal transition tested at entity layer + service layer + push acceptance + UI; every illegal transition variant from the documented matrix.
- **Sync semantics** -- every entity's conflict policy (additive-only / LWW / manual), every push / pull symmetry, tenant injection on /sync/push (server-side, not just pull), idempotency replay via ProcessedOp, audit-first ordering on BOTH local Rust and server Prisma sides.
- **Business logic** -- every service method's happy + error paths; every audit row's `delta` payload shape on local AND server; every outbox row's envelope shape; every event emission gated on commit success.
- **Frontend** -- every page / component / hook with both LTR and RTL coverage (`describe.each([['ltr'],['rtl']])`); every cache invalidation path on mutation success; every optimistic update path with rollback; every Zod schema with `min`/`max`/`refine` boundary cases.
- **Edge cases (the 8 mandatory categories)** -- time/timezone (Asia/Baghdad, midnight rollover); i18n & RTL (en/ar swap, Arabic-Indic numerals, mixed-script); offline & network; concurrency & conflicts (multi-device); crash & recovery (SIGKILL during writes, WAL state); scale & performance (10k visits, 1k patients); security & permissions (role bypass, JWT tamper, FTS5 injection, soft-delete bypass); data integrity (migration replay, FK enforcement, sync_version monotonicity).
- **Performance SLOs** -- every default from `.claude/rules/testing.md` §9 plus phase-specific tightenings, including the Criterion bench harness from phase-08 §5 (Lock end-to-end p95 < 30s; Sync replication after reconnect p95 < 5s; Audit query 90-day window p95 < 500ms local).
- **Coverage gates** -- Rust domain >=90%, sync engine >=95%, infrastructure >=75%; frontend domain hooks/services >=90%, presentation >=60%; sync server domain >=90%, routes >=85%; with explicit per-module rows for load-bearing helpers (lwwShouldApply 100% branches, rtl/icons.ts >=95%).
- **Snapshot artifacts** -- A5 PDF receipt, thermal receipt, daily-close PDF, sync push/pull envelopes, error response envelope, i18n key namespaces; canonical hashes plus fixture-input hashes.

## What Remains

Pass 5's zero result formally closes the **planning** cycle. It does NOT mean the tests exist as code -- the testing suite currently has **0 tests authored** and **no tooling installed**. The 334 test-row subsections are copy-paste-ready scenarios pinned to specific build-spec sections; execution begins per phase under the workflow in `.claude/rules/testing.md` §15.

Per `testing-status.md`, all 9 phases are `in_progress` with started-2026-05-13. Each phase advances to `complete` only when its §8 DoD checkbox list is fully checked -- including running every Pass 1-4 row that landed under it.

## Methodology Notes

The five-pass cycle validated three structural patterns documented in the planning rules:

1. **Pass 2 growth is the methodology working.** A Pass 2 that surfaces fewer gaps than Pass 1 is a warning sign -- it means §9 additions weren't audited for the new surface they opened. Our Pass 2 grew from 112 -> 132; eight rows directly attached to Pass 1 §9 rows that themselves carried holes.
2. **Server-side parity of local-side invariants is the recurring blind spot.** Every audit-first ordering test that lived in Rust integration needed a Prisma-side mirror. Closed via §11 (Pass 3) and §12 (Pass 4) systematically.
3. **Closed-set / registry exhaustiveness is the second recurring blind spot.** Enumerating one of N variants (Pass 1) needs an exhaustiveness pair (Pass 2+). The recurring "the one of three / four / seven not tested" pattern surfaced in every phase through Pass 4.

These observations should be folded into a future revision of `.claude/rules/planning.md` Gap Analysis Methodology as Pass 2+ focus-area annotations.

## Next Steps

1. **Update `testing-status.md` Gap Analysis Summary** with the Pass 5 closing row: `2026-05-13 \| 0 \| 0 \| 0 \| 0 \| 0 \| complete (cycle closed)`.
2. **No §13.x additions land.** Pass 5 produced zero gaps; no test-plan edits are needed.
3. **Begin test execution per phase** under `.claude/rules/testing.md` §15. The pre-flight critical from Pass 3 (P03-G32 server-side `entityIdTenant` injection) lands first.
4. **Update `.claude/rules/planning.md` Gap Analysis Methodology** with the documented Pass-2-growth / server-side-parity / closed-set-exhaustiveness patterns observed across this cycle.
