# Cuts Computed on the Paid Amount — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the doctor-side cuts (external/referring, internal/house, reporting) scale off the amount the patient actually paid (net of dye), add a receptionist-editable per-visit price, keep attribution cuts (operator, representative, dalal) fixed, zero everyone when nothing is left after dye, and report a truthful (negative-capable) net.

**Architecture:** The change centers on the pure Rust money engine `money_math::compute()`. Today it computes every cut off the billed catalog price and the collected-amount override is patched onto the snapshot *after* the engine runs. This plan feeds an editable price and the collected amount *into* the engine so the doctor cuts derive from `cut_base = max(0, collected − dye)`. The editable price lives on the draft visit as a nullable `price_override_iqd` (catalog price is the default when NULL), snapshots into the existing `price_snapshot_iqd` at lock, and threads through all three surfaces (SQLite/Rust, Prisma/Fastify, React) in schema-version lockstep. The reports read-models on both desktop and server drop their net floor and align to a single collected basis.

**Tech Stack:** Tauri v2, Rust 2021 (sqlx, serde, thiserror), SQLite, React 19 + TypeScript (Zustand, Zod v4, TanStack Query, react-i18next), Fastify + Prisma + Postgres (TypeBox validators).

## Global Constraints

- **No emojis** anywhere — code, comments, commit messages, user-facing strings (use i18n keys).
- **No Claude authorship in commits.** No `Co-Authored-By: Claude`, no Anthropic emails, no `git config` changes. Commits appear solely human-made. Author: `Haithem <cloud.torchcorp@gmail.com>`.
- **Always `pnpm`**, never `npm`/`yarn`. Never hand-edit `package.json`/`Cargo.toml` dependency sections (use `pnpm add` / `cargo add`); this plan adds no dependencies.
- **Context7 first** before writing code against any library API.
- **Money is `i64` IQD**, integer arithmetic, truncating division (`a * pct / 100`). No floats in money math.
- **Schema-version lockstep is mandatory.** Desktop `SYNC_SCHEMA_VERSION` (= count of `src-tauri/migrations/*.sql`) and server `SERVER_SCHEMA_VERSION` (`sync-server/src/app/common/version.ts`) MUST match and be bumped in the SAME commit as the new migration. Current value: **22 → 23**.
- **Migrations are forward-only and idempotent.** The runner records applied files by name and runs each exactly once inside a transaction.
- **Never run the full `cargo test`** (it crashes the IDE). Run per-binary: `cargo test --lib <module>` for unit tests, `cargo test --test <binary>` for integration files.
- **Offline-first:** every write commits locally first; the visit stays `manual`/version-based conflict policy; the editable price rides inside the same versioned visit snapshot.
- **Pre-push validation** (do NOT push without): `pnpm lint`, `pnpm build`, `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`, and the sync server's lint/typecheck/tests.

---

## File Structure

**Rust money engine (the core):**
- `src-tauri/src/domains/visits/domain/services/money_math.rs` — `MoneyMathInputs` gains `price_override_iqd` + `amount_paid_override_iqd`; `compute()` derives `cut_base` and dispatches fixed vs scaled cuts through the zero-guard. Its `#[cfg(test)] mod tests` gets the new coverage matrix.

**Rust visit entity + service + repo (thread the editable price):**
- `src-tauri/src/domains/visits/domain/entities/visit.rs` — `price_override_iqd` on `Visit`, `VisitCreateDraftInput`, `VisitDraftPatch`; validation.
- `src-tauri/src/domains/visits/service/visit_service.rs` — `CreateDraftInput`/`UpdateDraftInput` DTOs carry the price; `resolve_bundle` and `lock` feed `price_override` + `amount_paid_override` into `compute()`; remove the after-the-fact override overlay.
- `src-tauri/src/domains/visits/infrastructure/repositories/sqlite_visit_repo.rs` — persist/read `price_override_iqd` in the upsert `COLUMNS` list and the row struct.
- `src-tauri/src/domains/visits/commands.rs` — `visits_create_draft` / `visits_update_draft` / `visits_lock` args carry the editable price; `visits_pricing_resolve` uses it.

**Rust reports (truthful net + basis alignment):**
- `src-tauri/src/domains/reports/infrastructure/repositories/sqlite_reports_repo.rs` — drop `saturating_sub` (allow negative net) in per-visit and grouped net.
- `src-tauri/src/domains/reports/service/mod.rs` — dashboard/trend net moves from `revenue_iqd` (billed) to the collected basis, and drops the floor.

**SQLite migration:**
- `src-tauri/migrations/023_visit_price_override.sql` — add `price_override_iqd INTEGER NULL` to `visits`.

**Desktop schema version:**
- Wherever `SYNC_SCHEMA_VERSION` is defined (Task 1 locates it) — auto-derives from migration count; verify it reads 23.

**Server (Prisma + validator + reports + sync store):**
- `sync-server/prisma/schema.prisma` — `Visit.priceOverrideIqd` → `@map("price_override_iqd")`.
- `sync-server/src/app/common/version.ts` — `SERVER_SCHEMA_VERSION = 23`.
- `sync-server/src/app/sync/service/validators.ts` — accept `price_override_iqd`; total invariant unchanged (validates the sent price).
- `sync-server/src/app/domains/reports/service/reports-service.ts` — per-visit/grouped net to collected basis; drop floors.
- `sync-server/src/app/sync/infrastructure/prisma/entity-store.ts` + `.../memory/store.ts` — add `price_override_iqd` to the visit snapshot/conflict-key arrays.

**Frontend (editable price field):**
- `src/lib/schemas/visit.ts` — `price_override_iqd` on create/update draft schemas.
- `src/lib/ipc.ts` — IPC arg + snapshot record types.
- `src/stores/visit-tabs-store.ts` — `priceOverrideIqd` on `VisitTabForm`.
- `src/pages/reception/new-visit-tabbed.tsx` — editable price input, prefilled from `pricing_effective`; total math.
- `src/features/visits/queries.ts` — create/update/lock mutations send the price.
- `src/i18n/locales/{en,ar}/reception.json` — `price` label + hint keys.

---

## Task 1: SQLite migration — editable per-visit price

**Files:**
- Create: `src-tauri/migrations/023_visit_price_override.sql`
- Test: `src-tauri/tests/` (migration applies via the existing runner; verified by a probe below)

**Interfaces:**
- Produces: a nullable `visits.price_override_iqd INTEGER` column (NULL = use catalog default). Consumed by the repo (Task 5), entity (Task 3), and money engine (Task 2) downstream.

**Context:** Today the draft carries NO price column — `price_snapshot_iqd` is NULL on drafts and filled at lock from the catalog (`005_patients_visits_adjustments.sql:59`, `82-83`). The draft locked-state CHECK requires `price_snapshot_iqd IS NULL` while draft. The editable price is a *draft-time* value, so it needs its own column that is legal in every status. This is a plain additive `ADD COLUMN` (no table rebuild): the existing locked CHECK never references `price_override_iqd`, so it keeps holding. Mirrors the additive pattern of `022_visits_discount.sql`.

- [ ] **Step 1: Write the migration file**

Create `src-tauri/migrations/023_visit_price_override.sql`:

```sql
-- Phase 13: receptionist-editable per-visit price.
--
-- Catalog / subtype prices turned out not to be fixed. The receptionist may now
-- set the actual price for a visit at draft time. `price_override_iqd` holds that
-- chosen price; NULL means "use the catalog/subtype default" (the historical
-- behaviour). At lock the effective price is snapshotted into the existing
-- `price_snapshot_iqd` exactly as before, so the locked-state CHECK
-- (total = price + dye) is unchanged.
--
-- This value also becomes an INPUT to the money engine: the doctor-side cuts now
-- scale off what the patient paid, and the editable price is what "paid" defaults
-- to (see the money_math change in the same feature). The column is legal in
-- every status (draft/locked/voided), so no locked-CHECK clause references it and
-- a plain additive ADD COLUMN suffices -- no table rebuild.
--
-- Forward-only, idempotent: nullable add, no backfill. Existing rows get NULL
-- (= catalog default), the correct historical value.
--
-- Conflict policy: unchanged. `visits` stays manual/version-based; the editable
-- price rides inside the same versioned visit row alongside dye/report/dalal/
-- discount.
ALTER TABLE visits ADD COLUMN price_override_iqd INTEGER NULL;
```

- [ ] **Step 2: Apply the migration against a scratch DB and probe it**

Run (builds a DB through 001-023 using the app's own runner via a throwaway test, or manually with sqlite3):

```bash
cd src-tauri && cargo test --test migrations_apply 2>/dev/null || \
  echo "no migrations_apply test binary; verifying with sqlite3 below"
```

Then verify the column exists and drafts still insert with it NULL:

```bash
cd src-tauri && TMPDB=$(mktemp) && \
  for f in migrations/0*.sql; do sqlite3 "$TMPDB" < "$f"; done && \
  sqlite3 "$TMPDB" "PRAGMA table_info(visits);" | grep price_override_iqd && \
  sqlite3 "$TMPDB" "PRAGMA foreign_key_check;" && \
  echo "OK migration applies, column present, FK clean" && rm -f "$TMPDB"
```

Expected: a line `...|price_override_iqd|INTEGER|0||0`, no foreign_key_check output, and `OK ...`.

- [ ] **Step 3: Bump the desktop schema version in lockstep**

Locate the desktop schema-version constant and confirm it now reads 23:

```bash
cd src-tauri && grep -rn "SYNC_SCHEMA_VERSION" src/ && ls migrations/0*.sql | wc -l
```

Expected: the `ls | wc -l` prints `23`. If `SYNC_SCHEMA_VERSION` is derived from `MIGRATIONS.len()` (a `include_str!` list or a build-time count), it auto-advances to 23 — no edit needed; confirm by reading the definition. If it is a hardcoded constant, change it from `22` to `23` here and note the file.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/migrations/023_visit_price_override.sql
git commit -m "feat(visits): add editable per-visit price_override_iqd column (migration 023)"
```

---

## Task 2: Money engine — cuts off the paid amount

**Files:**
- Modify: `src-tauri/src/domains/visits/domain/services/money_math.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `Doctor`, `Operator`, `DoctorCheckPricing`, `CheckType`, `CheckSubtype`, `MoneySettings` (unchanged), plus two new `MoneyMathInputs` fields.
- Produces: `MoneyMathInputs` gains `pub price_override_iqd: Option<i64>` and `pub amount_paid_override_iqd: Option<i64>`. `compute()` signature is unchanged (`fn compute(inputs: &MoneyMathInputs<'_>) -> AppResult<VisitSnapshots>`). The returned `VisitSnapshots` is unchanged in shape; only the *values* of `doctor_cut_iqd`, `report_amount_iqd`, `operator_cut_iqd`, `mandoub_cut_iqd` change under the paid basis. A new private helper `fn cut_base(price_iqd: i64, collected: i64, dye_cost: i64) -> i64` returns `max(0, collected − dye_cost)`. `VisitSnapshots.amount_paid_override_iqd` is now SET by `compute()` (previously always `None`).

**Context (verified against the current file):** Today `compute()` computes `price_iqd` from catalog/subtype/pricing override, `dye_cost`, then `cuts()` computes doctor/operator/internal cuts off **`price_iqd`**. Report is `report_pct * (price_iqd - doctor_cut) / 100`. The dalal flat cut is `DALAL_CUT_IQD = 10_000`. `amount_paid_override_iqd` is hardcoded `None` at the end (line ~165) and patched in later by `visit_service::lock`. This task moves the paid amount *into* the engine.

The target math (from the design spec §3):

```
price_iqd = price_override_iqd ?? (subtype/pricing/catalog price)   // editable price wins
collected = amount_paid_override_iqd ?? price_iqd
cut_base  = max(0, collected − dye_cost)
if cut_base == 0 { all cuts (doctor, operator, mandoub, report) = 0 }
else:
  scaled doctor cuts use cut_base instead of price_iqd
  fixed cuts (operator base, mandoub 500/1000, dalal 10k, doctor FIXED cut) apply as-is
  report = report_pct * (cut_base − doctor_cut) / 100
total_amount_iqd = price_iqd + dye_cost   // unchanged; report NOT in patient total
```

- [ ] **Step 1: Write the failing tests (paid-basis coverage matrix)**

Add these tests to the `tests` module in `money_math.rs`. They encode the design's worked examples. `settings()` already provides `dye_cost_iqd: 2000, report_pct: 20, internal_doctor_pct: 40`; `operator()` has `base_cut_per_check_iqd: 5000`.

```rust
    // ---- paid-basis cut tests (feature: cuts off paid amount) ------------

    fn inputs_house<'a>(
        ct: &'a CheckType,
        op: &'a Operator,
        price_override: Option<i64>,
        paid: Option<i64>,
        dye: bool,
        report: bool,
    ) -> MoneyMathInputs<'a> {
        MoneyMathInputs {
            check_type: ct,
            check_subtype: None,
            doctor: None,
            doctor_pricing: None,
            operator: op,
            patient_name: "p",
            dye,
            report,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: price_override,
            amount_paid_override_iqd: paid,
            settings: settings(),
        }
    }

    #[test]
    fn house_underpaid_scales_internal_cut_off_collected() {
        // price 50k, no override on price, collected 30k, internal 40%.
        // cut_base = 30000; doctor_cut = 30000*40/100 = 12000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&inputs_house(&ct, &op, None, Some(30_000), false, false)).unwrap();
        assert_eq!(snap.price_iqd, 50_000);
        assert_eq!(snap.doctor_cut_iqd, 12_000);
        assert_eq!(snap.operator_cut_iqd, 5_000); // fixed, unchanged
        assert_eq!(snap.amount_paid_override_iqd, Some(30_000));
        assert_eq!(snap.total_amount_iqd, 50_000); // price + dye(0), unchanged
    }

    #[test]
    fn editable_price_override_replaces_catalog_price() {
        // catalog 50k but receptionist sets price 80k, paid in full.
        // cut_base = 80000; internal 40% = 32000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&inputs_house(&ct, &op, Some(80_000), None, false, false)).unwrap();
        assert_eq!(snap.price_iqd, 80_000);
        assert_eq!(snap.doctor_cut_iqd, 32_000);
        assert_eq!(snap.total_amount_iqd, 80_000);
    }

    #[test]
    fn external_doctor_pct_scales_off_collected() {
        // price 100k, doctor pct 25, collected 60k -> cut_base 60k -> 15000.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            amount_paid_override_iqd: Some(60_000),
            ..inputs_house(&ct, &op, None, None, false, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 15_000);
        assert_eq!(snap.internal_pct, None);
    }

    #[test]
    fn external_doctor_fixed_cut_does_not_scale() {
        // price 100k, doctor FIXED 12k, collected 60k. Fixed stays 12000.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            amount_paid_override_iqd: Some(60_000),
            ..inputs_house(&ct, &op, None, None, false, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 12_000);
    }

    #[test]
    fn report_base_uses_cut_base_after_doctor_cut() {
        // price 100k, doctor pct 25, collected 60k, report 20%.
        // cut_base 60k, doctor_cut 15k, report = 20% * (60000-15000) = 9000.
        let ct = ct(false, Some(100_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            amount_paid_override_iqd: Some(60_000),
            ..inputs_house(&ct, &op, None, None, false, true)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 15_000);
        assert_eq!(snap.report_amount_iqd, 9_000);
    }

    #[test]
    fn zero_cut_base_zeroes_every_cut_including_fixed() {
        // price 50k, dye 2000, collected 5000 -> cut_base = max(0, 5000-2000)=3000?
        // NO: choose collected below dye. collected 1500, dye 2000 -> base 0.
        // Everyone (operator fixed 5000, mandoub, doctor) zeroes.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Fixed, 12_000, None);
        let snap = compute(&MoneyMathInputs {
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            dye: true,
            amount_paid_override_iqd: Some(1_500),
            mandoub_cut_iqd: 1_000,
            mandoub_name: Some("Rep"),
            ..inputs_house(&ct, &op, None, None, true, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 0);
        assert_eq!(snap.operator_cut_iqd, 0);
        assert_eq!(snap.mandoub_cut_iqd, 0);
        assert_eq!(snap.report_amount_iqd, 0);
        // Patient total is still price + dye regardless of the zero cuts.
        assert_eq!(snap.total_amount_iqd, 52_000);
    }

    #[test]
    fn paid_full_default_matches_price_when_no_overrides() {
        // No price override, no paid override -> collected = price, cut_base = price.
        // Identical to the legacy house behaviour: internal 40% of 50000 = 20000.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&inputs_house(&ct, &op, None, None, false, false)).unwrap();
        assert_eq!(snap.doctor_cut_iqd, 20_000);
        assert_eq!(snap.amount_paid_override_iqd, None);
    }

    #[test]
    fn dalal_flat_cut_survives_partial_payment_when_base_positive() {
        // dalal flat 10k; collected 40k (>0 after dye 0) -> base 40k, dalal stays 10k.
        let ct = ct(false, Some(50_000));
        let op = operator();
        let snap = compute(&MoneyMathInputs {
            dalal: true,
            amount_paid_override_iqd: Some(40_000),
            ..inputs_house(&ct, &op, None, None, false, false)
        })
        .unwrap();
        assert_eq!(snap.doctor_cut_iqd, 10_000);
        assert!(snap.internal_pct.is_none());
    }
```

- [ ] **Step 2: Run the new tests to verify they fail to compile / fail**

Run:

```bash
cd src-tauri && cargo test --lib domains::visits::domain::services::money_math 2>&1 | tail -25
```

Expected: compile error — `MoneyMathInputs` has no field `price_override_iqd` / `amount_paid_override_iqd`. (This is the expected "red".)

- [ ] **Step 3: Add the two input fields**

In `money_math.rs`, add to `MoneyMathInputs` (after `mandoub_name`, before `settings`):

```rust
    /// Receptionist-editable price for this visit. `Some(p)` overrides the
    /// catalog/subtype/pricing price; `None` uses the resolved catalog price.
    /// Becomes the snapshot `price_iqd` and the default "collected" basis.
    pub price_override_iqd: Option<i64>,
    /// Cash actually collected. `Some(c)` (incl. 0) is the collected amount;
    /// `None` means the patient paid the full price. The doctor-side cuts scale
    /// off `max(0, collected - dye)`.
    pub amount_paid_override_iqd: Option<i64>,
```

- [ ] **Step 4: Thread the editable price into the base price**

Change `effective_price` (or add a wrapper) so the editable override wins over everything. Replace the `price_iqd` computation near the top of `compute()`:

```rust
    let base_price = base_price(inputs)?;
    let catalog_price = effective_price(base_price, inputs.doctor_pricing);
    // Receptionist-editable price wins over the catalog/pricing price.
    let price_iqd = inputs.price_override_iqd.unwrap_or(catalog_price);
    if price_iqd < 0 {
        return Err(AppError::Validation("price_override_iqd must be >= 0".into()));
    }
```

- [ ] **Step 5: Compute the cut base and thread it through**

After `let dye_cost = ...;` in `compute()`, add:

```rust
    // Collected cash defaults to the (editable) price when no override is set.
    let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd);
    if collected < 0 {
        return Err(AppError::Validation(
            "amount_paid_override_iqd must be >= 0".into(),
        ));
    }
    // Cut base: collected minus dye (dye is a material cost, covered first).
    // When this hits 0, EVERY cut zeroes -- fixed and scaled alike.
    let base = cut_base(price_iqd, collected, dye_cost);
```

Add the helper near `cut_from_kind_value`:

```rust
/// The base every cut is measured against: collected cash net of dye, floored
/// at zero. When it is zero, no cut (fixed or scaled) is paid.
fn cut_base(_price_iqd: i64, collected: i64, dye_cost: i64) -> i64 {
    (collected - dye_cost).max(0)
}
```

- [ ] **Step 6: Route cuts through the base and apply the zero-guard**

Change the `cuts()` call to pass `base` where it currently passes `price_iqd`, and rename its price parameter to `cut_base` internally (it already applies pct to that value and returns the operator base). Then wrap the whole cut/report block in the zero-guard. Replace the section from `let (computed_doctor_cut, ...) = cuts(...)` through the `report_amount` computation with:

```rust
    let (doctor_cut, internal_pct, operator_cut, mandoub_cut, report_amount) = if base == 0 {
        // Zero-guard: nothing left after dye -> nobody is paid, not even fixed
        // entities. internal_pct still marks house mode for the lock invariant.
        let internal_pct = if inputs.doctor.is_none() && !inputs.dalal {
            if !(0..=100).contains(&inputs.settings.internal_doctor_pct) {
                return Err(AppError::Validation(
                    "internal_doctor_pct must be 0..=100".into(),
                ));
            }
            Some(inputs.settings.internal_doctor_pct)
        } else {
            None
        };
        (0, internal_pct, 0, 0, 0)
    } else {
        let (computed_doctor_cut, internal_pct, operator_cut) = cuts(
            base,
            inputs.operator,
            inputs.doctor,
            inputs.dalal,
            inputs.doctor_pricing,
            &inputs.settings,
        )?;
        // Discount forces the referring doctor's cut to 0 (unchanged semantics).
        let doctor_cut = if inputs.discount && inputs.doctor.is_some() {
            0
        } else {
            computed_doctor_cut
        };
        // Report is a pct of the base AFTER the doctor cut.
        let report_amount = if inputs.report {
            if !(0..=100).contains(&inputs.settings.report_pct) {
                return Err(AppError::Validation("report_pct must be 0..=100".into()));
            }
            (base - doctor_cut).max(0) * inputs.settings.report_pct / 100
        } else {
            0
        };
        (
            doctor_cut,
            internal_pct,
            operator_cut,
            inputs.mandoub_cut_iqd,
            report_amount,
        )
    };
```

Then, below, update the snapshot assembly to use `mandoub_cut` (the guarded value) for `mandoub_cut_iqd`, keep `report_pct`/`reporting_doctor_name` logic as-is (they depend on the `report` flag, not the amount), and SET the override:

```rust
        mandoub_cut_iqd: mandoub_cut,
        // ...
        amount_paid_override_iqd: inputs.amount_paid_override_iqd,
```

**Note:** the `mandoub_name` snapshot must still be captured when a مندوب is referenced even if the cut zeroed, because the lock CHECK (migration 021) requires `mandoub_cut_snapshot IN (500,1000)` when `mandoub_id` is set. See Task 4 — the zero-guard case for a مندوب visit is reconciled there (a مندوب visit that collects below dye is rejected at lock rather than silently zeroing the مندوب, OR the CHECK is relaxed). Resolve per Task 4's decision; for the engine, when `base == 0` the returned `mandoub_cut` is 0 and `mandoub_name` follows the existing rule.

- [ ] **Step 7: Update the `cuts()` doc/param name**

Rename the first parameter of `cuts()` from `price_iqd` to `cut_base` for clarity (pure rename; the body already applies `* value / 100` to it and returns the operator base cut). Update the dalal branch comment; `DALAL_CUT_IQD` stays flat.

- [ ] **Step 8: Fix all existing tests that construct `MoneyMathInputs`**

Every existing test literal in this file now needs `price_override_iqd: None, amount_paid_override_iqd: None`. Add both fields to each existing `MoneyMathInputs { ... }` in the test module. The legacy tests assert cuts off full price; with no overrides `collected == price` and `cut_base == price − dye`. **Important:** legacy tests like `flat_house_with_dye` expect `doctor_cut = 20_000` (40% of 50000) but with dye 2000 the new `cut_base = 50000 − 2000 = 48000` → `19_200`. These legacy expectations must be updated to the paid-basis values, OR use `dye: false` where the test's intent was the cut, not the dye. Update each legacy assertion to the recomputed value (dye now reduces the cut base). Document each changed expectation with a one-line comment.

- [ ] **Step 9: Run the money_math tests**

Run:

```bash
cd src-tauri && cargo test --lib domains::visits::domain::services::money_math 2>&1 | tail -20
```

Expected: all pass (new matrix + updated legacy).

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/domains/visits/domain/services/money_math.rs
git commit -m "feat(money): compute doctor cuts off paid amount (cut_base), zero-guard on empty base"
```

---

## Task 3: Visit entity — carry the editable price

**Files:**
- Modify: `src-tauri/src/domains/visits/domain/entities/visit.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing new.
- Produces: `Visit` gains `pub price_override_iqd: Option<i64>`; `VisitCreateDraftInput` and `VisitDraftPatch` gain matching fields (`price_override_iqd: Option<i64>` on create, `price_override_iqd: Option<Option<i64>>` on the patch — outer Some = change it, inner None = clear back to catalog default). `create_draft` and `edit_draft` set/patch it with a `>= 0` validation.

**Context:** `Visit` currently has `dye/report/dalal/discount` bools and the snapshot block. The editable price is a draft-time scalar (nullable). It must survive `edit_draft` and be readable at lock. It does NOT participate in the lock snapshot invariants directly (the snapshot's `price_iqd` is what the money engine produced); it is purely the *input*.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `visit.rs`:

```rust
    #[test]
    fn create_draft_defaults_price_override_none() {
        let v = Visit::create_draft(draft_input()).unwrap();
        assert_eq!(v.price_override_iqd, None);
    }

    #[test]
    fn create_draft_accepts_price_override() {
        let mut input = draft_input();
        input.price_override_iqd = Some(80_000);
        let v = Visit::create_draft(input).unwrap();
        assert_eq!(v.price_override_iqd, Some(80_000));
    }

    #[test]
    fn create_draft_rejects_negative_price_override() {
        let mut input = draft_input();
        input.price_override_iqd = Some(-1);
        assert!(Visit::create_draft(input).is_err());
    }

    #[test]
    fn edit_draft_sets_and_clears_price_override() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let set = v
            .clone()
            .edit_draft(VisitDraftPatch {
                price_override_iqd: Some(Some(90_000)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(set.price_override_iqd, Some(90_000));
        let cleared = set
            .edit_draft(VisitDraftPatch {
                price_override_iqd: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(cleared.price_override_iqd, None);
    }
```

- [ ] **Step 2: Run to verify failure**

Run:

```bash
cd src-tauri && cargo test --lib domains::visits::domain::entities::visit 2>&1 | tail -15
```

Expected: compile error — no field `price_override_iqd`.

- [ ] **Step 3: Add the field to `Visit`, the create input, and the patch**

In `visit.rs`:
- On `struct Visit`, after `discount: bool,`: `pub price_override_iqd: Option<i64>,`
- On `struct VisitCreateDraftInput`, after `discount: bool,`: `pub price_override_iqd: Option<i64>,`
- On `struct VisitDraftPatch` (has `#[derive(Default)]`), after `discount: Option<bool>,`:

```rust
    /// `Some(Some(p))` sets the editable price, `Some(None)` clears it back to
    /// the catalog default, `None` leaves it unchanged.
    pub price_override_iqd: Option<Option<i64>>,
```

- [ ] **Step 4: Set it in `create_draft` and validate**

In `create_draft`, before building the struct, add:

```rust
        if let Some(p) = input.price_override_iqd {
            if p < 0 {
                return Err(AppError::Validation(
                    "price_override_iqd must be >= 0".into(),
                ));
            }
        }
```

And in the returned `Self { ... }`, after `discount: input.discount,`: `price_override_iqd: input.price_override_iqd,`

- [ ] **Step 5: Patch it in `edit_draft`**

In `edit_draft`, alongside the other patch applications:

```rust
        if let Some(price) = patch.price_override_iqd {
            if let Some(p) = price {
                if p < 0 {
                    return Err(AppError::Validation(
                        "price_override_iqd must be >= 0".into(),
                    ));
                }
            }
            self.price_override_iqd = price;
        }
```

- [ ] **Step 6: Fix the test constructors and any other `Visit`/`VisitCreateDraftInput` literals**

Add `price_override_iqd: None` to `draft_input()` and every `Visit { ... }` / `VisitCreateDraftInput { ... }` literal in this file's tests (grep within the file). Also add `price_override_iqd: None` to the `snap_*` helpers if they build `Visit` (they build `VisitSnapshots`, not `Visit`, so likely unaffected — confirm).

- [ ] **Step 7: Run the entity tests**

Run:

```bash
cd src-tauri && cargo test --lib domains::visits::domain::entities::visit 2>&1 | tail -15
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/domains/visits/domain/entities/visit.rs
git commit -m "feat(visits): carry editable price_override_iqd on the visit draft entity"
```

---

## Task 4: Reconcile the مندوب zero-guard at lock

**Files:**
- Modify: `src-tauri/src/domains/visits/domain/entities/visit.rs` (the `lock()` مندوب CHECK)
- Modify: `src-tauri/migrations/023_visit_price_override.sql` (only if the CHECK must relax — decided below)
- Test: `visit.rs` tests

**Interfaces:**
- Consumes: `VisitSnapshots` from Task 2.
- Produces: a coherent lock rule for the corner case "a مندوب visit whose `cut_base` is 0". Decision: **reject the lock** with a clear domain error rather than silently dropping the مندوب cut, because migration 021's locked CHECK hard-requires `mandoub_cut_snapshot IN (500,1000)` when `mandoub_id` is set, and relaxing that CHECK would weaken an existing invariant. This keeps the DB CHECK intact.

**Context:** Migration 021's locked CHECK: `mandoub_id IS NOT NULL → mandoub_cut_snapshot_iqd IN (500,1000)`. The Task 2 zero-guard makes `compute()` return `mandoub_cut = 0` when `base == 0`. A مندوب visit that collects below dye would then produce `mandoub_cut = 0`, which the DB CHECK and `Visit::lock`'s existing مندوب clause reject. Rather than let that surface as an opaque CHECK failure, detect it in `lock()` and return an explicit error: a مندوب visit cannot be locked when nothing remains after dye.

- [ ] **Step 1: Write the failing test**

Add to `visit.rs` tests:

```rust
    #[test]
    fn lock_mandoub_visit_with_zeroed_cut_base_is_rejected_clearly() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.mandoub_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        // Snapshot the engine would produce when base==0: mandoub cut 0.
        let mut snap = snap_doctor(50_000, "Dr");
        snap.mandoub_cut_iqd = 0;
        snap.mandoub_name = Some("Rep".into());
        let err = v.lock(Uuid::now_v7(), snap, Utc::now());
        assert!(err.is_err());
    }
```

- [ ] **Step 2: Run to verify current behaviour**

Run:

```bash
cd src-tauri && cargo test --lib domains::visits::domain::entities::visit::tests::lock_mandoub_visit_with_zeroed_cut_base_is_rejected_clearly 2>&1 | tail -12
```

Expected: PASS already (the existing مندوب clause rejects cut 0), OR FAIL if the name-present/cut-zero combination slips through. If it already passes, keep the test as a regression guard and improve only the error message in Step 3.

- [ ] **Step 3: Make the error explicit**

In `lock()`, within the `if self.mandoub_id.is_some()` block, before the `matches!(500|1000)` check, add:

```rust
            // A مندوب visit that collected nothing after dye produces a zeroed
            // cut base (money engine zero-guard). Rather than fail the opaque
            // 500/1000 CHECK, reject with a precise reason.
            if snapshots.mandoub_cut_iqd == 0 {
                return Err(AppError::Validation(
                    "cannot lock a mandoub visit with no collectable amount after dye".into(),
                ));
            }
```

- [ ] **Step 4: Run the entity tests**

Run:

```bash
cd src-tauri && cargo test --lib domains::visits::domain::entities::visit 2>&1 | tail -12
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domains/visits/domain/entities/visit.rs
git commit -m "feat(visits): reject locking a mandoub visit with zeroed cut base with a clear error"
```

---

## Task 5: Visit repository — persist the editable price

**Files:**
- Modify: `src-tauri/src/domains/visits/infrastructure/repositories/sqlite_visit_repo.rs`
- Test: an existing repo integration test file under `src-tauri/tests/` (locate with grep in Step 1) OR add a focused round-trip test.

**Interfaces:**
- Consumes: `Visit.price_override_iqd` (Task 3).
- Produces: the visit upsert reads/writes `price_override_iqd`. The row struct (`VisitRow` or similar, the one with `price_snapshot_iqd: Option<i64>` at line ~55) gains `price_override_iqd: Option<i64>` and maps it into the reconstituted `Visit`.

**Context:** The repo persists via a single `COLUMNS` const list and an `INSERT ... ON CONFLICT DO UPDATE` with `.bind(...)` calls (lines ~151-256). The row-to-entity mapper (`into_domain` / `from_row`, ~line 92) builds the `Visit`.

- [ ] **Step 1: Locate the repo test binary and read the column plumbing**

Run:

```bash
cd src-tauri && grep -rln "visits" tests/ | head && \
  grep -n "COLUMNS\|price_snapshot_iqd\|amount_paid_override_iqd\|into_domain\|fn upsert" \
  src/domains/visits/infrastructure/repositories/sqlite_visit_repo.rs | head -30
```

- [ ] **Step 2: Add the column to the row struct and the mapper**

In the row struct (has `price_snapshot_iqd: Option<i64>`), add `price_override_iqd: Option<i64>,`. In the `into_domain`/`from_row` builder for `Visit`, set `price_override_iqd: self.price_override_iqd,`.

- [ ] **Step 3: Add the column to the upsert**

- Add `price_override_iqd` to the `COLUMNS` list (the SELECT/INSERT column names, near `price_snapshot_iqd`).
- Add `price_override_iqd = excluded.price_override_iqd,` to the `ON CONFLICT DO UPDATE SET` list.
- Add the matching `.bind(v.price_override_iqd)` in the correct positional order (place it adjacent to the `price_snapshot_iqd` bind so ordinal alignment holds — verify the SELECT column order matches the bind order).

- [ ] **Step 4: Add a round-trip test**

In the located repo test file, add a test that creates a draft with `price_override_iqd: Some(77_000)`, saves, reloads, and asserts it round-trips; and one with `None`.

```rust
#[tokio::test]
async fn price_override_round_trips_through_repo() {
    // ... build pool + migrations as sibling tests do ...
    let mut input = /* draft input helper */;
    input.price_override_iqd = Some(77_000);
    let v = Visit::create_draft(input).unwrap();
    repo.upsert(&v).await.unwrap();
    let loaded = repo.get_by_id(v.id).await.unwrap().unwrap();
    assert_eq!(loaded.price_override_iqd, Some(77_000));
}
```

Adapt the setup to match the sibling tests' fixture pattern in that file.

- [ ] **Step 5: Run the repo test**

Run (per-binary, never the full suite):

```bash
cd src-tauri && cargo test --test <located_binary> price_override_round_trips 2>&1 | tail -15
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/domains/visits/infrastructure/repositories/sqlite_visit_repo.rs src-tauri/tests/
git commit -m "feat(visits): persist price_override_iqd through the sqlite visit repo"
```

---

## Task 6: Visit service + commands — feed price and paid into the engine

**Files:**
- Modify: `src-tauri/src/domains/visits/service/visit_service.rs`
- Modify: `src-tauri/src/domains/visits/commands.rs`
- Test: `visit_service.rs` tests and/or an integration test under `src-tauri/tests/`

**Interfaces:**
- Consumes: `Visit.price_override_iqd` (Task 3), `MoneyMathInputs { price_override_iqd, amount_paid_override_iqd }` (Task 2).
- Produces: `CreateDraftInput`/`UpdateDraftInput` gain the editable price; `resolve_bundle` accepts and forwards `amount_paid_override` into `compute()`; `lock` passes both the visit's `price_override_iqd` and the lock-arg `amount_paid_override_iqd` into `compute()` and **removes** the `snap.amount_paid_override_iqd = amount_paid_override_iqd;` overlay (the engine now sets it). Command structs `VisitLockArgs`, create/update args carry the price.

**Context (verified):** `resolve_bundle` (line 603) builds `MoneyMathInputs` at line 699 and returns the snapshot. `lock` (line 730) calls `resolve_bundle`, then overlays the override at line 807. `resolve_snapshots` (the dry-run for the live form, line 581) also calls `resolve_bundle` — it must pass the draft's `price_override_iqd` but `amount_paid_override = None` (draft has no collected amount yet), so the dry-run total previews at full price. `money_settings` (commands.rs:39) is unchanged.

- [ ] **Step 1: Write a failing service/integration test**

Add a test that locks a visit with a price override and a lower collected amount and asserts the doctor cut scaled. Use the sibling integration test harness (locate with `grep -rln "visits_lock\|VisitService::" src-tauri/tests`). Example assertion intent:

```rust
// House visit, editable price 100k, collected 40k, internal 40%.
// Expect doctor_cut_snapshot = 40000*40/100 = 16000, not 40000.
```

If the service has in-module unit tests with a mock repo, prefer those; otherwise add an integration test.

- [ ] **Step 2: Run to verify failure**

Run the located test; expected FAIL (cut still computed off full price until wired).

- [ ] **Step 3: Add the editable price to the service DTOs**

In `visit_service.rs`:
- `CreateDraftInput` (line 56): add `#[serde(default)] pub price_override_iqd: Option<i64>,`
- `UpdateDraftInput` (line 77): add `pub price_override_iqd: Option<Option<i64>>,`
- In the `create_draft` service method, map it into `VisitCreateDraftInput { ..., price_override_iqd: input.price_override_iqd }`.
- In the `update_draft` service method, map it into `VisitDraftPatch { ..., price_override_iqd: input.price_override_iqd }`.

- [ ] **Step 4: Thread the collected amount through `resolve_bundle`**

Change `resolve_bundle`'s signature to accept the collected override:

```rust
    async fn resolve_bundle(
        &self,
        visit: Visit,
        operator_id: &Option<Uuid>,
        mandoub_cut_iqd: i64,
        amount_paid_override_iqd: Option<i64>,
        settings: MoneySettings,
    ) -> AppResult<(VisitSnapshots, LockBundle)> {
```

In the `MoneyMathInputs { ... }` literal (line 699), add:

```rust
            price_override_iqd: visit.price_override_iqd,
            amount_paid_override_iqd,
```

- [ ] **Step 5: Update the two `resolve_bundle` callers**

- `resolve_snapshots` (dry-run, line 595): pass `None` for the collected override (draft has no collected amount; the form previews at full price). Update the call to `resolve_bundle(visit, &placeholder, 0, None, settings)`.
- `lock` (line 801): pass the lock arg through: `resolve_bundle(current.clone(), &Some(operator_id), mandoub_cut, amount_paid_override_iqd, settings)`.

- [ ] **Step 6: Remove the after-the-fact overlay**

In `lock`, delete the line:

```rust
        snap.amount_paid_override_iqd = amount_paid_override_iqd;
```

and the `mut` on `let (mut snap, bundle)` becomes `let (snap, bundle)` (the engine now sets the override). Keep the boundary `paid < 0` guard at the top of `lock` (it is a cheap early check).

- [ ] **Step 7: Add the editable price to the command args**

In `commands.rs`:
- The create-draft command args struct: add `#[serde(default)] pub price_override_iqd: Option<i64>,` and map into `CreateDraftInput`.
- The update-draft command args struct: add `pub price_override_iqd: Option<Option<i64>>,` and map into `UpdateDraftInput`.
- `VisitLockArgs` already carries `amount_paid_override_iqd` (line 450) — no change; it flows into `lock`.

- [ ] **Step 8: Run the service tests + a broad compile check**

Run:

```bash
cd src-tauri && cargo check --all-targets 2>&1 | tail -20 && \
  cargo test --test <located_binary> 2>&1 | tail -15
```

Expected: compiles; the new lock-with-override test passes (doctor cut scaled).

- [ ] **Step 9: Run clippy + fmt**

Run:

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -15
```

Expected: no warnings.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/domains/visits/service/visit_service.rs src-tauri/src/domains/visits/commands.rs src-tauri/tests/
git commit -m "feat(visits): feed editable price and collected amount into the money engine at lock"
```

---

## Task 7: Reports read-model — truthful net + collected basis

**Files:**
- Modify: `src-tauri/src/domains/reports/infrastructure/repositories/sqlite_reports_repo.rs`
- Modify: `src-tauri/src/domains/reports/service/mod.rs`
- Test: `src-tauri/src/domains/reports/**` unit tests (in-file) and/or `src-tauri/tests/` reports integration.

**Interfaces:**
- Consumes: the visit snapshot columns (unchanged names).
- Produces: net may be negative (no `saturating_sub`); the dashboard/trend net uses `collected_iqd` not `revenue_iqd`. `VisitsAggregate` already carries both `revenue_iqd` and `collected_iqd` (verified) — the dashboard just switches which it subtracts from.

**Context (verified):** Per-visit net `sqlite_reports_repo.rs:143-147` and grouped net `:281-285` use `collected` basis but `saturating_sub` (floors at 0). Dashboard `service/mod.rs:223-229` and trend `:322-335` use `revenue_iqd` (billed) with `saturating_sub`. Design §3.5 (truthful net) + §3.6 (align to collected).

- [ ] **Step 1: Write failing tests**

Add a reports test asserting a negative net and a collected-basis dashboard net. Locate the reports test module (`grep -rn "mod tests" src-tauri/src/domains/reports`), or add to `src-tauri/tests/` reports integration. Intent:

```rust
// One locked visit: collected 10k, doctor fixed cut 12k, operator 5k.
// Per-visit net must be 10000 - 12000 - 5000 = -7000 (NOT floored to 0).
```

And for the dashboard:

```rust
// Billed 50k but collected (override) 30k, cuts 12k+5k.
// Dashboard net must use collected: 30000 - 12000 - 5000 = 13000, NOT 33000.
```

- [ ] **Step 2: Run to verify failure**

Run the located reports test; expected FAIL (net floored / billed basis).

- [ ] **Step 3: Drop the floor in the repo net (per-visit + grouped)**

In `sqlite_reports_repo.rs`, replace both `net_iqd` computations (lines ~143 and ~281) that chain `.saturating_sub(...)` with plain subtraction:

```rust
            net_iqd: collected - dc - oc - report_amt - mandoub_amt,
```

(per-visit; use the local variable names present) and for the grouped one:

```rust
                    net_iqd: collected - dc - oc - report - mc,
```

`i64` handles the negative. Keep the `collected` definition (`COALESCE(amount_paid_override_iqd, total_amount_iqd_snapshot)`) as-is.

- [ ] **Step 4: Switch the dashboard + trend net to collected and drop the floor**

In `service/mod.rs`, the dashboard net (line ~223):

```rust
        let net = agg.collected_iqd
            - agg.doctor_cut_iqd
            - agg.operator_cut_iqd
            - agg.report_iqd
            - agg.mandoub_cut_iqd
            - inv_value;
```

Apply the same substitution in `trend_matrix` (lines ~322 and ~329, `cur_net` and `prior_net`): use the collected field and plain subtraction. If the trend aggregate does not already expose `collected_iqd`, use the same `aggregate_visits` result which does.

- [ ] **Step 5: Run reports tests**

Run:

```bash
cd src-tauri && cargo test --lib domains::reports 2>&1 | tail -20
```

Expected: pass (negative net + collected dashboard). Also run any reports integration binary: `cargo test --test <reports_binary>`.

- [ ] **Step 6: Update the CSV writer expectation if it hardcodes a net**

`csv_writer.rs:412-413` computes an expected net in a test (`50_000 - 20_000 - 5_000 - 3_000`). If that test's inputs imply a collected basis differing from billed, update the expectation. Run `cargo test --lib domains::reports::domain::services::csv_writer` and fix any assertion drift.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domains/reports/
git commit -m "feat(reports): truthful (negative-capable) net; align dashboard/trend to collected basis"
```

---

## Task 8: Server — schema, validator, version, reports, sync store

**Files:**
- Modify: `sync-server/prisma/schema.prisma`
- Modify: `sync-server/src/app/common/version.ts`
- Modify: `sync-server/src/app/sync/service/validators.ts`
- Modify: `sync-server/src/app/domains/reports/service/reports-service.ts`
- Modify: `sync-server/src/app/sync/infrastructure/prisma/entity-store.ts`
- Modify: `sync-server/src/app/sync/infrastructure/memory/store.ts`
- Test: `sync-server` test suite (`pnpm test`)

**Interfaces:**
- Consumes: the desktop push payload, which now includes `price_override_iqd` on the visit row and doctor cuts already computed off the collected amount.
- Produces: `Visit.priceOverrideIqd` persists; the validator accepts the field; the total invariant (`total = price + dye`) is unchanged (it validates whatever price the desktop sent, so no new check); server net aligns to the collected basis; the conflict-key arrays include `price_override_iqd`.

**Context (verified):** `SERVER_SCHEMA_VERSION = 22` (version.ts:59). Validator total invariant at `validators.ts:117-120`. Server reports net: headline uses collected (`reports-service.ts:93-95`), but per-visit (`:346`) and grouped (`:397`) use billed `total` — same divergence as desktop. Conflict-key arrays at `entity-store.ts:570-586` and `memory/store.ts:521-537` already include `price_snapshot_iqd` + `amount_paid_override_iqd`.

- [ ] **Step 1: Add the Prisma field**

In `schema.prisma`, on the `Visit` model near `priceSnapshotIqd` (L472):

```prisma
  priceOverrideIqd            Int?      @map("price_override_iqd")
```

- [ ] **Step 2: Bump the server schema version in lockstep**

In `version.ts:59`: change `22` to `23`:

```ts
export const SERVER_SCHEMA_VERSION = 23
```

- [ ] **Step 3: Accept `price_override_iqd` in the validator**

In `validators.ts`, `validateVisit`, add a bounds check next to the override check (L131):

```ts
  if (row.price_override_iqd != null && row.price_override_iqd < 0) {
    return err(opId, 'price_override_iqd must be >= 0')
  }
```

The total invariant (L117-120) stays: `total_amount_iqd_snapshot === price_snapshot_iqd + dye_cost_snapshot_iqd`. No change — the desktop already snapshots the editable price into `price_snapshot_iqd`.

- [ ] **Step 4: Add the field to the visit sync-record type + conflict arrays**

Wherever `VisitSyncRecord` (snake_case) is declared, add `price_override_iqd: number | null`. Add `'price_override_iqd'` to the conflict-key arrays in `entity-store.ts:570-586` and `memory/store.ts:521-537` (so a divergent editable price triggers a conflict like the other snapshot fields). Also ensure the Prisma create/update mapping in the visit sync store writes `priceOverrideIqd`.

- [ ] **Step 5: Align server reports net to collected + drop floors**

In `reports-service.ts`:
- Per-visit net (L346): change the basis from billed `total` to collected: `const collected = row.amount_paid_override_iqd ?? row.total_amount_iqd_snapshot ?? 0; net_iqd = collected - dc - oc - reportAmt - mandoubCut`.
- Grouped net (L397): same substitution to the collected sum.
- Confirm the headline net (L93-95) already uses collected — leave as-is.
- Ensure no `Math.max(0, ...)` floor is applied to net anywhere (grep `Math.max(0` in this file); if present around net, remove it so net can be negative.

- [ ] **Step 6: Regenerate Prisma client + typecheck + test**

Run (Context7-check Prisma CLI first per project rules):

```bash
cd sync-server && pnpm prisma generate && pnpm tsc --noEmit 2>&1 | tail -20 && pnpm test 2>&1 | tail -25
```

Expected: client regenerates, typecheck clean, tests pass. Fix any validator/reports test that asserted the old billed-basis net or lacked the new field.

- [ ] **Step 7: Commit**

```bash
git add sync-server/prisma/schema.prisma sync-server/src/app/common/version.ts \
  sync-server/src/app/sync/service/validators.ts \
  sync-server/src/app/domains/reports/service/reports-service.ts \
  sync-server/src/app/sync/infrastructure/
git commit -m "feat(server): visit price_override_iqd, schema v23, collected-basis truthful net"
```

**Freeze-hash note (no code change):** the desktop daily-close freeze `input_hash`
(`src-tauri/src/domains/reports/domain/services/input_hash.rs`) folds in `visit_ids`,
`total_revenue_iqd`, `total_doctor_cuts_iqd`, `total_operator_cuts_iqd`, and the settings
snapshot. The editable price flows into it **transitively** — it changes
`price_snapshot_iqd`, which changes `total_revenue_iqd` and `total_doctor_cuts_iqd`, both
hash inputs. So an edited price already invalidates a re-computed freeze hash; no hash
field needs adding. (Pre-existing property, unchanged here: the hash uses billed revenue,
not collected — out of scope for this feature.)

---

## Task 9: Frontend — editable price field + total math

**Files:**
- Modify: `src/lib/schemas/visit.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/stores/visit-tabs-store.ts`
- Modify: `src/pages/reception/new-visit-tabbed.tsx`
- Modify: `src/features/visits/queries.ts`
- Modify: `src/i18n/locales/en/reception.json`, `src/i18n/locales/ar/reception.json`
- Test: `pnpm lint`, `pnpm build`, `pnpm test` (vitest)

**Interfaces:**
- Consumes: IPC `visits_create_draft` / `visits_update_draft` now accept `price_override_iqd`; the money engine uses it.
- Produces: an editable price `<input type="number">` prefilled from `pricing_effective`, bound to `form.priceOverrideIqd`; the running total uses the editable price; create/update mutations send it.

**Context (verified):** Form state is Zustand `VisitTabForm` in `visit-tabs-store.ts` (not RHF). Price comes from `usePricingEffective` (IPC `pricing_effective`), total at `new-visit-tabbed.tsx:128` `const totalIqd = (priceIqd ?? 0) + (dyeApplied ? dyeCostIqd : 0)`. There is already an `amountPaidOverrideIqd` collected input (`:539-587`). Draft schemas `VisitCreateDraftSchema` (`visit.ts:30`) / `VisitUpdateDraftSchema` (`:59`) have NO price field. i18n keys under `reception.new_visit.*` in `reception.json`.

- [ ] **Step 1: Add `price_override_iqd` to the Zod draft schemas**

In `src/lib/schemas/visit.ts`, add to `VisitCreateDraftSchema` and `VisitUpdateDraftSchema`:

```ts
  price_override_iqd: z.number().int().min(0).nullable().optional(),
```

- [ ] **Step 2: Add the IPC arg + type**

In `src/lib/ipc.ts`, add `price_override_iqd?: number | null` to the `visits_create_draft` and `visits_update_draft` arg types. (The lock args and `VisitSnapshotRecord` already carry `price_iqd`/`amount_paid_override_iqd` — no change.)

- [ ] **Step 3: Add `priceOverrideIqd` to the form store**

In `src/stores/visit-tabs-store.ts`, add `priceOverrideIqd: number | "" | null` to `VisitTabForm` (mirror the `amountPaidOverrideIqd` field shape at `:42`), default `null` (meaning "use catalog price"). Add a setter if the store uses per-field setters.

- [ ] **Step 4: Render the editable price input**

In `new-visit-tabbed.tsx`, near where the price is shown (the subtype select / `priceIqd`), add an editable number input bound to `form.priceOverrideIqd`, prefilled/placeholder from `pricing_effective` (`priceIqd`). Compute the effective price for the total:

```tsx
const effectivePriceIqd =
  form.priceOverrideIqd !== null && form.priceOverrideIqd !== ""
    ? Number(form.priceOverrideIqd)
    : (priceIqd ?? 0);
const totalIqd = effectivePriceIqd + (dyeApplied ? dyeCostIqd : 0);
```

Give the input `data-testid="price-override"` and label it with the i18n key from Step 7.

- [ ] **Step 5: Send the price in create/update mutations**

In `src/features/visits/queries.ts` (and the caller in `new-visit-tabbed.tsx` that autosaves the draft), include in the create/update payload:

```ts
price_override_iqd:
  form.priceOverrideIqd === "" || form.priceOverrideIqd == null
    ? null
    : Number(form.priceOverrideIqd),
```

- [ ] **Step 6: Verify the running total reflects the editable price**

The `RunningTotalPanel` receives `totalIqd` — now derived from `effectivePriceIqd`. No prop change needed; confirm the panel shows the edited number.

- [ ] **Step 7: Add i18n keys (en + ar)**

In `src/i18n/locales/en/reception.json` under `new_visit`:

```json
"price": "Price",
"price_hint": "Edit if this visit's price differs from the default"
```

In `src/i18n/locales/ar/reception.json` under `new_visit` (Arabic, RTL):

```json
"price": "السعر",
"price_hint": "عدّل السعر إذا اختلف عن الافتراضي لهذه الزيارة"
```

- [ ] **Step 8: Lint, build, test, and smoke in the Tauri webview**

Run:

```bash
pnpm lint && pnpm build && pnpm test 2>&1 | tail -25
```

Expected: eslint + i18n parity clean, tsc + vite build passes, vitest green. Then smoke-test in the real webview:

```bash
pnpm tauri dev
```

Create a visit, edit the price, confirm the running total updates; lock with a collected override lower than the price; open the visit detail and confirm the doctor cut scaled to the paid amount. Stop `pnpm tauri dev` before finishing.

- [ ] **Step 9: Commit**

```bash
git add src/lib/schemas/visit.ts src/lib/ipc.ts src/stores/visit-tabs-store.ts \
  src/pages/reception/new-visit-tabbed.tsx src/features/visits/queries.ts \
  src/i18n/locales/en/reception.json src/i18n/locales/ar/reception.json
git commit -m "feat(reception): editable per-visit price field feeding the paid-basis cut math"
```

---

## Task 10: Cross-surface verification + status update

**Files:**
- Modify: `docs/idc-system/status.md` (phase-completion note + cumulative totals)
- No code; this task is the round-trip + reconciliation gate.

**Interfaces:**
- Consumes: everything above.
- Produces: a verified end-to-end round trip and an updated planning tracker.

- [ ] **Step 1: Confirm schema-version lockstep**

Run:

```bash
cd src-tauri && ls migrations/0*.sql | wc -l   # expect 23
grep -rn "SYNC_SCHEMA_VERSION" src/            # resolves to 23
cd ../sync-server && grep SERVER_SCHEMA_VERSION src/app/common/version.ts  # 23
```

Expected: all three read 23.

- [ ] **Step 2: Full desktop validation (per-binary tests, never the whole suite)**

Run:

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10 && \
  cargo test --lib 2>&1 | tail -15
```

Then run each touched integration binary individually (`cargo test --test <name>` for visits/reports/sync binaries located earlier). Expected: clean.

- [ ] **Step 3: Sync round-trip (offline create -> push -> server has the editable price)**

With `pnpm tauri dev` running and a reachable sync server (local compose or the dev server), create a visit with an editable price and a collected override, lock it, let it sync, and confirm the server row:

```bash
# adjust container/DB names to the local dev setup
docker exec <sync-db> psql -U postgres -d idc_sync -tAc \
  "SELECT price_override_iqd, price_snapshot_iqd, doctor_cut_snapshot_iqd, amount_paid_override_iqd FROM visits ORDER BY created_at DESC LIMIT 1;"
```

Expected: `price_override_iqd` matches what was typed; `doctor_cut_snapshot_iqd` reflects the collected-basis cut, not the full-price cut.

- [ ] **Step 4: Reconciliation check (dashboard vs daily-close net)**

In the accounting screens, with at least one underpaid (override) visit in range, confirm the dashboard net and the daily-close net now use the same collected basis and agree (they diverged before this change).

- [ ] **Step 5: Update `status.md`**

Append a phase-completion note to `docs/idc-system/status.md` under "Blockers & Notes": what landed (editable price, paid-basis cuts, zero-guard, truthful net, basis alignment), schema version 22 -> 23, verification results (clippy/tests/lint/build/server tests), and any deferral. Refresh the Cumulative Totals row (local tables unchanged; one migration added; note the money-engine change). Follow the existing status.md format exactly.

- [ ] **Step 6: Commit**

```bash
git add docs/idc-system/status.md
git commit -m "docs(status): record paid-basis cut calculation phase completion"
```

---

## Notes for the implementer

- **Order matters:** Tasks 1-3 are prerequisites for 5-6; Task 2 (engine) is the heart. Task 4 is a small reconciliation that depends on Task 2's zero-guard. Tasks 7-8 (reports) can proceed once the snapshot shape is stable (after Task 6). Task 9 (frontend) depends on the command args from Task 6. Task 10 gates the whole thing.
- **The single most important invariant:** with NO overrides (`price_override_iqd = None`, `amount_paid_override_iqd = None`), `collected == price` and cuts must match the legacy full-price behaviour EXCEPT that dye now reduces the cut base (`cut_base = price − dye`). That is a deliberate design change (dye covered first), so legacy tests asserting cuts off full price WITH dye on must be updated (Task 2 Step 8). Where a legacy test's intent was purely the cut, set `dye: false` to isolate it.
- **Never** run the full `cargo test` (crashes the IDE). Always `cargo test --lib <module>` or `cargo test --test <binary>`.
- **No production history** exists (pre-launch), so no migration backfills or recomputes locked visits — consistent with phase 11.
