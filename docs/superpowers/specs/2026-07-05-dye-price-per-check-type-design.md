# Per-Check-Type Dye Price — Design

**Date:** 2026-07-05
**Status:** approved (design), ready for implementation plan
**Surfaces:** SQLite + Tauri/Rust, Sync Server (Prisma/Fastify), Frontend (React)

## 1. Problem

Dye cost is a single global setting (`settings.dye_cost_iqd`) applied to every
dye-carrying visit. In reality the dye/contrast price varies by procedure, so it
must be configurable **per check type**, and — because a check type's price
itself lives on its subtypes when it has them — **per subtype** when subtypes
exist. The global setting is retired; dye price comes purely from the catalog.

The `check_types.dye_supported` flag (a check-type-level on/off for dye) is also
retired: with a per-check/per-subtype dye price, **price presence is the single
source of truth** — dye is available wherever a `dye_price_iqd` resolves
(non-null), unavailable where it is null.

## 2. Current model (verified)

- `settings.dye_cost_iqd`: one global IQD amount, read at visit-build time
  (`visits/commands.rs:60` -> `MoneySettings.dye_cost_iqd`) and fed into the
  money engine.
- `money_math::compute` (`money_math.rs:70`): guards `dye && !dye_supported`;
  `money_math.rs:114-115`: `dye_cost = if dye { settings.dye_cost_iqd } else { 0 }`.
- Price resolution (`money_math.rs:231-234`, `resolve_price`): subtype
  `price_iqd` when subtyped, else `check_type.base_price_iqd`. **Dye price will
  mirror this exact resolver.**
- `check_types` columns (migration 003): `base_price_iqd INTEGER NULL`,
  `dye_supported INTEGER NOT NULL DEFAULT 0`, `report_supported` (already
  dropped in 018), full sync column set.
- `check_subtypes` columns: `price_iqd INTEGER NOT NULL CHECK (>= 0)`, full sync
  column set.
- `dye_supported` blast radius (all must change): catalog entity/service/repo/
  push-payloads (`check_type.rs`, `check_type_service.rs`, `check_type_repo.rs`,
  `push_payloads.rs`, `consumption_service.rs:116`), money engine
  (`money_math.rs:70`), visit-service validation (`visit_service.rs:384,498`),
  sync puller (`puller_entities.rs:186,195,218,1274`), seed data
  (`migrations.rs:378`, `seed_weekly.rs:755`), reception grid
  (`visit_service.rs:336`, `checks-grid.tsx:86`, `format.ts:68,112`), and the
  server (Prisma model + push validators).

## 3. Target model

### 3.1 Schema

- **`check_types`**: add `dye_price_iqd INTEGER NULL CHECK (dye_price_iqd IS NULL
  OR dye_price_iqd >= 0)`. **Drop** `dye_supported`.
- **`check_subtypes`**: add the same `dye_price_iqd INTEGER NULL CHECK (...)`.
- `0` is a valid configured value (free dye); `NULL` means "dye not offered
  here". This distinction is load-bearing.

### 3.2 Resolution rule

At draft creation and at lock, mirroring the existing price resolver:

```
dye_price =
  if check_type.has_subtypes  -> chosen_subtype.dye_price_iqd   (Option<i64>)
  else                        -> check_type.dye_price_iqd       (Option<i64>)
```

**Dye availability = `dye_price.is_some()`.** The visit `dye` flag may be true
only when a dye price resolves.

### 3.3 Money engine (`money_math`)

- `MoneyMathInputs` gains a pre-resolved `dye_price_iqd: Option<i64>` (caller
  resolves base-vs-subtype before calling, exactly as it already resolves the
  catalog price). `MoneySettings.dye_cost_iqd` is **removed**.
- Dye-cost line:
  - `dye` off -> `dye_cost = 0`.
  - `dye` on -> `dye_cost = dye_price_iqd.ok_or(Validation("dye not available for
    this check"))?` (hard error, never a silent 0).
- The `dye && !dye_supported` guard (line 70) is **replaced** by the
  price-presence requirement above.
- Everything downstream is unchanged: `cut_base = max(0, collected - dye_cost)`,
  `total = price + dye_cost`, the `cut_base == 0` zero-guard, and the snapshot
  shape (`dye_cost_snapshot_iqd`).

### 3.4 Visit service

- New helper `resolve_dye_price(check_type, subtype) -> Option<i64>` mirroring
  the price resolver; threaded into `compute()` at lock like the editable price
  already is.
- `create_draft` / `update_draft`: the two `input.dye && !ct.dye_supported`
  checks (`visit_service.rs:384,498`) become `input.dye &&
  resolve_dye_price(ct, subtype).is_none()` -> reject "dye not available for this
  check/subtype". This is where per-subtype opt-out (null price) is enforced.

### 3.5 Migration `025`

- Add `dye_price_iqd` (nullable, `>= 0` CHECK) to `check_types` and
  `check_subtypes`.
- Drop `dye_supported` from `check_types` (SQLite table rebuild — the locked
  visits CHECK does not reference it; the catalog CHECK for `has_subtypes` vs
  `base_price_iqd` is preserved).
- **No backfill**: every `dye_price_iqd` starts NULL, so dye is OFF clinic-wide
  until the accountant configures each price. Acceptable pre-launch (no dye
  history to preserve).
- **Tombstone the obsolete global `dye_cost_iqd` settings row** (soft-delete,
  bump version, `dirty = 1`) so it stops appearing in the settings UI/cache and
  the tombstone syncs (LWW). Idempotent / guarded like migration 018's
  `report_cost_iqd` tombstone.

### 3.6 Schema-version lockstep

`check_types`/`check_subtypes` sync, so this bumps the schema version:
`SYNC_SCHEMA_VERSION` (= migration count -> 25) and `SERVER_SCHEMA_VERSION` ->
25 in the SAME commit. Prisma model gains `dyePriceIqd Int?`, drops
`dyeSupported`; server push validators accept `dye_price_iqd` (nullable, `>= 0`)
and stop requiring `dye_supported`.

## 4. Sync

- No new outbox entity; `dye_price_iqd` rides inside the existing versioned
  `check_types`/`check_subtypes` snapshots. Conflict policy unchanged (catalog
  LWW).
- Desktop push/pull mappers (`push_payloads.rs`, `puller_entities.rs`) and the
  Prisma store add `dyePriceIqd`, drop `dyeSupported`. Server push validators
  mirror.

## 5. Frontend

- **Catalog admin form**: add a "Dye price" input next to the existing base-price
  / subtype-price inputs, matching that pattern. It appears on the check-type
  form (no-subtype types) and on each subtype row (subtyped types), following
  where the base price lives. Empty/null = "dye not offered here". The
  `dye_supported` toggle is **removed**.
- **Reception** (`checks-grid.tsx`, new-visit form): the dye toggle's
  availability keys off "a dye price resolves for the selected check/subtype".
  `ChecksGridCard.dye_supported` becomes a derived `dye_available` (does this
  check type have any resolvable dye price). Running-total TS (`format.ts`)
  mirrors the engine: dye adds the resolved dye price; its `dye_supported` gate
  becomes the price-presence gate.
- **i18n**: en + ar for the dye-price field and "dye not available for this
  subtype" messaging, RTL verified.
- No new routes, no new IPC commands (existing catalog create/update args gain
  `dye_price_iqd`; visit args unchanged).

## 6. Edge cases

1. **Dye on, no resolvable price** -> hard `Validation` error at draft creation,
   at update, and at `compute()` (defense in depth). Never a silent zero.
2. **Subtype opt-out**: a check type may have some subtypes priced and others
   null -> dye allowed only on priced ones. UI disables the dye toggle for a
   null-priced subtype; the service rejects it if forced.
3. **`dye_price_iqd = 0`** is valid (free dye), distinct from NULL. Resolution
   treats `Some(0)` as available at 0 cost, `None` as unavailable. Tests pin it.
4. **has_subtypes toggle re-homing**: editing a check type between subtyped and
   non-subtyped re-homes the base price today; dye price follows the same
   re-homing so it never orphans.
5. **Locked visits immutable**: changing a catalog dye price after lock never
   touches `dye_cost_snapshot_iqd`. Test: lock -> bump catalog dye price ->
   re-read -> snapshot unchanged.
6. **Retired global `dye_cost_iqd`**: tombstoned; the `required_i64(state,
   "dye_cost_iqd")` read is removed. Test confirms no path reads it.
7. **Inventory consumption dye-rule guard** (`consumption_service.rs:116`): the
   `on_dye_only && !ct.dye_supported` check becomes "the check type must have dye
   available somewhere" — `ct.dye_price_iqd.is_some()` (no-subtype) OR at least
   one live subtype of `ct` has `dye_price_iqd.is_some()`. Rejects a dye-only
   consumption rule tied to a check type where dye is nowhere offered. Needs a
   repo helper (`check_type_has_any_dye_price(check_type_id)`), since the
   subtyped case requires looking across the check type's subtypes.

## 7. Testing (per-binary; full `cargo test` crashes the dev IDE)

- **money_math unit**: dye from resolved price (subtype + check-type paths);
  `Some(0)` vs `None`; dye-on-without-price errors; `cut_base`/`total` correct
  with the new dye source; regenerate the dye matrix.
- **visit entity/service**: draft + lock reject dye-without-price; resolver picks
  subtype dye price when subtyped, check-type otherwise.
- **catalog**: CRUD round-trips `dye_price_iqd`; re-homing on subtype toggle.
- **consumption**: a dye-only consumption rule is rejected for a check type with
  dye nowhere offered (no check-type price and no priced subtype), accepted when
  the check type or any subtype has a dye price.
- **migration 025**: columns added nullable; `dye_supported` dropped; existing
  rows -> null dye price; global `dye_cost_iqd` tombstoned; all migrations apply.
- **sync**: catalog round-trip carries `dye_price_iqd`, drops `dye_supported`,
  converges.
- **schema lockstep**: `SYNC_SCHEMA_VERSION == SERVER_SCHEMA_VERSION == 25`.
- **frontend (vitest)**: catalog form sets/clears dye price; reception dye toggle
  gated on `dye_available`; running-total uses resolved dye price.

## 8. Non-goals

- No backfill of existing rows (dye off until configured).
- No per-visit dye override — dye price comes purely from the catalog (unlike the
  per-visit editable *base* price).
- No historical recompute of locked visits or signed closes.
- No keeping `dye_supported` as a redundant flag — it is dropped outright.
