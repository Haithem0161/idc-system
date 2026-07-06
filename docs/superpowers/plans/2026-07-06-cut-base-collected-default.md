# Cut-base Collected Default Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the money engine so cuts run on the full collected service revenue: `collected` defaults to `price + dye` (not `price`), so `cut_base = collected − dye = price` and a 100k/60k dye visit yields doctor 25k / report 18,750 instead of 10k / 7,500.

**Architecture:** A one-line change to the default value of `collected` in the pure Rust `money_math::compute()`. Dye is profit secured off the top; `cut_base = max(0, collected − dye)` is unchanged, but with `collected` now defaulting to the full patient total, subtracting dye leaves the full service price as the base. The dead frontend TS port (`format.ts`) is brought into parity so it stops lying in the other direction. No schema, no server, no migration.

**Tech Stack:** Rust (sqlx-free pure domain logic), Vitest/TypeScript.

## Global Constraints

- **Cargo per-target ONLY.** Never `cargo test`, `cargo build`, `cargo check`, or `--all-targets` (crate-wide cargo crashes the user's machine). Use `cargo test --lib domains::visits::domain::services::money_math`, `cargo clippy --lib -- -D warnings`, `cargo fmt`. Run all cargo commands from `src-tauri/`.
- **No Claude authorship in commits.** Commit with `git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" commit -m "..."`. No `Co-Authored-By`, no Anthropic email.
- **No emojis** anywhere (code, comments, commit messages).
- **No schema/server/migration change.** Do not touch `src-tauri/migrations/`, `sync-server/`, or `SERVER_SCHEMA_VERSION`.
- **No retro-recompute of locked visits.** Only `money_math::compute()` logic changes; nothing rewrites stored snapshots.
- Reference spec: `docs/superpowers/specs/2026-07-06-cut-base-collected-default-design.md`.

---

## File Structure

- `src-tauri/src/domains/visits/domain/services/money_math.rs` — the one-line default change (line 121), refreshed doc comments, and two updated tests in its `#[cfg(test)] mod tests`.
- `src/features/visits/format.ts` — the dead TS port `computeRunningTotal`, brought into parity with the Rust rule.
- `src/features/visits/format.test.ts` — added dye-bearing cut tests locking the new TS behavior.

---

### Task 1: Fix the cut-base default in the Rust engine

**Files:**
- Modify: `src-tauri/src/domains/visits/domain/services/money_math.rs:121` (the default), plus doc comments at lines 60, 120, 127, 257–262.
- Test: same file's `#[cfg(test)] mod tests` — update `flat_house_with_dye` and `report_base_uses_cut_base_which_dye_reduces`, add one new regression test.

**Interfaces:**
- Consumes: nothing new. `compute(inputs: &MoneyMathInputs<'_>) -> AppResult<VisitSnapshots>` signature unchanged.
- Produces: the values of `doctor_cut_iqd`, `report_amount_iqd` for dye visits with no `amount_paid_override_iqd` now compute off `cut_base = price` (was `price − dye`). No signature or shape change.

- [ ] **Step 1: Update the failing test to the new expectation first (TDD — assert the target, watch it fail)**

In `money_math.rs`, replace the body of `flat_house_with_dye` (the two lines at 521–523) so it asserts the corrected value:

```rust
        // Dye is profit off the top: collected defaults to price + dye
        // (50000 + 2000 = 52000), so cut_base = 52000 - 2000 = 50000 and
        // doctor_cut = 50000 * 40 / 100 = 20000 (the full off-service value).
        assert_eq!(snap.doctor_cut_iqd, 20_000);
```

- [ ] **Step 2: Rewrite `report_base_uses_cut_base_which_dye_reduces` — its premise is now inverted**

The old test asserted with-dye and without-dye report amounts DIVERGE (5760 vs 6000). Under the fix they CONVERGE (both 6000), because dye no longer shrinks the base. Rename the function and flip the final assertions. Replace the whole `fn report_base_uses_cut_base_which_dye_reduces() { ... }` block's comment header and its three closing assertions:

Change the doc comment (lines 648–655) to:

```rust
    fn report_base_is_unaffected_by_dye_since_dye_is_off_the_top() {
        // Dye is profit off the top: collected defaults to price + dye, so
        // subtracting dye leaves the full service price as the cut base. The
        // report base is therefore identical with or without dye.
        //   with dye:    collected = 52000; cut_base = 52000 - 2000 = 50000;
        //                doctor_cut = 20000; report = 20% * (50000-20000) = 6000.
        //   without dye: collected = 50000; cut_base = 50000; doctor_cut = 20000;
        //                report = 20% * (50000-20000) = 6000.
        // The two now MATCH (dye stopped reducing the base).
```

Change the three closing assertions (lines 694–696) to:

```rust
        assert_eq!(with_dye.report_amount_iqd, 6_000);
        assert_eq!(without_dye.report_amount_iqd, 6_000);
        assert_eq!(with_dye.report_amount_iqd, without_dye.report_amount_iqd);
```

- [ ] **Step 3: Add a new regression test for the reported bug (the exact production case)**

Add this test immediately after `report_base_is_unaffected_by_dye_since_dye_is_off_the_top`. It locks the exact 100k/60k/25%/25% case from the bug report. Note `settings()` has `report_pct: 20, internal_doctor_pct: 40`; this test uses an EXTERNAL doctor with a 25% per-check pricing row and overrides `report_pct` via a custom settings, OR asserts against the house path. Use the external-doctor path with an explicit 25% pricing row and the default `report_pct: 20` from `settings()` — assert the doctor cut (the load-bearing number) and derive report from 20%:

```rust
    #[test]
    fn dye_visit_cuts_run_on_full_service_price_not_price_minus_dye() {
        // Reproduces the reported bug: price 100k, dye 60k, external doctor 25%.
        // Before the fix: collected defaulted to price (100k), cut_base =
        // 100000 - 60000 = 40000, doctor_cut = 40000 * 25% = 10000 (WRONG).
        // After: collected defaults to price + dye = 160000, cut_base =
        // 160000 - 60000 = 100000, doctor_cut = 100000 * 25% = 25000 (RIGHT).
        let ct = ct_dye(false, Some(100_000), Some(60_000));
        let op = operator();
        let doc = doctor();
        let pr = pricing(doc.id, ct.id, CutKind::Pct, 25, None);
        let snap = compute(&MoneyMathInputs {
            check_type: &ct,
            check_subtype: None,
            doctor: Some(&doc),
            doctor_pricing: Some(&pr),
            operator: &op,
            patient_name: "p",
            dye: true,
            report: true,
            dalal: false,
            discount: false,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            price_override_iqd: None,
            amount_paid_override_iqd: None,
            settings: settings(),
        })
        .unwrap();
        assert_eq!(snap.dye_cost_iqd, 60_000);
        assert_eq!(snap.total_amount_iqd, 160_000);
        // cut_base = 100000, doctor 25% = 25000.
        assert_eq!(snap.doctor_cut_iqd, 25_000);
        // report_pct from settings() is 20%: 20% * (100000 - 25000) = 15000.
        assert_eq!(snap.report_amount_iqd, 15_000);
    }
```

Note: `settings()` uses `report_pct: 20`, so the report here is 15,000 (20% of 75k), not 18,750 (which was the 25% illustration in the spec). The spec's 18,750 assumed report_pct 25%; this test uses the fixture's 20%. Both are correct for their respective report_pct. Do NOT change `settings()`.

- [ ] **Step 4: Run the tests to verify they FAIL against the current code**

Run from `src-tauri/`:
```bash
cargo test --lib domains::visits::domain::services::money_math 2>&1 | tail -30
```
Expected: `flat_house_with_dye`, `report_base_is_unaffected_by_dye_since_dye_is_off_the_top`, and `dye_visit_cuts_run_on_full_service_price_not_price_minus_dye` FAIL (they assert post-fix values against pre-fix code). Other money_math tests PASS.

- [ ] **Step 5: Apply the one-line fix**

In `money_math.rs`, change line 121 from:
```rust
    let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd);
```
to:
```rust
    let collected = inputs.amount_paid_override_iqd.unwrap_or(price_iqd + dye_cost);
```

- [ ] **Step 6: Update the surrounding doc comments so they describe the correct model**

Line 120, change:
```rust
    // Collected cash defaults to the (editable) price when no override is set.
```
to:
```rust
    // Collected cash defaults to the full patient total (price + dye) when no
    // override is set. Dye is profit secured off the top, so subtracting it
    // below leaves the full service price as the cut base.
```

Line 127, change:
```rust
    // Cut base: collected minus dye (dye is a material cost, covered first).
```
to:
```rust
    // Cut base: collected minus dye. Dye is profit secured off the top; what
    // remains is the collected service revenue every cut is measured against.
```

Line 60 (the `MoneyMathInputs` field doc that says "off `max(0, collected - dye)`") — leave the formula but if it calls dye a "material cost", reword to "profit off the top". Read lines 57–62 and adjust only if the wording contradicts the new model.

Lines 257–262 (the `cut_base` fn doc), change:
```rust
/// The base every cut is measured against: collected cash net of dye, floored
/// at zero. When it is zero, no cut (fixed or scaled) is paid. `price_iqd` is
/// accepted for signature symmetry with the design spec but is not needed for
/// the computation (the base is purely collected - dye).
```
to:
```rust
/// The base every cut is measured against: collected cash net of the dye that
/// is secured off the top, floored at zero. When it is zero, no cut (fixed or
/// scaled) is paid. `price_iqd` is accepted for signature symmetry with the
/// design spec but is not needed for the computation (the base is purely
/// collected - dye).
```

- [ ] **Step 7: Run the money_math tests to verify they now PASS**

Run from `src-tauri/`:
```bash
cargo test --lib domains::visits::domain::services::money_math 2>&1 | tail -30
```
Expected: all money_math tests PASS, including the three updated/added ones. If any OTHER dye test fails, it asserted a cut off `price − dye` that this plan missed — read its body, recompute against `cut_base = price` (for no-override cases), and update its expectation + comment. (Per the pre-plan audit only `flat_house_with_dye` and the renamed report test assert cuts under dye-with-no-override; all others assert `dye_cost_iqd`/`total_amount`/error paths, which are unchanged.)

- [ ] **Step 8: Clippy + fmt (per-target)**

Run from `src-tauri/`:
```bash
cargo clippy --lib -- -D warnings 2>&1 | tail -20
cargo fmt
```
Expected: no warnings; fmt makes no or trivial changes.

- [ ] **Step 9: Commit**

```bash
cd /home/haithem/Projects/idc-system
git add src-tauri/src/domains/visits/domain/services/money_math.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" commit -m "fix(money): cut base uses full collected total; dye is profit off the top

Default collected to price+dye so cut_base=collected-dye equals the full
service price. A 100k service + 60k dye visit now cuts off 100k (doctor 25%
= 25000), not off 40k (was 10000). Dye is secured off the top and kept whole
in clinic net. No schema/server change; locked visits keep their snapshots."
```

---

### Task 2: Bring the frontend TS port into parity

**Files:**
- Modify: `src/features/visits/format.ts` — `computeRunningTotal` and its `MoneyMathInputs` interface.
- Test: `src/features/visits/format.test.ts` — add dye-bearing cut tests.

**Interfaces:**
- Consumes: nothing from Task 1 at the code level (independent surface). Mirrors the same rule.
- Produces: `computeRunningTotal(inputs: MoneyMathInputs): MoneyMathSnapshot` — cuts now computed off `cutBase = max(0, collected − dyeCost)` where `collected` defaults to `price + dyeCost`. `MoneyMathInputs` gains an optional `amount_paid_override_iqd?: number | null`.

Current bug (opposite direction): `format.ts:138,149,159` compute cuts off `price` and never subtract dye, so a dye visit shows cuts that are too HIGH relative to the (correct) Rust engine. This task makes the port subtract dye from the base like Rust.

- [ ] **Step 1: Add the failing tests first**

In `format.test.ts`, inside `describe("computeRunningTotal", ...)`, add:

```typescript
  it("house dye visit: cuts run on price (collected price+dye, dye subtracted)", () => {
    // collected defaults to price + dye = 50_000 + 2_000 = 52_000.
    // cutBase = 52_000 - 2_000 = 50_000. Doctor 40% = 20_000.
    const snap = computeRunningTotal({ ...baseInputs, dye: true })
    expect(snap.dye_cost_iqd).toBe(2_000)
    expect(snap.doctor_cut_iqd).toBe(20_000)
    expect(snap.patient_total_iqd).toBe(52_000)
  })

  it("underpayment: dye taken first, cuts scale off the remaining service", () => {
    // price 50_000, dye 2_000, patient pays 30_000 total.
    // cutBase = max(0, 30_000 - 2_000) = 28_000. Doctor 40% = 11_200.
    const snap = computeRunningTotal({
      ...baseInputs,
      dye: true,
      amount_paid_override_iqd: 30_000,
    })
    expect(snap.doctor_cut_iqd).toBe(11_200)
  })

  it("underpayment below the dye price zeroes every cut", () => {
    // pays 1_500 < dye 2_000 -> cutBase = 0 -> all cuts zero.
    const snap = computeRunningTotal({
      ...baseInputs,
      dye: true,
      amount_paid_override_iqd: 1_500,
    })
    expect(snap.doctor_cut_iqd).toBe(0)
    expect(snap.operator_cut_iqd).toBe(0)
    expect(snap.report_amount_iqd).toBe(0)
  })
```

- [ ] **Step 2: Run the tests to verify they FAIL**

```bash
cd /home/haithem/Projects/idc-system
pnpm vitest run src/features/visits/format.test.ts 2>&1 | tail -30
```
Expected: the three new tests FAIL (current port ignores dye in the base and has no `amount_paid_override_iqd` field or zero-guard).

- [ ] **Step 3: Rewrite `computeRunningTotal` to mirror the Rust rule**

In `format.ts`, add the optional field to `MoneyMathInputs` (after `dalal: boolean` at line 81, inside the interface):

```typescript
  /**
   * Cash actually collected. Defaults to the full patient total (price + dye)
   * when omitted. Dye is secured off the top; cuts run on collected − dye.
   */
  amount_paid_override_iqd?: number | null
```

Then replace the body from the `const dyeCost` line (125) down to the `patientTotal` line (161) with a version that computes a cut base and a zero-guard. Replace lines 125–161:

```typescript
  const dyeCost = inputs.dye ? (inputs.dye_price_iqd as number) : 0
  // Dye is profit off the top: collected defaults to the full patient total,
  // and the cut base is what remains after the dye is secured.
  const collected =
    inputs.amount_paid_override_iqd != null
      ? inputs.amount_paid_override_iqd
      : price + dyeCost
  const cutBase = Math.max(0, collected - dyeCost)
  let doctorCut: number
  let internalPct: number | null
  if (cutBase === 0) {
    // Zero-guard: a collection that does not cover the dye leaves nothing to
    // share; every cut (fixed and scaled alike) drops to zero.
    doctorCut = 0
    internalPct =
      !inputs.dalal && inputs.doctor_pricing == null
        ? inputs.internal_doctor_pct
        : null
    const patientTotal = price + dyeCost
    return {
      price_iqd: price,
      dye_cost_iqd: dyeCost,
      doctor_cut_iqd: 0,
      operator_cut_iqd: 0,
      internal_pct: internalPct,
      report_amount_iqd: 0,
      patient_total_iqd: patientTotal,
    }
  }
  if (inputs.dalal) {
    // Doctor-substitute mode: flat cut, no house percentage.
    doctorCut = DALAL_DOCTOR_CUT_IQD
    internalPct = null
  } else if (inputs.doctor_pricing == null) {
    if (inputs.internal_doctor_pct < 0 || inputs.internal_doctor_pct > 100) {
      throw new Error(
        "computeRunningTotal: internal_doctor_pct must be in 0..=100"
      )
    }
    doctorCut = Math.floor((cutBase * inputs.internal_doctor_pct) / 100)
    internalPct = inputs.internal_doctor_pct
  } else if (inputs.doctor_pricing.cut_kind === "pct") {
    if (
      inputs.doctor_pricing.cut_value < 0 ||
      inputs.doctor_pricing.cut_value > 100
    ) {
      throw new Error(
        "computeRunningTotal: doctor cut percentage must be in 0..=100"
      )
    }
    doctorCut = Math.floor((cutBase * inputs.doctor_pricing.cut_value) / 100)
    internalPct = null
  } else {
    doctorCut = Math.max(0, inputs.doctor_pricing.cut_value)
    internalPct = null
  }
  if (inputs.report_pct < 0 || inputs.report_pct > 100) {
    throw new Error("computeRunningTotal: report_pct must be in 0..=100")
  }
  const reportAmount = inputs.report
    ? Math.floor(((cutBase - doctorCut) * inputs.report_pct) / 100)
    : 0
  const patientTotal = price + dyeCost
```

The `return { ... }` block at the end (lines 162–170) stays as-is (it references `price`, `dyeCost`, `doctorCut`, `internalPct`, `reportAmount`, `patientTotal`, all still in scope).

Also update the JSDoc at line 71–72 that says "post-doctor-cut price" — change "price" to "cut base" so it reads "Percentage of the post-doctor-cut cut base paid to the internal reporting doctor."

- [ ] **Step 4: Run the tests to verify they PASS**

```bash
pnpm vitest run src/features/visits/format.test.ts 2>&1 | tail -30
```
Expected: ALL tests PASS. The existing no-dye tests (flat house, dalal, subtype, overrides) are unchanged because with `dye: false`, `dyeCost = 0`, `collected = price`, `cutBase = price` — identical to the old `price`-based math. The report test at line 76 (`dye: false`) still yields 6_000.

- [ ] **Step 5: Lint + build**

```bash
pnpm lint 2>&1 | tail -15
pnpm build 2>&1 | tail -15
```
Expected: no ESLint errors, TS build passes.

- [ ] **Step 6: Commit**

```bash
git add src/features/visits/format.ts src/features/visits/format.test.ts
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" commit -m "fix(visits): align TS money port to engine cut base (dye off the top)

computeRunningTotal now computes cuts off cutBase = max(0, collected - dye)
with collected defaulting to price + dye, matching money_math.rs. Adds the
zero-guard and an optional amount_paid_override_iqd input. No-dye behavior
is unchanged; dye visits now match the authoritative Rust snapshot."
```

---

## Verification (whole change)

1. `cd src-tauri && cargo test --lib domains::visits::domain::services::money_math` — all green.
2. `cd src-tauri && cargo clippy --lib -- -D warnings && cargo fmt --check` — clean.
3. `pnpm vitest run src/features/visits/format.test.ts` — all green.
4. `pnpm lint && pnpm build` — clean.
5. Manual (deferred to user, needs UI): `pnpm tauri dev` → create a dye visit (service 100k + dye 60k, external doctor 25%, report on) → confirm doctor cut 25,000 and clinic net = 160,000 − cuts (retains the full 60k dye). Note: the exact report figure depends on the configured `report_pct` (25% → 18,750; 20% → 15,000).

## Out of scope (do not touch)

- `src-tauri/migrations/`, `sync-server/`, `SERVER_SCHEMA_VERSION` — no schema/server change.
- Any retro-recompute of already-locked visit snapshots.
- `visit-detail.tsx` net calc — already treats collected as `total_amount` (price + dye); consistent with the fix, no change.
