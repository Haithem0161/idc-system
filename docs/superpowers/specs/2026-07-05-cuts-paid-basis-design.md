# Cuts Computed on the Paid Amount — Design

**Date:** 2026-07-05
**Status:** approved (design), ready for implementation plan
**Surfaces:** SQLite + Tauri/Rust, Sync Server (Prisma/Fastify), Frontend (React)
**Companion:** worked-examples matrix — https://claude.ai/code/artifact/0a42cfef-b3f9-43ce-8e56-b898635ed884

## 1. Problem

The clinic's cut model splits every locked visit into fixed "attribution" cuts and
percentage "doctor" cuts. Two things are wrong today:

1. **Doctor cuts are computed off the billed catalog `price`, not what the patient
   actually paid.** When a receptionist records a collected amount lower than the
   billed total (`amount_paid_override_iqd`), the external/internal/reporting doctor
   cuts do **not** shrink — the override is patched onto the snapshot *after* the
   money engine runs and is deliberately decoupled from every cut
   (`src-tauri/src/domains/visits/service/visit_service.rs:804-807`; migration
   `016_visit_amount_paid_override.sql:13`).
2. **Catalog prices turned out not to be fixed.** The price for a check type varies
   per visit, so the receptionist needs to set the actual price at visit time.

The three attribution roles (operator, representative/مندوب, dalal) are already fixed
and independent of price — that part is correct and stays.

## 2. Current model (verified)

Net, in the reports read-model and daily close
(`src-tauri/src/domains/reports/infrastructure/repositories/sqlite_reports_repo.rs:143`,
`:281`; `service/mod.rs:677`):

```
collected = amount_paid_override_iqd ?? total_amount_iqd_snapshot   (total = price + dye)
net       = collected − doctor_cut − operator_cut − report − mandoub   (− inventory on daily close)
```

Per-role cut basis today (`src-tauri/src/domains/visits/domain/services/money_math.rs`):

| Role | Code term | Basis today | Fixed / scaled |
|-|-|-|-|
| Operator | `operator_cut_snapshot_iqd` | `operator.base_cut_per_check_iqd` | fixed |
| Representative (مندوب) | `mandoub_cut_snapshot_iqd` | 500 or 1000, chosen on the visit | fixed |
| Dalal (دلال) | `dalal` → `doctor_cut` | flat `DALAL_CUT_IQD` = 10,000 | fixed |
| External / referring doctor | `doctor_cut_snapshot_iqd` | per-check pricing → doctor default → 0; `pct` or `fixed` of **price** | scaled off **price** |
| Internal / house doctor | `internal_pct` → `doctor_cut` | `price × internal_doctor_pct / 100` | scaled off **price** |
| Reporting doctor | `report_amount_snapshot_iqd` | `report_pct × (price − doctor_cut) / 100` | scaled off **price** |

## 3. Target model

### 3.1 Editable price

- The per-visit `price` becomes **receptionist-editable**. The catalog/subtype price is
  the prefilled **default**, not a hard value.
- `amount_paid_override_iqd` **stays** as a separate field: "patient paid even less than
  the agreed (editable) price."
- Effective cash in: `collected = amount_paid_override_iqd ?? price`.
- Dye is **still a separate line**, added on top for the patient total, still excluded
  from every doctor-cut base. `total = price + dye` (unchanged invariant).

### 3.2 The cut base

```
collected = amount_paid_override_iqd ?? price
cut_base  = max(0, collected − dye_cost)      // dye is a material cost, covered first
```

### 3.3 Per-role rules

**Scaled (from paid):**
- External / referring doctor, **percentage** cut: `cut = cut_base × pct / 100`
- Internal / house doctor: `cut = cut_base × internal_pct / 100`
- Reporting doctor: `report = report_pct × (cut_base − doctor_cut) / 100`

**Fixed (attribution):**
- Operator: `base_cut_per_check_iqd`
- Representative (مندوب): 500 / 1000
- Dalal: 10,000
- External / referring doctor, **fixed** cut: `fixed_amount` (does **not** scale)

### 3.4 The zero rule (single guard)

When **`cut_base == 0`**, **every cut is forced to 0** — fixed and scaled alike. A
collection that does not even cover the dye leaves nothing to share, so operator,
representative, dalal, and any fixed doctor cut all drop to zero too. This collapses
"paid nothing" and "paid less than the dye" into one condition. It is the *only*
place a fixed cut is suppressed.

### 3.4a Interaction with the `discount` flag

The existing `discount` flag forces the referring doctor's cut to 0 for the visit
(`money_math.rs:119`). It is unchanged and composes cleanly: with discount on,
`doctor_cut = 0` regardless of `cut_base`, and the report base widens to
`cut_base − 0 = cut_base`. The zero-guard (§3.4) still overrides everything when
`cut_base == 0`.

### 3.5 Truthful net

Net may go **negative**. Replace the `saturating_sub` chain in the reports read-model so
a day where cuts exceed collection shows the real loss and reconciles against the cash
drawer. (See the "fixed cuts exceed collection" case in §4: doctor fixed 12k, collected
10k → net −7,000.)

### 3.6 Basis alignment (reconciliation fix)

The dashboard/trend net currently uses a **billed** basis (`revenue_iqd`) while the daily
close uses a **collected** basis (`service/mod.rs:223` vs `:677`). Under overrides they
diverge. Align both to the collected basis so every accounting surface reconciles.

## 4. Worked examples (must reconcile exactly)

Operator flat = 5,000; report_pct = 20%; internal_pct = 40%. Full 12-case matrix is in
the companion artifact; the load-bearing edge cases:

| Case | Price | Dye | Collected | cut_base | Doctor | Report | Op | Rep | Net |
|-|-|-|-|-|-|-|-|-|-|
| House, underpaid (override 30k) | 50,000 | 0 | 30,000 | 30,000 | 12,000 | — | 5,000 | — | 13,000 |
| External % 25%, underpaid (override 60k) | 100,000 | 0 | 60,000 | 60,000 | 15,000 | — | 5,000 | — | 40,000 |
| External **fixed** 12k, underpaid (override 60k) | 100,000 | 0 | 60,000 | 60,000 | 12,000 | — | 5,000 | — | 43,000 |
| Doctor % + report, underpaid (override 60k) | 100,000 | 0 | 60,000 | 60,000 | 15,000 | 9,000 | 5,000 | — | 31,000 |
| Underpaid below dye (collected 5k, dye 8k) | 50,000 | 8,000 | 5,000 | 0 | 0 | 0 | 0 | 0 | −3,000 |
| Paid nothing / waived (collected 0) | 50,000 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Fixed cuts exceed collection (doctor fixed 12k, collected 10k) | 100,000 | 0 | 10,000 | 10,000 | 12,000 | — | 5,000 | — | −7,000 |

Report base after the doctor cut, underpaid case: `20% × (60,000 − 15,000) = 9,000`.

## 5. Surfaces touched

### 5.1 Schema — SQLite + Prisma
- Make per-visit `price` editable: the draft carries a `price_iqd` the receptionist can
  set; catalog/subtype price is the default. Snapshot at lock as today
  (`price_snapshot_iqd`). New forward-only migration; bump `SYNC_SCHEMA_VERSION`
  (= `MIGRATIONS.len()`) **and** server `SERVER_SCHEMA_VERSION` in the same commit
  (lockstep — see root `CLAUDE.md`).
- Locked-state CHECK: `total = price + dye` still holds. Cut-coherence clauses reviewed
  for the paid basis (discount-forces-zero-doctor-cut, dalal flat, mandoub 500/1000,
  report-on-implies-pct all still valid; the *inputs* to those cuts change, not the
  coherence shape).
- Mirror on the server push validator (`sync-server .../validators.ts`).

### 5.2 Engine — Rust `money_math`
- `compute()` takes the effective `collected` amount (and the editable price) as input
  and derives `cut_base = max(0, collected − dye)`.
- Single `cut_base == 0` zero-guard before dispatching any cut.
- Fixed vs scaled dispatch per §3.3; report base = `cut_base − doctor_cut`.
- Rewrite `visit_service::lock` so the collected amount feeds **into** `compute()`
  instead of the after-the-fact overlay at `visit_service.rs:807`.

### 5.3 Reports — Rust + frozen close
- Drop the `saturating_sub` floor (truthful net, §3.5).
- Align dashboard/trend to the collected basis (§3.6).
- Thread editable price into the frozen daily-close inputs and its BLAKE3 freeze hash.
- Regenerate the `money_math` coverage matrix and reports tests around the new cases.

### 5.4 Frontend — React
- Editable price field on the new-visit / visit form, prefilled from the catalog default.
- Running-total and collected math reflect the editable price; the visit-detail cut
  breakdown reflects the paid basis.
- en / ar i18n for the price field and any new labels (RTL verified).

## 6. Conflict policy

Unchanged. `visits` stays manual/version-based; the editable price and every snapshot
ride inside the same versioned visit snapshot. `daily_close` stays last-write-wins.
`settings` stays last-write-wins per key.

## 7. Non-goals

- No new roles or snapshot columns for a distinct "internal doctor" — internal = existing
  house mode, only its basis changes.
- No proportional splitting of a partial override between price and dye — dye is covered
  first (§3.2).
- No change to how mandoub / operator / dalal amounts are chosen — only the zero-guard
  can suppress them.
- No backfill/recompute of historical locked visits or signed closes (pre-launch, no
  production history — consistent with phase 11).
