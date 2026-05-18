# Test Fixtures

The fixtures in this directory are the shared, realistic seed data used by E2E tests and persona scripts. See `.claude/rules/testing.md` §7 for rules.

## Files

| File | Purpose |
|-|-|
| `clinical-day.sql` | The canonical "Tuesday at IDC" seed. ONE realistic snapshot used by every E2E persona unless explicitly overridden. |
| `scale-12-months.sql` | 12-month `audit_log` volume fixture for the perf drill (phase-09 §10). Generated from `scripts/regen-scale-12-months.ts`; committed at smoke size (30 days x 10 rows/day = 300 rows), full scale (365 days x 50 rows/day = 18,250 rows) runs locally via `SCALE_DAYS=365 SCALE_ROWS_PER_DAY=50 SCALE_OUT=/tmp/scale-full.sql npx tsx scripts/regen-scale-12-months.ts`. Audit-only -- the other scale targets (1k patients with FTS, 10k visits, 365 operator_shifts, catalog rows) need a full factory chain across FK boundaries; see the "Scaling beyond audit_log" section below. |
| `scale-10k.sql` | (Future) 10k-visit scaled fixture for `/accounting/visits` 90-day report perf drill. |
| `scale-fts.sql` | (Future) 1k-patient scaled fixture for `patients_fts` perf drill. |

Only `clinical-day.sql` is mandatory and shipped from day one. Scale fixtures are added on demand when their owning perf drills are authored.

## Scaling beyond audit_log

`scale-12-months.sql` covers the audit volume half of the phase-09 §10 brief. To finish the other scale targets, future sessions need a TypeScript script that:

1. Boots a clean migrated SQLite + Postgres pair (the existing `migrations::run` + `prisma db push` paths).
2. Uses the factory functions from `src-tauri/tests/support/factories.rs` and `sync-server/test/support/factories.ts` to build the catalog chain (users -> doctors -> check_types -> inventory_items -> consumption maps -> operators).
3. Walks 365 days of clock-in/clock-out for 4 operators (~1460 shift rows).
4. Generates ~10k visits distributed across the year against the catalog rows, with realistic inventory consumption FK-linked to the consumption maps from step 2.
5. Generates ~1k patients with Arabic + Latin names + FTS5 inserts.

The FK coupling is the hard part -- the audit_log generator sidesteps it by emitting only audit rows (which have no FK constraints visible in the schema). A real visit/patient fixture has to honor every CHECK constraint and every FK declared in `migrations/005_patients_visits_adjustments.sql`.

## Rules

1. **Factories first.** Unit and integration tests use factories (`src-tauri/tests/support/factories.rs`, `src/test-utils/factories.ts`, `sync-server/test/support/factories.ts`). Fixtures are for E2E and persona scripts only.
2. **One canonical fixture.** `clinical-day.sql` is the shared world. Do not fork it. If a test needs a slight variation, compose factories on top of the loaded fixture rather than creating a new SQL dump.
3. **Never hand-edit.** Fixtures are regenerated from a deterministic script. Hand-edits drift; the schema changes; the fixture stops loading; a human "fixes" it by hand; the divergence accelerates.
4. **Regenerate on schema change.** Any change to a `migrations/*.sql` file or Prisma schema MUST be followed by a fixture regeneration in the same PR.
5. **Idempotent loading.** Loading a fixture into a clean DB must produce the same state every time. The SQL is forward-only (no `IF NOT EXISTS` defensive cruft).
6. **Loadable on both sides.** The same fixture loads into SQLite (Tauri local) and Postgres (sync server via Prisma). Cross-database differences are handled by the regeneration script, not by separate files.

## Regeneration Procedure

(To be authored as part of the first phase that needs the fixture, likely phase-05-test or phase-07-test.)

Sketch:

1. `scripts/regen-clinical-day.ts` boots a clean SQLite DB and a clean Postgres test DB via the project's migration tooling.
2. The script uses the factory functions to build:
   - 8 doctors with full pricing across all check types.
   - 200 patients with FTS5 populated.
   - 30 visits in mixed states (draft / locked / voided), distributed across the day.
   - Full inventory items + 30 days of receive/writeoff/count-correction adjustments.
   - 2 operator_shifts (1 open, 1 closed yesterday).
   - 5 days of audit_log entries.
3. The script `pg_dump --data-only --inserts` the Postgres state and `sqlite3 .dump` the SQLite state.
4. The two dumps are merged into a single `clinical-day.sql` with conditional sections (Postgres `\if` blocks or duplicated tables -- decided when the script is written).
5. The script commits the regenerated file with a deterministic header (build version, schema version, regeneration date).

## Naming Convention

- `clinical-day.sql` -- the canonical seed.
- `scale-<dimension>.sql` -- scale fixtures.
- `persona-<name>-override.sql` -- overrides loaded ON TOP of `clinical-day.sql` for specific persona scripts.
- `crash-recovery-<scenario>.sql` -- minimal fixtures for crash drills (phase plans §6.5).

Do not create files for one-off needs. If you find yourself wanting a new fixture file, ask first: can a factory call inside the test reproduce this?

## Header Contract

Every fixture SQL file MUST begin with:

```sql
-- File: <name>.sql
-- Purpose: <one line>
-- Schema version: <local-migration-NNN + server-prisma-migration>
-- Regenerated: <ISO date>
-- Regen script: scripts/regen-<name>.ts
-- Loadable into: SQLite | Postgres | Both
-- DO NOT EDIT BY HAND. Regenerate via the script above.
```

A fixture without this header is rejected in review.
