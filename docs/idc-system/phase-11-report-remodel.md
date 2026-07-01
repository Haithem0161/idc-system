# Phase 11 — Report Re-Model (percentage carve-out to an internal reporting doctor)

**Status:** complete (started + completed 2026-06-29)
**Surfaces:** SQLite + Tauri/Rust, Sync Server (Prisma/Fastify), Frontend (React)

## 1. Problem

"Reports" were modeled wrong. The original implementation treated `report` as a
FLAT global IQD surcharge ADDED to the patient's bill:

```
total = price + dye + report_cost          # report inflated the patient total
net   = collected - doctor_cut - operator_cut   # report money silently kept as clinic net
```

The real-world rule is different: a report is a PERCENTAGE of the price,
deducted from the clinic's NET gain (not profit), paid to an internal
"reporting doctor", and computed AFTER the doctor's cut is removed.

## 2. Target Model

```
patient_total = price + dye                          # report NOT in the patient bill
report_amount = report_pct * (price - doctor_cut) / 100   # base excludes dye + operator cut
clinic_net    = collected - doctor_cut - operator_cut - report_amount
report_amount -> owed to the single internal reporting doctor (configured in settings)
```

- **`report_pct`**: one global integer setting (0..=100), replaces the flat
  `report_cost_iqd`. Seeded at 20.
- **`reporting_doctor_name`**: one global text setting naming the internal
  reporting doctor who receives every report amount. Captured into the visit
  snapshot at lock time when `report` is on.
- **دلال (dalal)**: a new built-in doctor-substitute money mode. A `dalal`
  boolean on the visit (parallel to `dye`/`report`) applies a FLAT 10 IQD doctor
  cut with no referring-doctor row. Mutually exclusive with a referring doctor.
  Three money modes now exist:
  | mode | doctor_id | dalal | internal_pct | doctor_cut |
  |-|-|-|-|-|
  | house | NULL | 0 | set | price * pct / 100 |
  | doctor | set | 0 | NULL | per-check -> doctor default -> 0 |
  | dalal | NULL | 1 | NULL | 10 (flat) |
- **`report_supported`** per-check flag is REMOVED — every check can carry a
  report.
- **No history**: pre-launch, so the migration does not backfill or recompute
  any locked visits or signed daily closes.

## 3. Schema (migration `018_report_percentage.sql`)

`visits` table is rebuilt (SQLite cannot alter a table CHECK in place):
- Rename `report_cost_snapshot_iqd` -> `report_amount_snapshot_iqd`.
- Add `report_pct_snapshot INTEGER NULL`, `reporting_doctor_name_snapshot TEXT NULL`,
  `dalal INTEGER NOT NULL DEFAULT 0`.
- New locked-state CHECK: `total_amount_iqd_snapshot = price + dye` (no report);
  report-coherence (report=0 -> amount 0 and pct/name NULL; report=1 -> pct set);
  internal_pct present iff house mode (doctor NULL AND dalal 0).
- Global CHECK: `dalal = 0 OR doctor_id IS NULL` (mutual exclusion).
- All 12 `visits` indexes (7 from 005 + 5 from 007) are rebuilt verbatim.
- FK integrity preserved via `PRAGMA defer_foreign_keys` inside the migration
  transaction; the single inbound FK is `inventory_adjustments.visit_id`.

`settings`: tombstone `report_cost_iqd`; seed `report_pct` (int, 20) and
`reporting_doctor_name` (text, '').
`check_types`: `DROP COLUMN report_supported`.

Migration verified end to end against a DB built from 001-017: applies clean in
one FK-on transaction; CHECK accept/reject probes pass for house/doctor/dalal,
report on/off, and the old `total=price+dye+report` shape (rejected).

## 4. Sync Server (mirrored — no deferral)

Prisma `Visit`: rename `reportCostSnapshotIqd` -> `reportAmountSnapshotIqd`; add
`reportPctSnapshot`, `reportingDoctorNameSnapshot`, `dalal`. `CheckType`: remove
`reportSupported`. The push validator (`validators.ts`) total invariant becomes
`price + dye`; dalal/internal_pct exclusivity and report coherence added; the
protected-setting key `report_cost_iqd` -> `report_pct`. The reports service net
formula subtracts the report amount. Memory + Prisma sync stores mirror the new
columns and conflict-detection keys.

## 5. Frontend

Patient-facing running total = `price + dye` only — report is removed from the
patient total and shown (if at all) as an internal "reporting doctor share"
line. `report_supported` gating removed. Doctor picker gains a built-in دلال
option (flat 10 IQD). Settings page: `report_cost_iqd` -> `report_pct` +
`reporting_doctor_name`. IPC types, Zod schemas, and en/ar i18n updated.

## 6. Frozen daily close (migration `019_daily_close_report_payable.sql`)

A signed/frozen `daily_close` row stored only the final `net_iqd` (already net of
report) plus the doctor/operator cut totals, with no report line. Migration 019
adds `daily_close.total_report_iqd` (and the Prisma `DailyClose.totalReportIqd`)
so a reopened/historical frozen close itemizes the reporting-doctor payable
exactly like doctor/operator cuts. Threaded through the desktop `FrozenClose`
entity + repo + push payload and the server `DailyCloseSyncRecord` + sync store.
`net_iqd` is unchanged (it already nets out report); this is purely the additive
breakdown line. Forward-only, idempotent `ADD COLUMN` (no table rebuild).

## 7. Schema version

Migrations went 17 -> 19 (018 + 019). Desktop `SYNC_SCHEMA_VERSION` (=
`MIGRATIONS.len()`) and server `SERVER_SCHEMA_VERSION` are both bumped to 19 in
lockstep.

## 8. Conflict Policy

Unchanged. `visits` stays `manual` (version-based); the new report snapshots and
`dalal` ride inside the same versioned visit snapshot. `daily_close` /
`DailyClose` stay `last-write-wins`. `settings` stays `last-write-wins` per key.

## 9. Verification (complete)

- Migration 018 + 019: apply clean against a DB built from 001-017/018; CHECK
  accept/reject probes pass for house/doctor/dalal x report on/off; the old
  `price+dye+report` total is rejected; `PRAGMA foreign_key_check` clean.
- Rust: `cargo check --all-targets` clean; `cargo clippy --all-targets -- -D
  warnings` clean; `cargo fmt --check` clean; `cargo test` 1017 pass / 0 real
  failures (the only red is the pre-existing `shifts_perf_phase04::
  perf_history_today_at_500_rows` timing flake -- a shifts-domain SLO test
  untouched by this work; passes 3/3 in isolation, fails only under full-suite
  parallel CPU load). money_math 43, visits 91, settings 27, reports 44 + 40/13/1
  integration.
- Frontend: `pnpm lint` 0 violations (eslint + i18n parity + RTL); `pnpm build`
  (tsc -b + vite) passes; vitest 1060 pass.
- Sync server: `tsc` typecheck clean; `pnpm test` 314 / 314 pass.
- Cross-surface seams verified: desktop push payload <-> server validator/store
  field names match byte-for-byte for `dalal` + the three report snapshots;
  schema version 19 <-> 19 lockstep; `dalal` coerced `?? false` for pre-019
  clients.
