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

(empty -- to be populated as defects surface)

## History (severity escalations, status changes, wontfix notes)

(empty)
