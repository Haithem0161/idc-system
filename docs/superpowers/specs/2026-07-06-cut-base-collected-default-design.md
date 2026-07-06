# Design: Cut-base collected default — dye is profit off the top

**Date:** 2026-07-06
**Status:** Approved
**Surfaces:** Tauri/Rust (authoritative), Frontend (dead TS port only). No server, no schema, no migration.

## 1. Problem

The money engine computes every cut off a `cut_base`. Today (`money_math.rs:121`):

```rust
let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd);
let base = cut_base(price_iqd, collected, dye_cost);   // (collected - dye).max(0)
```

When no payment override is set, `collected` defaults to the **service price alone**
(e.g. 100k), then dye (60k) is subtracted, giving `cut_base = 40k`. Every scaled cut
is therefore computed off 40k instead of the intended 100k service revenue.

Observed on a real visit: price 100k, dye 60k, doctor pct 25%, report_pct 25% →
doctor cut **10,000** and report **7,500**, when the clinic expects **25,000** and
**18,750**.

## 2. Root cause

Dye was modeled as a **cost the clinic covers first**, carved out of a base that
started from the service price. The correct business model is the opposite:

> **Dye is profit kept off the top, not a clinic cost.** The clinic secures the full
> dye amount first; cuts are then paid out of the collected *service* money; the
> clinic keeps whatever service money remains.

The formula `cut_base = max(0, collected − dye)` is actually correct under this model
— but only if `collected` is the **full patient total** (`price + dye`), not the
service price alone. The single defect is the default: `collected` must default to
`price + dye`, so that subtracting dye leaves the full service price as the base.

## 3. The fix

One line in `money_math.rs`:

```rust
// before
let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd);
// after
let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd + dye_cost);
```

`cut_base = max(0, collected − dye_cost)` is unchanged. The zero-guard, discount
handling, fixed/scaled dispatch, and negative-net behavior are all unchanged.

## 4. Rules (canonical)

```
collected = amount_paid_override_iqd ?? (price + dye)      // full patient total by default
cut_base  = max(0, collected − dye)                        // dye secured first, off the top
```

Per-role (all unchanged except that they now see the correct, larger cut_base):

| Role | Rule |
|-|-|
| External/referring doctor, pct | `cut_base × pct / 100` |
| External/referring doctor, fixed | `fixed_amount` (does not scale) |
| Internal/house doctor | `cut_base × internal_pct / 100` |
| Reporting doctor | `report_pct × (cut_base − doctor_cut) / 100` |
| Operator | `base_cut_per_check_iqd` (fixed) |
| Representative (مندوب) | 500 / 1000 (fixed) |
| Dalal (دلال) | 10,000 (fixed) |

Zero-guard: `cut_base == 0` → every cut forced to 0 (fixed and scaled alike).
Discount: forces referring-doctor cut to 0; report base widens to `cut_base − 0`.

## 5. Worked examples

Fixed facts for the primary case: price 100k, dye 60k, doctor pct 25%, operator 5k,
report_pct 25%, mandoub 1k. `net = collected − doctor − operator − report − mandoub`.

| Case | Collected | cut_base | Doctor | Report | Op | Rep | Net |
|-|-|-|-|-|-|-|-|
| Full payment (default) | 160,000 | 100,000 | 25,000 | 18,750 | 5,000 | 1,000 | 110,250 |
| Underpay to 120k | 120,000 | 60,000 | 15,000 | 11,250 | 5,000 | 1,000 | 87,750 |
| Underpay to 60k | 60,000 | 0 | 0 | 0 | 0 | 0 | 60,000 |
| Underpay to 40k (< dye) | 40,000 | 0 | 0 | 0 | 0 | 0 | 40,000 |

Net always retains the full dye when collected ≥ dye: 110,250 = 60,000 dye +
50,250 service leftover. When collected < dye, the clinic keeps only what was paid
and no cuts are owed (zero-guard).

No-dye visits are unaffected: `collected = price + 0 = price`, identical to before.

## 6. Blast radius

1. **`money_math.rs:121`** — the one-line default change. Refresh the doc comments at
   lines 60, 120, 127, 257–262 that describe dye as a "material cost covered first";
   reword to "dye is profit secured off the top; cut_base is collected service money".
2. **`money_math.rs` `#[cfg(test)] mod tests`** — every test that sets `dye: true`
   without an explicit `amount_paid_override_iqd` now yields a larger `cut_base`
   (previously `price − dye`, now `price`). Recompute each such expectation and update
   the inline comment. Tests without dye, or with an explicit paid override, are
   unaffected. Add one regression test locking the primary case (100k/60k → doctor
   25k, report 18,750).
3. **`src/features/visits/format.ts`** — the `computeRunningTotal` TS port. Currently
   dead (imported only by its own test), and wrong in the *other* direction (computes
   cuts off `price`, ignores dye). Bring it into parity with the Rust rule: default
   collected to `price + dye`, subtract dye for the cut base, keep the zero-guard.
   Update `format.test.ts` accordingly.

## 7. Explicitly out of scope

- **No schema change, no migration, no `SERVER_SCHEMA_VERSION` bump.** Pure domain logic.
- **No server change.** The sync server stores snapshots; it never recomputes cuts.
  The `total_amount = price + dye` validator invariant is untouched.
- **No frontend net-calc change.** `visit-detail.tsx:194` already treats `collected`
  as `total_amount` (price + dye), so net is already consistent with the fix.
- **No retro-recompute of locked visits.** Already-locked visits keep their stored
  (lower) snapshots; financial records are immutable once locked. Only new visits get
  the corrected math. Recomputing history, if ever wanted, is a separate migration.

## 8. Verification

1. `cd src-tauri && cargo test --lib domains::visits::domain::services::money_math` —
   all money-math tests green with recomputed expectations. (Per-target only; never
   crate-wide `cargo test`.)
2. `cd src-tauri && cargo clippy --lib -- -D warnings && cargo fmt --check`.
3. `pnpm vitest run src/features/visits/format.test.ts` — TS port parity.
4. `pnpm lint && pnpm build`.
5. Manual (deferred to user): create a dye visit (100k + 60k), confirm doctor cut 25k,
   report 18,750, clinic net 110,250 on the visit-detail card.
