# IDC System v0.1.x Testing Status

_Last updated: 2026-05-13 (scaffold initialized; no phase test plans written yet). Source: [.claude/rules/testing.md](../../../.claude/rules/testing.md)._

This file tracks the testing cycle the same way `status.md` tracked the build cycle. A phase flips to `complete` only when its `phase-XX-test.md` §8 Definition of Done is fully checked. See `.claude/rules/testing.md` §5 + §11.

## Phase Status Table

| # | Phase | Status | Started | Completed | Unit | Integration | Contract | E2E | Manual | Coverage % | Open Defects |
|-|-|-|-|-|-|-|-|-|-|-|-|
| 01 | Foundation & Sync Plumbing | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 02 | Authentication & Users | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 03 | Catalog & Reference Data | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 04 | Operator Shifts | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 05 | Reception & Visit Lock | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 06 | Inventory Operations | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 07 | Accounting & Reports | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 08 | Audit, Conflict Resolver & Polish | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |
| 09 | Pre-Ship Hardening | not_started | -- | -- | 0 | 0 | 0 | 0 | 0 | -- | 0 |

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
| 1 | -- | -- | -- | -- | -- | -- | not_started |
| 2 | -- | -- | -- | -- | -- | -- | not_started |
| 3 | -- | -- | -- | -- | -- | -- | not_started |

Pass 1 will execute after all 9 `phase-XX-test.md` files are drafted. Pass 2 and 3 only run if Pass 1 surfaces gaps. A pass that finds zero true gaps ends the cycle.

### Per-Phase Distribution (Pass 1)

| Phase | Gaps | Critical | High | Medium | Low |
|-|-|-|-|-|-|

(populated by Pass 1)

## Blockers & Notes

- 2026-05-13: testing suite scaffold initialized. `.claude/rules/testing.md`, `_template/phase-XX-test.md`, this `testing-status.md`, `defects.md`, `personas.md`, `sync-conflicts.md`, `i18n-rtl.md`, `performance-soak.md`, `security.md`, `fixtures/README.md`, `fixtures/clinical-day.sql` all created. No phase test plans written yet; no tooling installed yet.
- Next: write `phase-04-test.md` first as a small-surface dogfood of the template (Operator Shifts has 8 IPC commands, 1 table -- the smallest phase). Use lessons from that to refine the template before tackling larger phases.
- WebdriverIO + tauri-driver and Vitest installation is deferred to the first phase that needs them (likely phase-04-test or phase-02-test).
