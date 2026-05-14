# IDC System Defect Log

In-repo defect log. Every bug found by automated tests, persona scripts, or manual inspection is recorded here. See `.claude/rules/testing.md` §12 for the schema and severity definitions.

## Severity Legend

| Code | Meaning | Examples |
|-|-|-|
| P0 | Data loss, crash, corruption, security breach | SQLite WAL corruption; visit lock loses inventory deduction; auth bypass; ConflictParked row vanishes |
| P1 | Workflow blocker -- a primary user task cannot complete | Cannot lock a visit; sync push silently drops ops; Daily Close PDF fails to render |
| P2 | Degraded UX -- task completes but is impaired | Wrong number formatting; RTL layout breaks; receipt missing a non-critical field; perf 2x over SLO |
| P3 | Cosmetic -- visual or copy issue with no functional impact | Off-by-1-pixel border; misspelled label; outdated tooltip |

P0 and P1 block the owning phase's `testing-status.md` row from flipping to `complete`. P2 and P3 do not block but must be tracked and triaged.

## Status Values

- `open` -- newly logged, no fix in progress.
- `fix_in_progress` -- a developer has picked it up.
- `fixed_verified` -- fix committed AND a deterministic repro test exists AND that test now passes.
- `wontfix` -- closed without a fix; row MUST include an inline note explaining why.

## Rules

1. Every defect that survives initial review MUST have a deterministic repro test before being marked `fixed_verified`. Manual repro alone is not enough.
2. `ID` is monotonic and never reused. Use `DEF-001`, `DEF-002`, ...
3. The `Repro test` column points to a real file path + test name (e.g. `src-tauri/tests/visits_phase05.rs::lock_atomic_under_kill`).
4. The `Fix commit` column is filled when the fix lands. Short SHA only.
5. When a defect is escalated (severity changes), append a row to the History section below rather than editing the original.

## Defect Table

| ID | Phase | Severity | Surface | Found by | Repro test | Status | Fix commit | Date | Notes |
|-|-|-|-|-|-|-|-|-|-|
| DEF-002 | 01 | P1 | Tauri/Rust | `src-tauri/tests/sync_loop_phase01.rs::puller_persists_pulled_audit_log_row_into_local_table_under_shared_cache` | same | fixed_verified | uncommitted | 2026-05-14 | `src-tauri/src/sync/puller.rs::run_step` opened a tx via `pool.begin()`, called `apply_changes(&mut tx, ...)`, then called `state_repo.put_pull_cursor(...)` BEFORE `tx.commit()`. The state_repo write opened a SECOND connection from the pool, which on shared SQLite (production: single-file DB; test: `?cache=shared`) deadlocked on the WAL writer lock — tx held writer 1, put_pull_cursor blocked waiting for writer, tx never committed. **Fix landed 2026-05-14**: added `SyncStateRepo::put_pull_cursor_in_tx(&mut Tx, cursor)` (trait + sqlx impl in `domains/sync/{domain,infrastructure}/...sync_state_repo.rs`); refactored `puller::run_step` to call the in-tx variant before `tx.commit()`. Result: cursor write runs on the SAME connection as the apply tx (phase-01 §4 pull-step 3 atomicity invariant restored). Regression test `puller_persists_pulled_audit_log_row_into_local_table_under_shared_cache` uses `file:def002-<uuid>?mode=memory&cache=shared` (real shared-cache scenario) and verifies (a) no deadlock, (b) audit row materialised, (c) cursor advanced. Pre-fix this test hung indefinitely (timeout-only); post-fix it completes in 0.04 s. |
| DEF-001 | 01 | P2 | Frontend | `src/lib/schemas/sync.test.ts::ConflictSchema rejects missing localPayload with the correct path` (+ sibling `rejects missing serverPayload`, `rejects when both payload keys are missing`) | same | fixed_verified | uncommitted | 2026-05-13 | `ConflictSchema` used `z.unknown()` for `serverPayload` and `localPayload`, which treats missing-as-undefined and silently accepted malformed envelopes. Fix landed 2026-05-14: introduced `requiredUnknown = z.custom<unknown>((v) => v !== undefined, { message: "payload is required" })` and applied it to both fields in `src/lib/schemas/sync.ts`. The original repro test that pinned the broken behaviour was replaced with three positive-rejection tests verifying `ZodError.path` contains `'localPayload'` / `'serverPayload'` per plan §1.2 invariant. The arbitrary-shape test (`serverPayload: null, localPayload: 42`) still passes because `null` and `42` are valid `unknown` values; only `undefined` (missing key) is rejected. Vitest 32/32 green. |

## History (severity escalations, status changes, wontfix notes)

(empty)
