# Per-Check-Type Dye Price Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single global `settings.dye_cost_iqd` and the `check_types.dye_supported` flag with a nullable per-check-type AND per-subtype `dye_price_iqd`, where dye availability = a dye price resolves (subtype's value when the check type has subtypes, else the check type's value).

**Architecture:** Dye price resolves from the catalog exactly like the base price already does (`money_math::base_price`): the chosen subtype's `dye_price_iqd` when a subtype exists, else the check type's `dye_price_iqd`. `NULL` = dye not offered; `Some(0)` = free dye. The money engine snapshots the resolved dye amount into the existing `dye_cost_snapshot_iqd` at lock, so locked visits/receipts/reports/freeze-hash are unchanged in shape. The change is wide but mechanical: it flips `dye_supported: bool` -> `dye_price_iqd: Option<i64>` across the catalog entity/repo/push/sync surfaces, adds `dye_price_iqd` to subtypes, and drops the global setting.

**Tech Stack:** Rust (sqlx, tokio, chrono, uuid, serde), SQLite; Fastify + Prisma + Postgres sync server (TypeScript); React 19 + TypeScript frontend.

## Global Constraints

- **No Claude authorship in commits.** No `Co-Authored-By: Claude`, no Anthropic emails, no `git config` changes. Every commit uses `git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" commit ...`.
- **NEVER run crate-wide `cargo test`, `cargo check --all-targets`, `cargo clippy --all-targets`, or `cargo build`** — they crash the user's IDE. Use per-target only: `cargo check --lib`, `cargo clippy --lib -- -D warnings`, `cargo test --lib <module::path>`, `cargo test --test <binary> <name>`. Run all Rust commands from `src-tauri/`.
- **No heavy multi-agent parallel fan-out** during execution — one subagent at a time, sequential, never concurrent.
- **Context7 first (MANDATORY)** before writing code against any library API (sqlx, Prisma, React Hook Form, Zod).
- **No emojis** anywhere.
- **Schema-version lockstep:** desktop `SYNC_SCHEMA_VERSION` (= count of `src-tauri/migrations/*.sql`) MUST equal server `SERVER_SCHEMA_VERSION` (`sync-server/src/app/common/version.ts`). Adding migration `025` makes both **25**; bump the server constant in the SAME commit as the migration.
- **Offline-first invariants:** every syncable mutation bumps `version`, sets `dirty = 1`, tombstones (never hard-deletes). `check_types`/`check_subtypes` conflict policy stays LWW.
- **Package management:** `cargo add` / `pnpm add` only. This plan adds no new dependencies.
- **Money semantics (verbatim from spec):** dye availability = resolved `dye_price_iqd.is_some()`. `NULL` = dye not offered; `Some(0)` = free dye (valid). Dye-on with no resolvable price is a hard `Validation` error at draft, update, AND lock. `cut_base = max(0, collected − dye_cost)`, `total = price + dye_cost`, snapshot into `dye_cost_snapshot_iqd` — all unchanged in shape.

## Context the implementer needs (read before starting)

**Resolution pattern to mirror** (`money_math.rs:230-237`, `base_price`):
```rust
fn base_price(inputs: &MoneyMathInputs<'_>) -> AppResult<i64> {
    if let Some(sub) = inputs.check_subtype { return Ok(sub.price_iqd); }
    inputs.check_type.base_price_iqd.ok_or_else(|| AppError::Validation("...".into()))
}
```
Dye price mirrors this but returns a plain `Option<i64>` (NULL is a legal "no dye" answer, not an error):
```rust
fn dye_price(inputs: &MoneyMathInputs<'_>) -> Option<i64> {
    match inputs.check_subtype {
        Some(sub) => sub.dye_price_iqd,
        None => inputs.check_type.dye_price_iqd,
    }
}
```

**Nullable-patch idiom** (already used by `base_price_iqd`): entity update structs use `Option<Option<i64>>` — outer `Some` = "field present in the patch", inner `None` = "set to NULL", inner `Some(n)` = "set to n". `CheckSubtype.price_iqd` is non-nullable so its patch is `Option<i64>`; `dye_price_iqd` IS nullable so its patch is `Option<Option<i64>>` on BOTH entities.

**Bind-order is the critical invariant** in every sqlx upsert and the sync puller: the column list, the `VALUES (?,?,…)` placeholder tuple, and the `.bind()` chain must agree ordinally. For `check_types` this is a 1:1 replacement of `dye_supported` (position 6, right after `base_price_iqd`) so the count stays 16. For `check_subtypes` it is a NET-NEW column (place right after `price_iqd`) so the placeholder count grows by one and a new `.bind()` inserts at the matching position.

**Type change:** `dye_supported` was stored/read as `i64` with `as i64` on bind and `!= 0` on read. `dye_price_iqd` is nullable INTEGER -> Rust `Option<i64>`: bind the Option directly (no cast), read it straight through (no bool conversion). In the sync puller, bind `.bind(p.get("dye_price_iqd").and_then(|v| v.as_i64()))` (NULL passes through), mirroring the existing `base_price_iqd` bind — NOT the old `.unwrap_or(false) as i64` dye_supported bind.

**Migration mechanics** (verified precedents): `ALTER TABLE ... ADD COLUMN ... CHECK(...)` works (migration 022); `ALTER TABLE check_types DROP COLUMN dye_supported` works (migration 018:61, SQLite 3.35+). No table rebuild needed. Settings tombstone pattern (migration 018:46-55): `UPDATE settings SET deleted_at=..., updated_at=..., version=version+1, dirty=1 WHERE key='dye_cost_iqd' AND deleted_at IS NULL`.

**PROD DEPLOY BLOCKER (must be in the deploy task):** the prod entrypoint (`sync-server/tools/prod-entrypoint.sh:48`) runs `pnpm prisma db push` **WITHOUT `--accept-data-loss`**. Dropping `dyeSupported` from the Prisma schema is a destructive drift that will **FAIL the prod boot**. Before deploying, the `dye_supported` column must be dropped from prod Postgres manually (after the automatic pre-deploy backup) so the plain `db push` sees no drift. Do NOT deploy without handling this. (Local dev `Dockerfile.dev` uses `--accept-data-loss`, so local is unaffected.)

**No server catalog validator exists** (`validators.ts` only validates settings + visits). The ONLY server `validators.ts` touch is removing `'dye_cost_iqd'` from `PROTECTED_SETTING_KEYS` (line 19) so the client tombstone of that key is accepted instead of 422'd.

**Test-binary discovery:** catalog tests likely live in `src-tauri/tests/catalog_phase03.rs` (or similar); money in `money_math.rs` `#[cfg(test)]` + `src-tauri/tests/visits_phase05.rs`; migrations lib test in `db::migrations`. Confirm the exact binary name with `ls src-tauri/tests/ | grep -i catalog` before writing test steps. Do NOT run the whole crate test suite.

**Entity fixtures that will break to compile** (must be swept in Task 1): `check_type.rs` tests at lines ~189, 234-238, 248, 264 (`dye_supported`); `money_math.rs` `#[cfg(test)]` fixtures (`dye_supported: true` ~line 345, `dye_cost_iqd: 2000` ~line 381, `assert snap.dye_cost_iqd == 2000` ~line 494) — these belong to the money task (Task 3), not Task 1.

---

## Task 1: Catalog entities — `dye_price_iqd` on CheckType + CheckSubtype

**Files:**
- Modify: `src-tauri/src/domains/catalog/domain/entities/check_type.rs`
- Modify: `src-tauri/src/domains/catalog/domain/entities/check_subtype.rs`
- Test: same two files (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `CheckType.dye_price_iqd: Option<i64>` (replaces `dye_supported: bool`); `CheckTypeNewInput.dye_price_iqd: Option<i64>`; `CheckTypeUpdate.dye_price_iqd: Option<Option<i64>>`.
  - `CheckSubtype.dye_price_iqd: Option<i64>` (NEW); `CheckSubtypeNewInput.dye_price_iqd: Option<i64>`; `CheckSubtypeUpdate.dye_price_iqd: Option<Option<i64>>`.
  - Both constructors validate `dye_price_iqd >= 0` when `Some`.
  - `validate_xor` is UNCHANGED (dye does not participate; base price stays mandatory-when-flat). A separate inline `>= 0` check guards dye price.

- [ ] **Step 1: Write failing tests (check_type)**

Add to `check_type.rs` `#[cfg(test)] mod tests`:
```rust
    #[test]
    fn try_new_accepts_dye_price_and_defaults_none() {
        let mut i = new_input();
        i.dye_price_iqd = Some(5_000);
        let ct = CheckType::try_new(i).unwrap();
        assert_eq!(ct.dye_price_iqd, Some(5_000));

        let ct2 = CheckType::try_new(new_input()).unwrap();
        assert_eq!(ct2.dye_price_iqd, None, "default is no dye offered");
    }

    #[test]
    fn try_new_rejects_negative_dye_price() {
        let mut i = new_input();
        i.dye_price_iqd = Some(-1);
        assert!(CheckType::try_new(i).is_err());
    }

    #[test]
    fn try_new_accepts_zero_dye_price_free_dye() {
        let mut i = new_input();
        i.dye_price_iqd = Some(0);
        assert_eq!(CheckType::try_new(i).unwrap().dye_price_iqd, Some(0));
    }

    #[test]
    fn update_sets_and_clears_dye_price() {
        let ct = CheckType::try_new({ let mut i = new_input(); i.dye_price_iqd = Some(3_000); i }).unwrap();
        let set = ct.clone().with_updated_fields(CheckTypeUpdate {
            dye_price_iqd: Some(Some(7_000)),
            ..Default::default()
        }).unwrap();
        assert_eq!(set.dye_price_iqd, Some(7_000));
        let cleared = set.with_updated_fields(CheckTypeUpdate {
            dye_price_iqd: Some(None),
            ..Default::default()
        }).unwrap();
        assert_eq!(cleared.dye_price_iqd, None, "inner None clears to NULL");
    }

    #[test]
    fn toggling_to_subtyped_clears_dye_price() {
        let ct = CheckType::try_new({ let mut i = new_input(); i.dye_price_iqd = Some(6_000); i }).unwrap();
        let subtyped = ct.toggled_has_subtypes(true, None).unwrap();
        assert!(subtyped.has_subtypes);
        assert_eq!(subtyped.base_price_iqd, None);
        assert_eq!(subtyped.dye_price_iqd, None, "subtyped types carry dye on subtypes, not the check type");
    }
```
Add a `fn new_input() -> CheckTypeNewInput` helper if the existing tests don't already have one (base it on the existing `dye_supported: false` fixture at line ~189, replacing that field with `dye_price_iqd: None`).

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib domains::catalog::domain::entities::check_type`
Expected: FAIL — `no field dye_price_iqd` / `dye_supported` still referenced.

- [ ] **Step 3: Edit the CheckType entity**

In `check_type.rs`:
- Struct field (line 21): `pub dye_supported: bool,` -> `pub dye_price_iqd: Option<i64>,`.
- `CheckTypeNewInput` (line 40): `pub dye_supported: bool,` -> `pub dye_price_iqd: Option<i64>,`.
- `CheckTypeUpdate` (line 51): `pub dye_supported: Option<bool>,` -> `pub dye_price_iqd: Option<Option<i64>>,`.
- `try_new` (lines 66-86): after `validate_xor(...)?;`, add:
  ```rust
  if let Some(n) = input.dye_price_iqd {
      if n < 0 {
          return Err(AppError::Validation("dye_price_iqd must be >= 0".into()));
      }
  }
  ```
  and in the struct literal replace `dye_supported: input.dye_supported,` with `dye_price_iqd: input.dye_price_iqd,`.
- `with_updated_fields` (lines 106-108): replace the `dye_supported` block with:
  ```rust
  if let Some(dye) = patch.dye_price_iqd {
      if let Some(n) = dye {
          if n < 0 {
              return Err(AppError::Validation("dye_price_iqd must be >= 0".into()));
          }
      }
      self.dye_price_iqd = dye;
  }
  ```
- `toggled_has_subtypes` (lines 124-150): when flipping TO subtyped (`to_value == true`, the `if to_value` branch that already sets `self.base_price_iqd = None`), also clear the dye price: add `self.dye_price_iqd = None;` right after `self.base_price_iqd = None;`. A subtyped check type carries its dye price on its subtypes, so the check-type-level value must not linger as a stale dead value (parity with `base_price_iqd`). The `to_value == false` branch leaves `dye_price_iqd` as-is (caller sets it via a separate update; default `None` = dye not offered).
- Sweep the `#[cfg(test)]` fixtures that reference `dye_supported` (lines ~189, 234-238, 248, 264): replace `dye_supported: false` with `dye_price_iqd: None`; delete or rewrite the `try_new_preserves_dye_flag`-style test that sets `i.dye_supported = true` and asserts `ct.dye_supported` (the new dye-price tests above supersede it).
- Leave `validate_xor` untouched.

- [ ] **Step 4: Write failing tests (check_subtype)**

Add to `check_subtype.rs` tests:
```rust
    #[test]
    fn try_new_accepts_optional_dye_price() {
        let s = CheckSubtype::try_new({ let mut i = input("X", 1000); i.dye_price_iqd = Some(4_000); i }).unwrap();
        assert_eq!(s.dye_price_iqd, Some(4_000));
        assert_eq!(CheckSubtype::try_new(input("Y", 1000)).unwrap().dye_price_iqd, None);
    }

    #[test]
    fn try_new_rejects_negative_dye_price() {
        assert!(CheckSubtype::try_new({ let mut i = input("X", 1000); i.dye_price_iqd = Some(-1); i }).is_err());
    }

    #[test]
    fn update_sets_and_clears_dye_price() {
        let s = CheckSubtype::try_new(input("X", 1000)).unwrap();
        let set = s.with_updated_fields(CheckSubtypeUpdate {
            dye_price_iqd: Some(Some(2_500)),
            ..Default::default()
        }).unwrap();
        assert_eq!(set.dye_price_iqd, Some(2_500));
        let cleared = set.with_updated_fields(CheckSubtypeUpdate {
            dye_price_iqd: Some(None),
            ..Default::default()
        }).unwrap();
        assert_eq!(cleared.dye_price_iqd, None);
    }
```
Update the existing `fn input(name, price)` helper (lines 124-133) to add `dye_price_iqd: None`, and the full-struct literal at lines 151-159.

- [ ] **Step 5: Run to verify failure**

Run: `cargo test --lib domains::catalog::domain::entities::check_subtype`
Expected: FAIL — `no field dye_price_iqd`.

- [ ] **Step 6: Edit the CheckSubtype entity**

In `check_subtype.rs`:
- Struct (after line 15 `price_iqd`): add `pub dye_price_iqd: Option<i64>,`.
- `CheckSubtypeNewInput` (after line 32): add `pub dye_price_iqd: Option<i64>,`.
- `CheckSubtypeUpdate` (after line 42): add `pub dye_price_iqd: Option<Option<i64>>,`.
- `try_new` (after the `price_iqd < 0` check ~line 52-56): add:
  ```rust
  if let Some(n) = input.dye_price_iqd {
      if n < 0 {
          return Err(AppError::Validation("dye_price_iqd must be >= 0".into()));
      }
  }
  ```
  and add `dye_price_iqd: input.dye_price_iqd,` to the struct literal.
- `with_updated_fields` (after the `price_iqd` block ~line 93-100): add:
  ```rust
  if let Some(dye) = patch.dye_price_iqd {
      if let Some(n) = dye {
          if n < 0 {
              return Err(AppError::Validation("dye_price_iqd must be >= 0".into()));
          }
      }
      self.dye_price_iqd = dye;
  }
  ```

- [ ] **Step 7: Run both entity test modules**

```
cargo test --lib domains::catalog::domain::entities::check_type
cargo test --lib domains::catalog::domain::entities::check_subtype
```
Expected: PASS both. (`cargo check --lib` will still fail — repos/services reference the old field; that's Tasks 2+. Do NOT run `cargo check --lib` yet.)

- [ ] **Step 8: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/catalog/domain/entities/check_type.rs src/domains/catalog/domain/entities/check_subtype.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(catalog): dye_price_iqd on CheckType + CheckSubtype entities"
```

---

## Task 2: Catalog SQLite repos, push payloads, service DTOs, sync puller, seed

**Files:**
- Modify: `src-tauri/src/domains/catalog/infrastructure/repositories/check_type_repo.rs`
- Modify: `src-tauri/src/domains/catalog/infrastructure/repositories/check_subtype_repo.rs`
- Modify: `src-tauri/src/domains/catalog/service/push_payloads.rs`
- Modify: `src-tauri/src/domains/catalog/service/check_type_service.rs`
- Modify: the subtype create/update service/command site (find via `grep -rn "CheckSubtypeNewInput\|CheckSubtypeUpdate" src-tauri/src`)
- Modify: `src-tauri/src/sync/puller_entities.rs`
- Modify: `src-tauri/src/bin/seed_weekly.rs`

**Interfaces:**
- Consumes: entity fields from Task 1 (`dye_price_iqd: Option<i64>`, patch `Option<Option<i64>>`).
- Produces: repo upsert/FromRow, push payloads, service DTOs, puller SQL, and seed inserts all carrying `dye_price_iqd`; `dye_supported` removed everywhere. `CheckTypePushPayload.dye_price_iqd: Option<i64>`, `CheckSubtypePushPayload.dye_price_iqd: Option<i64>`.

Note: this task will not fully compile until the migration (Task 5) adds the columns — but `cargo check --lib` compiling the Rust against the OLD DB schema is fine (sqlx here uses runtime queries, not compile-time `query!`, so schema mismatch surfaces at test runtime, not compile). Verify with `cargo check --lib` (compiles) at the end; DB-touching tests run after Task 5.

- [ ] **Step 1: `check_type_repo.rs`**
- Upsert INSERT column list (lines 29-33): replace `dye_supported` (position 6, after `base_price_iqd`) with `dye_price_iqd`. Placeholder tuple unchanged (16).
- `ON CONFLICT DO UPDATE SET` (line 37): `dye_supported = excluded.dye_supported,` -> `dye_price_iqd = excluded.dye_price_iqd,`.
- Bind (line 48): `.bind(ct.dye_supported as i64)` -> `.bind(ct.dye_price_iqd)` (position 6, after `.bind(ct.base_price_iqd)`, before `.bind(ct.sort_order)`).
- `CheckTypeRow` field (line 154): `dye_supported: i64,` -> `dye_price_iqd: Option<i64>,`.
- `into_domain` (line 175): `dye_supported: self.dye_supported != 0,` -> `dye_price_iqd: self.dye_price_iqd,`.

- [ ] **Step 2: `check_subtype_repo.rs`**
- Upsert INSERT column list (lines 29-33): add `dye_price_iqd` right after `price_iqd`; grow placeholder tuple 14 -> 15 `?`.
- `ON CONFLICT DO UPDATE SET`: add `dye_price_iqd = excluded.dye_price_iqd,` after `price_iqd = excluded.price_iqd,`.
- Bind: insert `.bind(sub.dye_price_iqd)` between `.bind(sub.price_iqd)` (line 45) and `.bind(sub.sort_order)` (line 46).
- `CheckSubtypeRow` (between lines 95-96): add `dye_price_iqd: Option<i64>,` between `price_iqd: i64,` and `sort_order: i64,`.
- `into_domain` (between lines 114-115): add `dye_price_iqd: self.dye_price_iqd,`.

- [ ] **Step 3: `push_payloads.rs`**
- `CheckTypePushPayload` (line 18): `pub dye_supported: bool,` -> `pub dye_price_iqd: Option<i64>,` (after `base_price_iqd`).
- `From<&CheckType>` (line 36): `dye_supported: ct.dye_supported,` -> `dye_price_iqd: ct.dye_price_iqd,`.
- `CheckSubtypePushPayload` (between lines 54-55): add `pub dye_price_iqd: Option<i64>,` between `price_iqd` and `sort_order`.
- `From<&CheckSubtype>` (between lines 70-71): add `dye_price_iqd: s.dye_price_iqd,`.

- [ ] **Step 4: `check_type_service.rs` DTOs + mapping**
- `CheckTypeCreateInput` (lines 33-34): remove `#[serde(default)] pub dye_supported: bool,` -> add `pub dye_price_iqd: Option<i64>,`.
- `CheckTypeUpdateInput` (line 44): `pub dye_supported: Option<bool>,` -> `pub dye_price_iqd: Option<Option<i64>>,`.
- `create()` mapping (line 124): `dye_supported: input.dye_supported,` -> `dye_price_iqd: input.dye_price_iqd,`.
- `update()` mapping (line 178): `dye_supported: input.dye_supported,` -> `dye_price_iqd: input.dye_price_iqd,`.

- [ ] **Step 5: Subtype create/update DTO site**
- Run `grep -rn "CheckSubtypeNewInput\|CheckSubtypeUpdate" src-tauri/src` to find the subtype create/update service/command. Add `dye_price_iqd: Option<i64>` to its create DTO and `dye_price_iqd: Option<Option<i64>>` to its update DTO, and thread them into `CheckSubtypeNewInput`/`CheckSubtypeUpdate`. (There is NO `check_subtype_service.rs`; the mapping lives in the catalog service or commands — follow the grep.)

- [ ] **Step 6: `puller_entities.rs`**
- `apply_check_types_change` INSERT (lines 184-189): replace `dye_supported` with `dye_price_iqd` in the column list (placeholder count unchanged, 16, trailing `dirty` is a literal `0`).
- `ON CONFLICT SET` (line 195): `dye_supported = excluded.dye_supported,` -> `dye_price_iqd = excluded.dye_price_iqd,`.
- Bind (lines 217-221): replace the `.bind(p.get("dye_supported")...unwrap_or(false) as i64)` with `.bind(p.get("dye_price_iqd").and_then(|v| v.as_i64()))` (mirrors the `base_price_iqd` bind two lines above).
- `apply_check_subtypes_change` INSERT (lines 257-261): add `dye_price_iqd` to the column list + one more `?`; add `dye_price_iqd = excluded.dye_price_iqd,` to the SET block; add `.bind(p.get("dye_price_iqd").and_then(|v| v.as_i64()))` positioned to match column order (after the `price_iqd` bind).
- Do the same for the second check_types INSERT at line 1274 if it is a distinct code path (verify; the map flagged it).

- [ ] **Step 7: `seed_weekly.rs`**
- `insert_check_types` (line 755): replace `dye_supported` column with `dye_price_iqd`; change `.bind(c.dye as i32)` (line 763) to bind a nullable dye price from an updated seed struct field (`dye_price_iqd: Option<i64>` on the seed `CheckType` struct — default `None`, or a sensible per-seed value).
- `insert_subtypes` (line 777): add `dye_price_iqd` column + one `?` + `.bind(...)` after the `price_iqd` bind; add a dye field to the Subtype seed struct.

- [ ] **Step 8: Compile check**

Run: `cargo check --lib`
Expected: clean (compiles against the entity changes; DB schema mismatch does not fail compile for runtime sqlx). If a subtype service test binary references the DTOs, it may not compile yet — that is fine; only `--lib` must be clean here.
Then: `cargo clippy --lib -- -D warnings`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/catalog/infrastructure/repositories/check_type_repo.rs \
      src/domains/catalog/infrastructure/repositories/check_subtype_repo.rs \
      src/domains/catalog/service/push_payloads.rs \
      src/domains/catalog/service/check_type_service.rs \
      src/sync/puller_entities.rs src/bin/seed_weekly.rs
# plus the subtype DTO file found in Step 5
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(catalog): thread dye_price_iqd through repos, push, service, sync, seed"
```

---

## Task 3: Money engine — resolve dye price, drop the global setting

**Files:**
- Modify: `src-tauri/src/domains/visits/domain/services/money_math.rs`
- Modify: `src-tauri/src/domains/visits/commands.rs`
- Test: `money_math.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `CheckType.dye_price_iqd` / `CheckSubtype.dye_price_iqd` (Task 1).
- Produces: `money_math::compute` resolves dye from the catalog; `MoneySettings.dye_cost_iqd` removed; `commands.rs` no longer reads the `dye_cost_iqd` setting. Snapshot `dye_cost_iqd` still holds the resolved charged amount (unchanged field).

- [ ] **Step 1: Write/adapt failing tests**

In `money_math.rs` `#[cfg(test)]`, add (and adapt existing dye fixtures):
```rust
    #[test]
    fn dye_cost_comes_from_check_type_price_when_flat() {
        // flat check type, dye price 3000, dye on -> dye_cost 3000
        let (ct, sub, ...) = /* build a flat check type with dye_price_iqd = Some(3_000) */;
        let snap = compute(&inputs_with(ct, None, /* dye */ true, ...)).unwrap();
        assert_eq!(snap.dye_cost_iqd, 3_000);
        assert_eq!(snap.total_amount_iqd, snap.price_iqd + 3_000);
    }

    #[test]
    fn dye_cost_comes_from_subtype_price_when_subtyped() {
        // subtyped check type; chosen subtype dye_price_iqd = Some(4_000)
        let snap = compute(&inputs_with(ct_subtyped, Some(sub_dye_4000), true, ...)).unwrap();
        assert_eq!(snap.dye_cost_iqd, 4_000);
    }

    #[test]
    fn dye_price_zero_is_free_dye_not_unavailable() {
        let snap = compute(&inputs_with(ct_dye_zero, None, true, ...)).unwrap();
        assert_eq!(snap.dye_cost_iqd, 0);
        assert_eq!(snap.total_amount_iqd, snap.price_iqd);
    }

    #[test]
    fn dye_on_without_resolvable_price_errors() {
        // dye_price_iqd = None, dye on -> Validation error
        let err = compute(&inputs_with(ct_no_dye, None, true, ...)).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn dye_off_ignores_price() {
        let snap = compute(&inputs_with(ct_dye_5000, None, /* dye */ false, ...)).unwrap();
        assert_eq!(snap.dye_cost_iqd, 0);
    }
```
Adapt the existing dye fixtures (`dye_supported: true` ~345, `MoneySettings { dye_cost_iqd: 2000, .. }` ~381, `assert snap.dye_cost_iqd == 2000` ~494, ~532): set the check type's `dye_price_iqd = Some(2_000)` instead of the global setting, drop `dye_cost_iqd` from every `MoneySettings` literal, and keep the `dye_cost_iqd == 2000` assertions (now sourced from the catalog).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib domains::visits::domain::services::money_math`
Expected: FAIL — `MoneySettings` has no `dye_cost_iqd` after you remove it / fixtures reference removed field. (If you write tests before removing the field, they fail on the new `dye_price` behavior instead — either way, red first.)

- [ ] **Step 3: Edit `money_math.rs`**
- `MoneySettings` (lines 19-28): remove `pub dye_cost_iqd: i64,`.
- Add the resolver after `base_price`:
  ```rust
  fn dye_price(inputs: &MoneyMathInputs<'_>) -> Option<i64> {
      match inputs.check_subtype {
          Some(sub) => sub.dye_price_iqd,
          None => inputs.check_type.dye_price_iqd,
      }
  }
  ```
- Dye guard (lines 70-74): replace with
  ```rust
  if inputs.dye && dye_price(inputs).is_none() {
      return Err(AppError::Validation(
          "dye not available for this check".into(),
      ));
  }
  ```
- Dye cost (lines 114-118):
  ```rust
  let dye_cost = if inputs.dye {
      dye_price(inputs).ok_or_else(|| AppError::Validation("dye not available for this check".into()))?
  } else {
      0
  };
  ```
  (The guard above already caught the `None` case, so this `ok_or_else` is defense-in-depth and keeps the type an `i64`.)
- Everything downstream (`cut_base`, `total = price_iqd + dye_cost`, snapshot `dye_cost_iqd: dye_cost`) is unchanged.

- [ ] **Step 4: Edit `commands.rs`**
- In the `MoneySettings { ... }` literal (lines 59-64): remove the `dye_cost_iqd: required_i64(state, "dye_cost_iqd").await?,` line. Keep `report_pct`, `internal_doctor_pct`, `reporting_doctor_name`, and the `required_i64` helper (still used).

- [ ] **Step 5: Run money tests**

Run: `cargo test --lib domains::visits::domain::services::money_math`
Expected: PASS (all, including the new dye cases).

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/visits/domain/services/money_math.rs src/domains/visits/commands.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(money): resolve dye cost per check type/subtype; drop global dye setting"
```

---

## Task 4: Visit-service dye validation + reception grid availability

**Files:**
- Modify: `src-tauri/src/domains/visits/service/visit_service.rs`
- Test: `src-tauri/tests/visits_phase05.rs` (confirm binary name first)

**Interfaces:**
- Consumes: `dye_price_iqd` on entities; `money_math::dye_price` resolution (Task 3, private — replicate the small resolution inline in the service since it's a different module).
- Produces: `create_draft`/`update_draft` reject dye when no dye price resolves; `ChecksGridCard.dye_supported: bool` -> `dye_available: bool` (derived from resolved dye price presence).

- [ ] **Step 1: Write a failing service test**

In `visits_phase05.rs`, add a test that creating a draft with `dye = true` on a check type whose resolved `dye_price_iqd` is `None` returns a `Validation` error, and that it succeeds when the check type (or chosen subtype) has a dye price. (Follow the existing draft-creation test setup in that binary — seed a check type with `dye_price_iqd = None` vs `Some(n)`.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test visits_phase05 <new_test_name>`
Expected: FAIL (compile error on `dye_supported`, or wrong behavior).

- [ ] **Step 3: Edit `visit_service.rs`**
- Add a small private helper near the top of the impl:
  ```rust
  fn resolve_dye_price(ct: &CheckType, sub: Option<&CheckSubtype>) -> Option<i64> {
      match sub {
          Some(s) => s.dye_price_iqd,
          None => ct.dye_price_iqd,
      }
  }
  ```
- `create_draft` dye check (lines 384-388): the subtype is already fetched just above (lines 372-383) — capture it into a variable and use
  ```rust
  if input.dye && Self::resolve_dye_price(&ct, subtype.as_ref()).is_none() {
      return Err(AppError::Validation("dye not available for this check".into()));
  }
  ```
- `update_draft` dye check (lines 498-502): if only `ct` is loaded here (map says subtype is not fetched), fetch the subtype when `updated.check_subtype_id` is `Some` (mirror the create_draft fetch), then apply the same `resolve_dye_price(...).is_none()` check. If fetching the subtype here is disproportionate, the authoritative guard is `compute()` at lock (Task 3) — but per the spec, draft/update MUST reject too, so fetch the subtype and check.
- `ChecksGridCard` struct (lines 117-125): rename `pub dye_supported: bool,` -> `pub dye_available: bool,`.
- `checks_grid` population (lines 331-338): `dye_supported: ct.dye_supported,` -> `dye_available: ct.dye_price_iqd.is_some() || /* has any priced subtype */`. For a subtyped check type the check-type `dye_price_iqd` is `None`, so availability must also consider subtypes: call the subtype repo's `list_by_type(ct.id)` and OR in `subs.iter().any(|s| s.dye_price_iqd.is_some())`. (The `checks_grid` builder already loops per check type and has repo access — verify and add the subtype fetch; it is one query per card, acceptable for the grid.)

- [ ] **Step 4: Run the service test**

Run: `cargo test --test visits_phase05 <new_test_name>`
Then the whole binary for regressions: `cargo test --test visits_phase05`
Expected: PASS.

- [ ] **Step 5: Compile + lint**

```
cargo check --lib
cargo clippy --lib -- -D warnings
```
Expected: clean.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/visits/service/visit_service.rs tests/visits_phase05.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(visits): dye availability from resolved price; grid dye_available"
```

---

## Task 5: Migration 025 + consumption guard + schema lockstep

**Files:**
- Create: `src-tauri/migrations/025_dye_price_per_check_type.sql`
- Modify: `src-tauri/src/db/migrations.rs` (register 025)
- Modify: `src-tauri/src/domains/catalog/service/consumption_service.rs` (+ its `new` call sites)
- Modify: `src-tauri/src/domains/catalog/infrastructure/repositories/check_subtype_repo.rs` (only if not already done in Task 2 — the `SELECT *` FromRow must already carry `dye_price_iqd`)
- Modify: `sync-server/src/app/common/version.ts` (`SERVER_SCHEMA_VERSION` -> 25)
- Test: catalog test binary (confirm name) + `db::migrations` lib test

**Interfaces:**
- Consumes: entity + repo `dye_price_iqd` (Tasks 1-2).
- Produces: DB columns exist; `dye_supported` dropped; `dye_cost_iqd` setting tombstoned; consumption guard uses price presence; `SYNC == SERVER == 25`.

- [ ] **Step 1: Write migration 025**

Create `src-tauri/migrations/025_dye_price_per_check_type.sql`:
```sql
-- Per-check-type and per-subtype dye price replaces the global dye_cost_iqd
-- setting and the check_types.dye_supported flag.
--
-- dye_price_iqd is nullable: NULL = dye not offered here; a value (incl. 0 =
-- free dye) = dye available at that price. Resolution mirrors base_price:
-- subtype's value when the check type has subtypes, else the check type's.
--
-- No backfill: every dye_price_iqd starts NULL, so dye is off clinic-wide until
-- the accountant configures each price. Pre-launch, no dye history to preserve.
--
-- SQLite 3.35+ ALTER ADD/DROP COLUMN (see migration 018). No table rebuild.
-- Forward-only, idempotent within the migration runner (runs exactly once).

ALTER TABLE check_types ADD COLUMN dye_price_iqd INTEGER NULL
  CHECK (dye_price_iqd IS NULL OR dye_price_iqd >= 0);

ALTER TABLE check_subtypes ADD COLUMN dye_price_iqd INTEGER NULL
  CHECK (dye_price_iqd IS NULL OR dye_price_iqd >= 0);

ALTER TABLE check_types DROP COLUMN dye_supported;

-- Retire the obsolete global dye price setting (tombstone so it syncs and
-- stops appearing in the settings UI/cache). Mirrors migration 018's
-- report_cost_iqd tombstone.
UPDATE settings
   SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       version    = version + 1,
       dirty      = 1
 WHERE key = 'dye_cost_iqd' AND deleted_at IS NULL;
```

- [ ] **Step 2: Register 025 + write the failing lockstep/migration test**

In `src-tauri/src/db/migrations.rs`, add before the closing `];` of `MIGRATIONS`:
```rust
    (
        "025_dye_price_per_check_type.sql",
        include_str!("../../migrations/025_dye_price_per_check_type.sql"),
    ),
```
Add a test to the catalog test binary (confirm name via `ls src-tauri/tests/ | grep -i catalog`; else add to `db::migrations` unit tests):
```rust
#[test]
fn schema_version_is_25() {
    assert_eq!(app_lib::db::migrations::SYNC_SCHEMA_VERSION, 25);
}
```
And an integration test (in the catalog binary, using the `fresh_pool`-style harness) asserting: after migrations, `check_types` has `dye_price_iqd` and NOT `dye_supported`; `check_subtypes` has `dye_price_iqd`; the `dye_cost_iqd` setting row is tombstoned (`deleted_at IS NOT NULL`).

- [ ] **Step 3: Run to verify failure**

Run: `cargo test --lib db::migrations::` (or the catalog binary) — the `schema_version_is_25` assert fails (still 24) until registration; run before adding the tuple to see red, then after.

- [ ] **Step 4: Fix the consumption guard**

In `consumption_service.rs`:
- Add `subtype_repo: Arc<dyn CheckSubtypeRepo>` field to `ConsumptionService` (struct ~lines 39-64) and a matching `new` param.
- Update EVERY `ConsumptionService::new(...)` call site (grep `ConsumptionService::new` — likely `lib.rs` wiring) to pass the subtype repo Arc.
- Replace the guard (lines 116-121):
  ```rust
  if on_dye_only {
      let dye_available = ct.dye_price_iqd.is_some()
          || self
              .subtype_repo
              .list_by_type(ct.id)
              .await?
              .iter()
              .any(|s| s.dye_price_iqd.is_some());
      if !dye_available {
          return Err(AppError::Validation(
              "parent check_type does not support dye (errors:consumption.dye_not_supported_on_parent)".into(),
          ));
      }
  }
  ```
  (`list_by_type` already exists and filters live rows.)

- [ ] **Step 5: Server lockstep**

- `sync-server/src/app/common/version.ts` line 60: `SERVER_SCHEMA_VERSION = 24` -> `25`; update the drifting comment to mention migration 025 (dye price per check type).
- `sync-server/src/app/sync/service/validators.ts` line 19: remove `'dye_cost_iqd'` from `PROTECTED_SETTING_KEYS` so the client tombstone of that key is accepted (otherwise 422).

- [ ] **Step 6: Run tests**

```
cargo test --lib db::migrations
cargo test --test <catalog_binary>            # migration-shape + version tests
cargo test --lib domains::catalog             # consumption service unit tests if colocated
cargo check --lib && cargo clippy --lib -- -D warnings
```
Expected: PASS / clean. If a consumption service test exists, confirm the dye-availability guard test passes (dye-only rule rejected when no dye price anywhere; accepted when the check type or a subtype has a price — add this test if missing).

- [ ] **Step 7: Server type-check**

```
cd sync-server && pnpm exec tsc --noEmit
```
Expected: clean.

- [ ] **Step 8: Commit (client + server, one commit — lockstep)**

```bash
cd /home/haithem/Projects/idc-system
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src-tauri/migrations/025_dye_price_per_check_type.sql \
      src-tauri/src/db/migrations.rs \
      src-tauri/src/domains/catalog/service/consumption_service.rs \
      src-tauri/src/lib.rs \
      sync-server/src/app/common/version.ts \
      sync-server/src/app/sync/service/validators.ts
# plus the catalog test binary file
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(catalog): migration 025 dye_price columns; consumption guard; schema v25 lockstep"
```

---

## Task 6: Sync server — Prisma model + store mappers

**Files:**
- Modify: `sync-server/prisma/schema.prisma`
- Modify: `sync-server/src/app/sync/infrastructure/memory/store.ts`
- Modify: `sync-server/src/app/sync/infrastructure/prisma/entity-store.ts`

**Interfaces:**
- Consumes: the push payload shape from Task 2 (`dye_price_iqd` on both catalog records).
- Produces: server persists + round-trips `dye_price_iqd`; `dyeSupported` dropped.

- [ ] **Step 1: Prisma model**
- `schema.prisma` model `CheckType` (lines 167-168): drop `dyeSupported Boolean @default(false) @map("dye_supported")`; add `dyePriceIqd Int? @map("dye_price_iqd")`.
- model `CheckSubtype` (line 195): add `dyePriceIqd Int? @map("dye_price_iqd")` beside `priceIqd`.

- [ ] **Step 2: SyncRecord interfaces (`store.ts`)**
- `CheckTypeSyncRecord` (lines 51-52): drop `dye_supported: boolean`; add `dye_price_iqd: number | null`.
- `CheckSubtypeSyncRecord` (line 67): add `dye_price_iqd: number | null`.

- [ ] **Step 3: entity-store mappers (`entity-store.ts`)**
- `upsertCheckType` data (lines 167-169): drop `dyeSupported: row.dye_supported`; add `dyePriceIqd: row.dye_price_iqd` (spread covers create+update).
- `getCheckType` return (lines 196-197): drop `dye_supported: row.dyeSupported`; add `dye_price_iqd: row.dyePriceIqd`.
- `upsertCheckSubtype` data (line 218): add `dyePriceIqd: row.dye_price_iqd`.
- `getCheckSubtype` return (~lines 235-250): add `dye_price_iqd: row.dyePriceIqd`.
- `toCheckTypeSyncRecord` (lines 1044-1065): param type drop `dyeSupported`, add `dyePriceIqd: number | null`; return drop `dye_supported`, add `dye_price_iqd: r.dyePriceIqd`.
- `toCheckSubtypeSyncRecord` (lines 1076-1094): param type add `dyePriceIqd: number | null`; return add `dye_price_iqd: r.dyePriceIqd`.

- [ ] **Step 4: Regenerate Prisma client + type-check**

```
cd sync-server && pnpm prisma generate && pnpm exec tsc --noEmit
```
Expected: clean. (Do NOT run `prisma db push` against any real DB here — schema application happens at deploy, see Task 8.)

- [ ] **Step 5: Server unit/route tests (if any touch catalog sync)**

```
cd sync-server && pnpm test
```
Expected: PASS (or unchanged pass count; catalog sync round-trip tests now carry the field). If no catalog sync test exists, note it.

- [ ] **Step 6: Commit**

```bash
cd /home/haithem/Projects/idc-system
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add sync-server/prisma/schema.prisma \
      sync-server/src/app/sync/infrastructure/memory/store.ts \
      sync-server/src/app/sync/infrastructure/prisma/entity-store.ts
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(server): dyePriceIqd on CheckType/CheckSubtype; drop dyeSupported"
```

---

## Task 7: Frontend — catalog editor, reception, IPC types, running-total, settings decommission

**Files:**
- Modify: `src/lib/ipc.ts` (CheckType/CheckSubtype records + args; ChecksGridCardRecord; keep VisitSnapshotRecord.dye_cost_iqd)
- Modify: `src/pages/admin/check-types/detail.tsx`
- Modify: `src/pages/admin/check-types/list.tsx`
- Modify: `src/pages/reception/checks-grid.tsx`
- Modify: `src/pages/reception/new-visit-tabbed.tsx`
- Modify: `src/features/visits/format.ts` (+ `format.test.ts`)
- Modify: `src/pages/admin/settings.tsx` (remove dye_cost_iqd from the registry + pricing group)
- Modify: `src/i18n/locales/{en,ar}/admin.json` (remove the dye_cost_iqd setting label/description; optional dye-price catalog label)

**Interfaces:**
- Consumes: Rust IPC now returns/accepts `dye_price_iqd` on catalog records/args and `dye_available` on grid cards; `dye_cost_iqd` setting removed.
- Produces: catalog form edits dye price (flat check type + per subtype); reception dye toggle gated on resolved price presence; running-total uses resolved dye price.

- [ ] **Step 1: IPC types (`ipc.ts`)**
- `CheckTypeRecord.dye_supported: boolean` (line 611) -> `dye_price_iqd: number | null`.
- `CheckTypeCreateArgs` (line 626) / `CheckTypeUpdateArgs` (line 635): `dye_supported?: boolean` -> `dye_price_iqd?: number | null`.
- `CheckSubtypeRecord` (after line 645): add `dye_price_iqd: number | null`. `CheckSubtypeCreateArgs`/`CheckSubtypeUpdateArgs`: add `dye_price_iqd?: number | null`.
- `ChecksGridCardRecord.dye_supported: boolean` (line 1003) -> `dye_available: boolean`.
- Leave `VisitSnapshotRecord.dye_cost_iqd` (line 1009) UNCHANGED (locked charged amount).

- [ ] **Step 2: Catalog check-type editor (`detail.tsx`)**
- Flat mode (lines 135-146): replace the `dye_supported` checkbox with a nullable dye-price number input, mirroring the flat `base_price_iqd` input; empty string -> `null` (dye not offered). In `onSave` (line 58) parse `form.get("dye_price_iqd")` to `number | null`; send `dye_price_iqd` in `update.mutateAsync` (null when `ct.has_subtypes`, like `base_price_iqd`).
- Subtype editor (lines 83-100 create, 176-197 add-form, 198-228 table): add a `newSubDyePrice` state (nullable), a nullable dye-price input in the add-subtype form (NOT `required` — empty = null), include it in `createSubtype.mutateAsync`, reset it on success, and add a dye-price column to the subtype table (show `—` when null).

- [ ] **Step 3: Catalog list create-form (`list.tsx`)**
- Replace `dye_supported` in form state (lines 30, 39) with `dye_price_iqd: number | null` (default null); replace the create-form checkbox (lines 142-150) with a flat-mode nullable dye-price input mirroring `base_price_iqd` (lines 131-141); send `dye_price_iqd` in the submit payload (null when `has_subtypes`); flags cell (line 191) derives from `ct.dye_price_iqd != null`.

- [ ] **Step 4: Reception grid (`checks-grid.tsx`)**
- Line 86: `{card.dye_supported ? (` -> `{card.dye_available ? (`. Pill JSX unchanged.

- [ ] **Step 5: Running-total port (`format.ts` + `format.test.ts`)**
- Inputs (lines 68-69): drop `dye_supported: boolean` and the input `dye_cost_iqd: number`; add `dye_price_iqd: number | null`.
- Guard (line 112): `if (inputs.dye && inputs.dye_price_iqd == null) throw new Error("computeRunningTotal: dye not available for this check")`.
- Dye cost (line 126): `const dyeCost = inputs.dye ? inputs.dye_price_iqd : 0`.
- Snapshot output field `dye_cost_iqd` (line 87) KEEPS its name (resolved charged amount).
- Update `format.test.ts` (lines 39-40, 50, 59-62, 72, 156) to the new input shape.

- [ ] **Step 6: New-visit form (`new-visit-tabbed.tsx`)**
- Remove the `dye_cost_iqd` global-setting read (lines 87-90). Resolve dye price per check type/subtype (mirror the base-price resolution at lines 107-113: subtype's `dye_price_iqd` when `has_subtypes`, else check type's).
- `dyeApplied` (line 114), the `FeatureToggle` `disabled` guard (line 562), and the draft-payload `dye` gating (lines 226, 244) switch from `checkType?.dye_supported` to `resolvedDyePrice != null`.
- Running-total line push (line 132) + `totalIqd` (line 138) use `resolvedDyePrice` instead of `dyeCostIqd`.
- Use the nullable-number input pattern already at lines 444-473 (price-override) if a manual dye override is ever surfaced — but per spec there is NO per-visit dye override, so dye price is read-only from the catalog here.

- [ ] **Step 7: Settings decommission (`settings.tsx` + i18n)**
- `settings.tsx`: remove `dye_cost_iqd` from the settings registry (line 49) and from the pricing-group `keys` list (line 76).
- `src/i18n/locales/{en,ar}/admin.json`: remove `admin.settings.key.dye_cost_iqd` label (line ~69) + description (line ~113). Optionally add an `admin.check_types.dye_price` label (with `defaultValue`) used by the catalog inputs.

- [ ] **Step 8: Lint, typecheck, build, unit tests**

```
pnpm lint
pnpm build          # tsc -b + vite; catches type drift across ipc.ts consumers
pnpm test           # vitest incl. format.test.ts
```
Expected: clean; vitest green (format tests updated). Fix any consumer of the renamed `dye_supported` / removed `dye_cost_iqd` the typecheck surfaces.

- [ ] **Step 9: Commit**

```bash
cd /home/haithem/Projects/idc-system
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/lib/ipc.ts src/pages/admin/check-types/detail.tsx src/pages/admin/check-types/list.tsx \
      src/pages/reception/checks-grid.tsx src/pages/reception/new-visit-tabbed.tsx \
      src/features/visits/format.ts src/features/visits/format.test.ts \
      src/pages/admin/settings.tsx src/i18n/locales/en/admin.json src/i18n/locales/ar/admin.json
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(frontend): catalog dye price inputs; reception dye availability; retire dye setting"
```

---

## Task 8: Full validation, status doc, gated deploy

**Files:**
- Modify: `docs/idc-system/status.md`

- [ ] **Step 1: Full per-target Rust validation**

From `src-tauri/` (per-target only, never crate-wide):
```
cargo fmt --check
cargo clippy --lib -- -D warnings
cargo test --lib domains::catalog
cargo test --lib domains::visits::domain::services::money_math
cargo test --lib db::migrations
cargo test --test <catalog_binary>
cargo test --test visits_phase05
```
Expected: all PASS / clean. Fix root causes; never `--no-verify`.

- [ ] **Step 2: Frontend + server**

```
pnpm lint && pnpm build && pnpm test
cd sync-server && pnpm exec tsc --noEmit && pnpm test
```
Expected: clean / green.

- [ ] **Step 3: Manual smoke (deferred to human — cannot drive UI)**

Document in the status note that the manual smoke is deferred: `pnpm tauri dev` -> set a dye price on a flat check type and on a subtype in the catalog editor -> create a visit with dye -> confirm the running total adds the resolved dye price and the receipt shows it -> confirm a check type with NO dye price disables the dye toggle. If run, stop the dev server before finishing (standing rule).

- [ ] **Step 4: Update `docs/idc-system/status.md`**

Append a dated "Per-check-type dye price (2026-07-05)" note under "Blockers & Notes": what landed (per-check/subtype `dye_price_iqd`, drop `dye_supported`, retire global `dye_cost_iqd`, resolution mirrors base price, availability = price present, schema v25 lockstep, consumption guard), verification results, and the two deferred items (manual UI smoke; the deploy column-drop below).

- [ ] **Step 5: Commit the status note**

```bash
cd /home/haithem/Projects/idc-system
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add docs/idc-system/status.md
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "docs(status): per-check-type dye price + schema v25"
```

- [ ] **Step 6: GATED DEPLOY (do NOT run without explicit user go-ahead)**

Deploying bumps `SERVER_SCHEMA_VERSION` to 25 and drops the `dye_supported` Postgres column. **The prod entrypoint runs `prisma db push` WITHOUT `--accept-data-loss`, so the column drop is a destructive drift that FAILS the boot unless handled.** When the user authorizes deploy:
1. Push `main` to GitHub.
2. On the VPS, BEFORE the rebuild, take the manual step so `db push` sees no drift: after the entrypoint's automatic pre-deploy `pg_dump` backup, drop the obsolete column explicitly:
   ```bash
   ssh root@Personal 'docker exec idc-sync-db psql -U postgres -d idc_sync -c "ALTER TABLE check_types DROP COLUMN IF EXISTS dye_supported;"'
   ```
   (Adding the two nullable `dye_price_iqd` columns is non-destructive; `prisma db push` creates them cleanly. Only the DROP needs the manual pre-step.)
3. Rebuild per the standard flow (`git pull` + `docker compose ... up -d --build sync-server`).
4. Verify: `SERVER_SCHEMA_VERSION` = 25 in the running container; `check_types`/`check_subtypes` have `dye_price_iqd`; boot logs show `db push` "in sync" with no data-loss error.
5. A client with the v25 build syncs; confirm catalog `dye_price_iqd` round-trips.

---

## Self-Review

**Spec coverage:**
- §3.1 schema (dye_price_iqd on both tables, nullable >=0; drop dye_supported) -> Task 1 (entities) + Task 5 (migration) + Task 6 (Prisma). ✓
- §3.2 resolution (subtype-or-checktype) -> Task 3 (`dye_price` in money_math) + Task 4 (`resolve_dye_price` in visit_service) + Task 7 (frontend resolution). ✓
- §3.3 engine (dye_cost from resolved price; drop MoneySettings.dye_cost_iqd; hard error) -> Task 3. ✓
- §3.4 visit-service draft/update rejection -> Task 4. ✓
- §3.5 migration 025 (add cols, drop dye_supported, no backfill, tombstone dye_cost_iqd) -> Task 5. ✓
- §3.6 lockstep 25/25 -> Task 5 (client + server version) + Task 6 (Prisma). ✓
- §4 sync (push payloads + puller + server mappers) -> Task 2 (desktop) + Task 6 (server). ✓
- §5 frontend (catalog form, reception, running-total, settings) -> Task 7. ✓
- §6 edge cases: (1) dye-on-no-price hard error -> Tasks 3+4; (2) subtype opt-out -> Task 4 resolution + Task 7 UI; (3) Some(0) vs None -> Task 1 + Task 3 tests; (4) has_subtypes re-homing -> Task 1 clears check-type `dye_price_iqd` to `None` in `toggled_has_subtypes` (parity with `base_price_iqd`), with a test; (5) locked immutable -> snapshot unchanged, add a lock->bump-price->reread test in Task 4 or Task 8; (6) retired dye_cost_iqd -> Task 3 (commands read removed) + Task 5 (tombstone) + Task 7 (settings UI) + Task 5 (PROTECTED_SETTING_KEYS); (7) consumption guard -> Task 5. ✓
- §7 testing -> per-task tests + Task 8. ✓

**Placeholder scan:** Task 3 Step 1 and Task 4 Step 1 use `/* build ... */` sketches for fixtures because the exact fixture builders are binary-specific — the implementer must follow the existing fixture pattern in that file. This is a soft spot; acceptable because the surrounding test structure and assertions are concrete, but the implementer must read the neighboring tests to build the check_type/subtype fixtures. Everything else has concrete code.

**Type consistency:** `dye_price_iqd: Option<i64>` (entity/struct), `Option<Option<i64>>` (update patch), `number | null` (TS), `Int?` (Prisma), `dye_available: bool`/`boolean` (grid), snapshot `dye_cost_iqd` kept everywhere. `resolve_dye_price`/`dye_price` helpers return `Option<i64>`. Consistent across tasks.

**Locked-immutability test placement:** Edge case §6.5 (changing a catalog dye price never mutates a locked visit's `dye_cost_snapshot_iqd`) is asserted by the money engine snapshotting at lock — add an explicit test in the visits integration binary (Task 4 or Task 8): lock a dye visit, mutate the check type's `dye_price_iqd`, re-read the locked visit, assert `dye_cost_snapshot_iqd` unchanged. This is the one edge case whose test isn't yet written into a specific step's code block; the implementer must add it following the existing lock-test pattern in `visits_phase05.rs`.
