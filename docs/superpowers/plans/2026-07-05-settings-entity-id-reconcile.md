# Settings entity_id Split-Brain Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the money engine read the tenant's configured settings instead of stale `'unscoped'` seed defaults, by reconciling every local `'unscoped'` settings row into the real tenant scope at login and re-warming the cache with the tenant id.

**Architecture:** A single idempotent reconcile — for each live `'unscoped'` settings row, tombstone it if a live tenant row already holds that key, else re-point it to the tenant — runs in Rust at `set_current_user` (the choke point that first knows the tenant), immediately followed by a tenant-scoped cache re-warm. A convenience migration performs the same fold in SQL so already-logged-in devices self-heal on next launch. Reads are hardened so a duplicate can never silently win again. The tombstones and re-points ride the normal LWW settings sync, so all devices and prod converge to one live tenant row per key.

**Tech Stack:** Rust (sqlx, tokio, async-trait, chrono, uuid), SQLite, Tauri v2; sync server (TypeScript) touched only for the `SERVER_SCHEMA_VERSION` lockstep bump.

## Global Constraints

- **No Claude authorship in commits.** No `Co-Authored-By: Claude`, no Anthropic emails, no `git config` changes. Commits appear solely human-made. Every commit uses `git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" commit ...`.
- **NEVER run full `cargo test`, `cargo check --all-targets`, `cargo clippy --all-targets`, or `cargo build`** — they crash the user's IDE. Use per-target commands only: `cargo check --lib`, `cargo clippy --lib -- -D warnings`, `cargo test --lib <module::path>`, `cargo test --test <binary> <name>`. Run all Rust commands from `src-tauri/`.
- **No heavy multi-agent parallel fan-out** during execution — it crashes the machine. Single subagent per task at most.
- **Context7 first (MANDATORY)** before writing code against any library API (sqlx query builder, chrono, uuid, async-trait). Query `resolve-library-id` then `query-docs`.
- **No emojis** anywhere — code, comments, commit messages, user-facing strings.
- **Schema-version lockstep:** desktop `SYNC_SCHEMA_VERSION` (= count of `src-tauri/migrations/*.sql`) MUST equal server `SERVER_SCHEMA_VERSION` (`sync-server/src/app/common/version.ts`). Adding migration `024` makes both **24**; bump the server constant in the SAME commit as the migration.
- **Offline-first invariants:** every mutation bumps `version`, sets `dirty = 1`, and rides the outbox/LWW sync. Deletes are tombstones (`deleted_at`), never hard deletes. Client-side timestamps via the local clock (`chrono::Utc::now()`), RFC3339.
- **`settings` conflict policy stays last-write-wins per `(entity_id, key)`.** Do not change it.
- **Package management:** `cargo add` / `pnpm add` only — never hand-edit `Cargo.toml [dependencies]` or `package.json` dependency sections. (This plan adds no new dependencies.)

## Context the implementer needs (read before starting)

**The bug (verified against the live local DB and prod):**
- Migration `002_users_settings.sql:42-52` and `018_report_percentage.sql:53-55` seed money/config settings with `entity_id = 'unscoped'`.
- Boot warms the cache from `'unscoped'` BEFORE any login: `lib.rs:691` → `warm_settings_cache(app, &entity_id_tenant)` where `entity_id_tenant = "unscoped"` (`lib.rs:355`, computed before login).
- Accountant edits + sync write under the REAL tenant: `settings/commands.rs:228` → `update_batch(.., &ctx.entity_id, ..)`.
- The partial unique index is `settings(entity_id, key) WHERE deleted_at IS NULL` (`002:39-40`), so the `'unscoped'` seed row and the tenant row do NOT collide — both stay live.
- `set_current_user` (`state.rs:347`) only sets in-memory `user_context`; it never re-warms the cache. So across a restart the money engine keeps reading `'unscoped'` seed values.

**Verified live local state** (`~/.local/share/com.idc.system/idc-local.db`), which the reconcile must handle:
- Money keys have BOTH scopes live: `dye_cost_iqd` (tenant `60000` / unscoped `10000`), `internal_doctor_pct` (tenant `25` / unscoped `30`), `report_pct` (tenant `25` / unscoped `20`). → **tombstone the unscoped duplicate** (case 1).
- Config keys have ONLY the unscoped seed (`arabic_numerals`, `currency_symbol`, `idle_lock_minutes`, `thermal_width`, `thermal_printer_name`, `reporting_doctor_name`, `clinic_display_name_ar/en`). → **re-point unscoped → tenant** (case 2).
- `report_cost_iqd` is tombstoned in BOTH scopes (migration 018). → reconcile only touches `deleted_at IS NULL` rows, so it is skipped.
- The tenant on this device is `3627804e-3594-4d6f-9e8c-b157e460e7f4`.

**Verified prod state** (`idc_sync` DB): only tenant-scoped rows exist (dye 60k, report_pct 25, internal 25); the tombstoned `report_cost_iqd` is tenant-scoped. **Prod has NO live `'unscoped'` rows.** So the reconcile's tombstones/re-points from clients are idempotent on the server, and no manual prod DB surgery is needed. The "reconcile prod data" concern is a no-op on the server — convergence is purely client-driven.

**Key code locations:**
- `Setting` entity: `src-tauri/src/domains/settings/domain/entities/mod.rs` — has `new_local()` and `updated_with(value)` (bumps `version`, sets `dirty`, keeps `entity_id`). NO re-point or tombstone method yet — Task 1 adds them.
- `SettingRepo` trait: `src-tauri/src/domains/settings/domain/repositories/mod.rs`.
- sqlx repo: `src-tauri/src/domains/settings/infrastructure/repositories/sqlite_setting_repo.rs` — `get_by_key` (`:57-66`, no `LIMIT 1`, non-deterministic on duplicate), `list` (`:68-76`).
- Service: `src-tauri/src/domains/settings/service.rs` — `SettingsService` holds `pool`, `setting_repo`, `audit_repo`, `outbox_repo`, `device_id`, `writer: AuditWriter`. `update_batch` (`:140`) shows the tx + audit + outbox pattern to mirror.
- `AppState`: `src-tauri/src/state.rs` — `settings_service()` accessor, `set_current_user` (`:347`), `entity_id_tenant()` (`:398`), `set_setting` (`:378`).
- Boot: `src-tauri/src/lib.rs` — `warm_settings_cache` (`:699`), boot call (`:691`).
- `set_current_user` call sites (ALL four — none are test helpers):
  1. `auth/commands.rs:87` — `auth_login_impl` (online login), `state: &AppState`.
  2. `auth/commands.rs:340` — session restore on startup, `state: &AppState`.
  3. `auth/commands.rs:716` — first-admin online, `state: State<'_, AppState>`.
  4. `auth/commands.rs:744` — first-admin offline fallback, `state: State<'_, AppState>`.
- Migrations array: `src-tauri/src/db/migrations.rs:14-107` (register `024` before the closing `];` at `:107`); `SYNC_SCHEMA_VERSION` derives from `MIGRATIONS.len()` (`:114`).
- Server version: `sync-server/src/app/common/version.ts:60`.
- Test harness pattern: `src-tauri/tests/settings_phase02.rs:19-42` (`fresh_pool()`, `make_service()`).

**The reconcile MUST skip `report_cost_iqd`** implicitly by only processing `deleted_at IS NULL` rows — it is already tombstoned in both scopes, so it is never selected. No special-casing needed.

---

## Task 1: `Setting` entity — `repointed_to` and `tombstoned` methods

**Files:**
- Modify: `src-tauri/src/domains/settings/domain/entities/mod.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: existing `Setting` struct fields and `updated_with` for the mutation shape.
- Produces:
  - `Setting::repointed_to(self, tenant_entity_id: &str) -> Setting` — returns a copy with `entity_id = tenant_entity_id`, `version += 1`, `dirty = true`, `updated_at = now`, value/key/id unchanged, `deleted_at` unchanged (None).
  - `Setting::tombstoned(self) -> Setting` — returns a copy with `deleted_at = Some(now)`, `version += 1`, `dirty = true`, `updated_at = now`, everything else unchanged.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `src-tauri/src/domains/settings/domain/entities/mod.rs`:

```rust
    #[test]
    fn repointed_to_changes_entity_and_bumps_version_keeps_value() {
        let s = Setting::new_local("dye_cost_iqd", SettingValue::Int(10_000), "unscoped", None)
            .unwrap();
        let v0 = s.version;
        let id0 = s.id;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let r = s.repointed_to("tenant-1");
        assert_eq!(r.entity_id, "tenant-1");
        assert_eq!(r.value, SettingValue::Int(10_000));
        assert_eq!(r.key, "dye_cost_iqd");
        assert_eq!(r.id, id0, "re-point keeps the same row id");
        assert_eq!(r.version, v0 + 1);
        assert!(r.dirty);
        assert!(r.deleted_at.is_none());
    }

    #[test]
    fn tombstoned_sets_deleted_at_and_bumps_version() {
        let s = Setting::new_local("dye_cost_iqd", SettingValue::Int(10_000), "unscoped", None)
            .unwrap();
        let v0 = s.version;
        let id0 = s.id;
        let t = s.tombstoned();
        assert!(t.deleted_at.is_some());
        assert_eq!(t.entity_id, "unscoped", "tombstone keeps the original scope");
        assert_eq!(t.id, id0);
        assert_eq!(t.version, v0 + 1);
        assert!(t.dirty);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib domains::settings::domain::entities`
Expected: FAIL — `no method named repointed_to` / `no method named tombstoned`.

- [ ] **Step 3: Implement the two methods**

Add inside `impl Setting` in `src-tauri/src/domains/settings/domain/entities/mod.rs`, right after `updated_with`:

```rust
    /// Move this row to a different tenant scope without touching its value.
    /// Bumps `version` and marks `dirty` so the re-scope syncs (LWW).
    pub fn repointed_to(mut self, tenant_entity_id: &str) -> Self {
        self.entity_id = tenant_entity_id.to_string();
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        self
    }

    /// Soft-delete this row. Bumps `version` and marks `dirty` so the tombstone
    /// syncs and other devices + the server hide the row (LWW).
    pub fn tombstoned(mut self) -> Self {
        self.deleted_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        self
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib domains::settings::domain::entities`
Expected: PASS (all entity tests, including the two new).

- [ ] **Step 5: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/settings/domain/entities/mod.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(settings): Setting::repointed_to + tombstoned for scope reconcile"
```

---

## Task 2: `SettingRepo` — list live `'unscoped'` rows + deterministic reads

**Files:**
- Modify: `src-tauri/src/domains/settings/domain/repositories/mod.rs` (trait)
- Modify: `src-tauri/src/domains/settings/infrastructure/repositories/sqlite_setting_repo.rs` (impl)
- Test: `src-tauri/tests/settings_phase02.rs`

**Interfaces:**
- Consumes: `Setting` entity, `SqliteSettingRepo`, existing `fresh_pool()` / `make_service()` test helpers.
- Produces (new trait methods):
  - `list_live_by_entity(&self, entity_id: &str) -> AppResult<Vec<Setting>>` — all rows for the scope with `deleted_at IS NULL`, ordered by `key`. (Used by the reconcile to enumerate `'unscoped'` rows.)
  - `has_live_key(&self, key: &str, entity_id: &str) -> AppResult<bool>` — true iff a live (`deleted_at IS NULL`) row exists for `(entity_id, key)`.
  - `update_row_by_id(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()>` — a plain `UPDATE ... WHERE id = ?` that rewrites every mutable column (`entity_id`, `value`, `value_type`, `updated_at`, `deleted_at`, `version`, `dirty`) from the given `Setting`. Unlike `upsert`, it does NOT go through the `ON CONFLICT(entity_id, key) WHERE deleted_at IS NULL` path, so it correctly applies a **tombstone** (which sets `deleted_at` and therefore no longer matches that partial index) and a **re-point** (which changes `entity_id`). The reconcile uses this, never `upsert`.
- Modifies (read hardening): `get_by_key` and `list` gain deterministic ordering + `LIMIT 1` on the single-key read so a duplicate never silently wins.

**Why a dedicated update method (do not reuse `upsert`):** `upsert` is `INSERT ... ON CONFLICT(entity_id, key) WHERE deleted_at IS NULL DO UPDATE`. Tombstoning a row sets `deleted_at`, so the incoming row no longer matches that partial unique index; the `INSERT` then collides on the `id` PRIMARY KEY, which has NO `ON CONFLICT` handler → a `UNIQUE constraint failed: settings.id` error. Re-pointing changes `entity_id`, which also would not match the original conflict target. An `UPDATE ... WHERE id = ?` sidesteps both — it targets the exact existing row by its stable PK.

- [ ] **Step 1: Write the failing tests**

Add to `src-tauri/tests/settings_phase02.rs`:

```rust
#[tokio::test]
async fn list_live_by_entity_returns_only_live_rows_for_scope() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    // Seed migration already inserted the 'unscoped' rows; add a tenant row.
    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "dye_cost_iqd",
        SettingValue::Int(60_000),
    )
    .await
    .unwrap();

    let unscoped = repo.list_live_by_entity("unscoped").await.unwrap();
    assert!(
        unscoped.iter().all(|s| s.entity_id == "unscoped" && s.deleted_at.is_none()),
        "only live unscoped rows"
    );
    assert!(
        unscoped.iter().any(|s| s.key == "dye_cost_iqd"),
        "the unscoped dye seed is present"
    );

    let tenant = repo.list_live_by_entity("tenant-1").await.unwrap();
    assert_eq!(tenant.len(), 1);
    assert_eq!(tenant[0].key, "dye_cost_iqd");
    assert_eq!(tenant[0].value, SettingValue::Int(60_000));
}

#[tokio::test]
async fn has_live_key_true_only_for_live_scoped_row() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();

    assert!(repo.has_live_key("dye_cost_iqd", "unscoped").await.unwrap());
    assert!(!repo.has_live_key("dye_cost_iqd", "tenant-1").await.unwrap());

    svc.update(
        actor,
        UserRole::Superadmin,
        "tenant-1",
        "dye_cost_iqd",
        SettingValue::Int(60_000),
    )
    .await
    .unwrap();
    assert!(repo.has_live_key("dye_cost_iqd", "tenant-1").await.unwrap());
}

#[tokio::test]
async fn update_row_by_id_applies_tombstone_and_repoint_without_conflict() {
    let pool = fresh_pool().await;
    let (_svc, repo) = make_service(&pool, "dev-A");

    // Take a live 'unscoped' seed row and tombstone it via update_row_by_id.
    let row = repo
        .get_by_key("dye_cost_iqd", "unscoped")
        .await
        .unwrap()
        .unwrap();
    let tomb = row.clone().tombstoned();
    let mut tx = pool.begin().await.unwrap();
    repo.update_row_by_id(&mut tx, &tomb).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        repo.get_by_key("dye_cost_iqd", "unscoped").await.unwrap().is_none(),
        "tombstone hides the row"
    );

    // Re-point a different live 'unscoped' row to a tenant; no conflict, value kept.
    let cfg = repo
        .get_by_key("arabic_numerals", "unscoped")
        .await
        .unwrap()
        .unwrap();
    let repointed = cfg.clone().repointed_to("tenant-1");
    let mut tx = pool.begin().await.unwrap();
    repo.update_row_by_id(&mut tx, &repointed).await.unwrap();
    tx.commit().await.unwrap();
    assert!(repo.get_by_key("arabic_numerals", "unscoped").await.unwrap().is_none());
    let moved = repo
        .get_by_key("arabic_numerals", "tenant-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.value, cfg.value);
}
```

This test needs `Setting::tombstoned` / `repointed_to` (Task 1) in scope; the `Setting` entity is already re-exported and used elsewhere in this binary. If the entity type is not directly imported in `settings_phase02.rs`, add `use app_lib::domains::settings::domain::entities::Setting;` — but the methods are called on values returned by `get_by_key`, so no explicit type name is needed.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test settings_phase02 list_live_by_entity_returns_only_live_rows_for_scope`
Expected: FAIL — `no method named list_live_by_entity`.

- [ ] **Step 3: Add the trait methods**

In `src-tauri/src/domains/settings/domain/repositories/mod.rs`, add to the `SettingRepo` trait (after `list`):

```rust
    /// All live (`deleted_at IS NULL`) rows for a scope, ordered by `key`.
    /// Used by the login-time scope reconcile to enumerate `'unscoped'` rows.
    async fn list_live_by_entity(&self, entity_id: &str) -> AppResult<Vec<Setting>>;

    /// True iff a live (`deleted_at IS NULL`) row exists for `(entity_id, key)`.
    async fn has_live_key(&self, key: &str, entity_id: &str) -> AppResult<bool>;

    /// Rewrite every mutable column of an EXISTING row, matched by `id`. Unlike
    /// `upsert`, this does not use the `(entity_id, key)` conflict path, so it
    /// safely applies a tombstone (sets `deleted_at`) or a re-point (changes
    /// `entity_id`). Used only by the scope reconcile.
    async fn update_row_by_id(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()>;
```

The trait's `use` block already imports `Tx` (see the existing `upsert` signature); no new import needed.

- [ ] **Step 4: Implement the trait methods and harden reads**

In `src-tauri/src/domains/settings/infrastructure/repositories/sqlite_setting_repo.rs`:

Replace the body of `get_by_key` (currently `:57-66`) with a deterministic, capped read:

```rust
    async fn get_by_key(&self, key: &str, entity_id: &str) -> AppResult<Option<Setting>> {
        // Deterministic even if a duplicate ever slips past the reconcile:
        // newest row wins, tie-broken by id, capped to one.
        let row: Option<SettingRow> = sqlx::query_as::<_, SettingRow>(
            "SELECT * FROM settings \
             WHERE key = ? AND entity_id = ? AND deleted_at IS NULL \
             ORDER BY updated_at DESC, id DESC \
             LIMIT 1",
        )
        .bind(key)
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(SettingRow::into_domain).transpose()
    }
```

Add deterministic ordering to `list` (currently `:68-76`) — change its `ORDER BY key ASC` to `ORDER BY key ASC, updated_at DESC, id DESC` so that if two live rows ever share a key within a scope, the newest is first (the cache-warm loop overwrites by key, so first-seen order matters):

```rust
    async fn list(&self, entity_id: &str) -> AppResult<Vec<Setting>> {
        let rows: Vec<SettingRow> = sqlx::query_as::<_, SettingRow>(
            "SELECT * FROM settings WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY key ASC, updated_at DESC, id DESC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SettingRow::into_domain).collect()
    }
```

Add the two new methods to `impl SettingRepo for SqliteSettingRepo` (after `list`):

```rust
    async fn list_live_by_entity(&self, entity_id: &str) -> AppResult<Vec<Setting>> {
        let rows: Vec<SettingRow> = sqlx::query_as::<_, SettingRow>(
            "SELECT * FROM settings WHERE entity_id = ? AND deleted_at IS NULL \
             ORDER BY key ASC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SettingRow::into_domain).collect()
    }

    async fn has_live_key(&self, key: &str, entity_id: &str) -> AppResult<bool> {
        let found: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM settings \
             WHERE key = ? AND entity_id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(key)
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(found.is_some())
    }

    async fn update_row_by_id(&self, tx: &mut Tx<'_>, setting: &Setting) -> AppResult<()> {
        sqlx::query(
            "UPDATE settings SET \
                entity_id = ?, value = ?, value_type = ?, updated_at = ?, \
                deleted_at = ?, version = ?, dirty = ? \
             WHERE id = ?",
        )
        .bind(&setting.entity_id)
        .bind(setting.value.as_storage())
        .bind(setting.value.value_type())
        .bind(setting.updated_at.to_rfc3339())
        .bind(setting.deleted_at.map(|d| d.to_rfc3339()))
        .bind(setting.version)
        .bind(setting.dirty as i64)
        .bind(setting.id.to_string())
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
```

`Tx` is already imported at the top of `sqlite_setting_repo.rs` (`use crate::db::Tx;`).

Note: `list` and `list_live_by_entity` have the same predicate today; both are kept because `list` is the cache-warm path (its ordering is the read-hardening concern) while `list_live_by_entity` is the reconcile enumerator (intent-named per the repository pattern). Do not collapse them.

- [ ] **Step 5: Run tests to verify they pass**

Run each, one at a time:
```
cargo test --test settings_phase02 list_live_by_entity_returns_only_live_rows_for_scope
cargo test --test settings_phase02 has_live_key_true_only_for_live_scoped_row
cargo test --test settings_phase02 update_row_by_id_applies_tombstone_and_repoint_without_conflict
```
Expected: PASS all three.

- [ ] **Step 6: Guard against a regression in existing settings tests**

Run: `cargo test --test settings_phase02`
Expected: PASS (all pre-existing tests still green with the reordered reads).

- [ ] **Step 7: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/settings/domain/repositories/mod.rs \
      src/domains/settings/infrastructure/repositories/sqlite_setting_repo.rs \
      tests/settings_phase02.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(settings): reconcile-enumerator repo methods + deterministic reads"
```

---

## Task 3: `SettingsService::reconcile_scope` — the core fold

**Files:**
- Modify: `src-tauri/src/domains/settings/service.rs`
- Test: `src-tauri/tests/settings_phase02.rs`

**Interfaces:**
- Consumes: `SettingRepo::list_live_by_entity`, `SettingRepo::has_live_key`, `SettingRepo::update_row_by_id` (all Task 2), `Setting::repointed_to`, `Setting::tombstoned` (Task 1), the service's `pool` + `outbox_repo` (existing fields).
- Produces:
  - `SettingsService::reconcile_scope(&self, tenant_entity_id: &str) -> AppResult<ReconcileOutcome>` where
    `pub struct ReconcileOutcome { pub repointed: usize, pub tombstoned: usize }`.
  - Behavior: for each live `'unscoped'` row, if `has_live_key(key, tenant)` → `tombstoned()` and `update_row_by_id`; else `repointed_to(tenant)` and `update_row_by_id`. All in one tx. Enqueue one `OutboxOp::new("settings", id, SettingPushPayload)` per changed row so the change syncs. Idempotent: a no-op once no live `'unscoped'` rows remain. When `tenant_entity_id == "unscoped"` it returns `ReconcileOutcome { 0, 0 }` without touching the DB (nothing to fold into).

- [ ] **Step 1: Write the failing integration tests**

Add to `src-tauri/tests/settings_phase02.rs`:

```rust
#[tokio::test]
async fn reconcile_scope_tombstones_duplicates_and_repoints_singletons() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();
    let tenant = "3627804e-3594-4d6f-9e8c-b157e460e7f4";

    // Tenant already edited dye + report_pct + internal (the accountant's values).
    for (k, v) in [
        ("dye_cost_iqd", 60_000),
        ("report_pct", 25),
        ("internal_doctor_pct", 25),
    ] {
        svc.update(actor, UserRole::Superadmin, tenant, k, SettingValue::Int(v))
            .await
            .unwrap();
    }

    let out = svc.reconcile_scope(tenant).await.unwrap();
    assert!(out.tombstoned >= 3, "3 money keys had tenant dupes to tombstone");
    assert!(out.repointed >= 1, "config-only keys got re-pointed");

    // No live 'unscoped' rows remain.
    let unscoped = repo.list_live_by_entity("unscoped").await.unwrap();
    assert!(unscoped.is_empty(), "unscoped fully folded, got {unscoped:?}");

    // Tenant keeps the edited money values (tombstone won, not re-point).
    let dye = repo.get_by_key("dye_cost_iqd", tenant).await.unwrap().unwrap();
    assert_eq!(dye.value, SettingValue::Int(60_000));
    let rp = repo.get_by_key("report_pct", tenant).await.unwrap().unwrap();
    assert_eq!(rp.value, SettingValue::Int(25));

    // A config-only key (arabic_numerals) is now under the tenant with its value.
    let an = repo
        .get_by_key("arabic_numerals", tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(an.value, SettingValue::Bool(false));
    assert!(repo.get_by_key("arabic_numerals", "unscoped").await.unwrap().is_none());
}

#[tokio::test]
async fn reconcile_scope_is_idempotent() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");
    let tenant = "tenant-1";

    let first = svc.reconcile_scope(tenant).await.unwrap();
    assert!(first.repointed + first.tombstoned > 0, "first run does work");

    let second = svc.reconcile_scope(tenant).await.unwrap();
    assert_eq!(second.repointed, 0);
    assert_eq!(second.tombstoned, 0);

    assert!(repo.list_live_by_entity("unscoped").await.unwrap().is_empty());
}

#[tokio::test]
async fn reconcile_scope_noop_for_unscoped_tenant() {
    let pool = fresh_pool().await;
    let (svc, repo) = make_service(&pool, "dev-A");

    let out = svc.reconcile_scope("unscoped").await.unwrap();
    assert_eq!(out.repointed, 0);
    assert_eq!(out.tombstoned, 0);
    // Unscoped rows are untouched (still live) when there is no real tenant.
    assert!(!repo.list_live_by_entity("unscoped").await.unwrap().is_empty());
}

#[tokio::test]
async fn reconcile_scope_enqueues_one_outbox_op_per_changed_row() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let tenant = "tenant-1";

    let out = svc.reconcile_scope(tenant).await.unwrap();
    let changed = out.repointed + out.tombstoned;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE entity = 'settings'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0 as usize, changed, "one settings op per changed row");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test settings_phase02 reconcile_scope_is_idempotent`
Expected: FAIL — `no method named reconcile_scope`.

- [ ] **Step 3: Implement `reconcile_scope` and `ReconcileOutcome`**

In `src-tauri/src/domains/settings/service.rs`, add the outcome struct near the top (after the imports, before `impl SettingsService`):

```rust
/// Result of a `reconcile_scope` run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileOutcome {
    pub repointed: usize,
    pub tombstoned: usize,
}
```

Add the method inside `impl SettingsService` (after `update_batch`):

```rust
    /// Fold every live `'unscoped'` settings row into `tenant_entity_id`:
    /// tombstone the unscoped row when the tenant already has that key live,
    /// otherwise re-point it to the tenant. Runs in one transaction, enqueues a
    /// `settings` outbox op per changed row (so the change syncs LWW), and is
    /// idempotent — a no-op once no live `'unscoped'` rows remain. A `tenant_id`
    /// of `"unscoped"` (no real tenant yet) is a no-op.
    pub async fn reconcile_scope(
        &self,
        tenant_entity_id: &str,
    ) -> AppResult<ReconcileOutcome> {
        if tenant_entity_id == "unscoped" {
            return Ok(ReconcileOutcome::default());
        }

        let unscoped = self.setting_repo.list_live_by_entity("unscoped").await?;
        if unscoped.is_empty() {
            return Ok(ReconcileOutcome::default());
        }

        // Classify BEFORE opening the tx (reads only). For each unscoped row,
        // decide tombstone-vs-repoint by whether the tenant already holds the key.
        let mut plan: Vec<(Setting, bool)> = Vec::with_capacity(unscoped.len());
        for row in unscoped {
            let tenant_has = self
                .setting_repo
                .has_live_key(&row.key, tenant_entity_id)
                .await?;
            plan.push((row, tenant_has));
        }

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let mut out = ReconcileOutcome::default();

        for (row, tenant_has) in plan {
            let changed = if tenant_has {
                out.tombstoned += 1;
                row.tombstoned()
            } else {
                out.repointed += 1;
                row.repointed_to(tenant_entity_id)
            };
            // MUST be update_row_by_id, NOT upsert: a tombstone sets deleted_at
            // (row no longer matches the partial unique index) and a re-point
            // changes entity_id, so the ON CONFLICT(entity_id, key) path would
            // fall through to an id-PK collision. UPDATE ... WHERE id = ? targets
            // the exact existing row.
            self.setting_repo.update_row_by_id(&mut tx, &changed).await?;
            let payload = serde_json::to_vec(&SettingPushPayload::from(&changed))?;
            let op = OutboxOp::new("settings", changed.id.to_string(), payload);
            self.outbox_repo.enqueue(&mut tx, &op).await?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(out)
    }
```

`SettingPushPayload` is a private type already defined in `service.rs` (used by `update_batch`/`resync_ops`); `OutboxOp` and `AppError` are already imported there. No new imports needed beyond `ReconcileOutcome` being defined in this same file.

- [ ] **Step 4: Run tests to verify they pass**

Run, one at a time (never the whole binary in a way that risks the crash constraint — per-test is fine):
```
cargo test --test settings_phase02 reconcile_scope_tombstones_duplicates_and_repoints_singletons
cargo test --test settings_phase02 reconcile_scope_is_idempotent
cargo test --test settings_phase02 reconcile_scope_noop_for_unscoped_tenant
cargo test --test settings_phase02 reconcile_scope_enqueues_one_outbox_op_per_changed_row
```
Expected: PASS all four.

- [ ] **Step 5: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/domains/settings/service.rs tests/settings_phase02.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(settings): reconcile_scope folds unscoped rows into the tenant"
```

---

## Task 4: `AppState::reconcile_and_warm_settings` + wire into all four login sites

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/domains/auth/commands.rs` (four call sites)
- Test: `src-tauri/tests/auth_phase02.rs` (integration through the state helper)

**Interfaces:**
- Consumes: `SettingsService::reconcile_scope` (Task 3), `SettingsService::list`, `AppState::set_setting`, `AppState::settings_service`.
- Produces:
  - `AppState::reconcile_and_warm_settings(&self, tenant_entity_id: &str) -> AppResult<()>` — runs `reconcile_scope(tenant)`, then re-warms `settings_cache` by loading `settings_service().list(tenant)` and calling `set_setting(key, value.to_cache_json())` for each. Logs the outcome. Never panics: a reconcile error is logged and swallowed (login must not fail because of a settings fold), but the re-warm still runs so the cache is at least tenant-scoped.
- Called from the four `set_current_user` sites immediately AFTER the user context is set, using the same `entity_id` that was just set.

- [ ] **Step 1: Write the failing integration test**

Add to `src-tauri/tests/auth_phase02.rs` (or wherever `AppState` is constructed for integration; if `auth_phase02.rs` lacks an `AppState` builder, add the test to `settings_phase02.rs` and drive the state helper there). Prefer `settings_phase02.rs` if it already builds a full `AppState`; otherwise use this service-level equivalent that asserts the cache re-warm effect through a constructed `AppState`. If constructing a full `AppState` in a test is heavy, assert the same behavior at the service level (reconcile + list returns tenant rows):

```rust
#[tokio::test]
async fn reconcile_then_list_yields_tenant_scoped_money_values() {
    let pool = fresh_pool().await;
    let (svc, _repo) = make_service(&pool, "dev-A");
    let actor = Uuid::now_v7();
    let tenant = "tenant-1";

    for (k, v) in [
        ("dye_cost_iqd", 60_000),
        ("report_pct", 25),
        ("internal_doctor_pct", 25),
    ] {
        svc.update(actor, UserRole::Superadmin, tenant, k, SettingValue::Int(v))
            .await
            .unwrap();
    }

    svc.reconcile_scope(tenant).await.unwrap();

    // What the cache-warm loop reads after login: list(tenant) must carry the
    // tenant money values AND the re-pointed config keys, with no unscoped rows.
    let rows = svc.list(tenant).await.unwrap();
    let get = |k: &str| rows.iter().find(|s| s.key == k).map(|s| s.value.clone());
    assert_eq!(get("dye_cost_iqd"), Some(SettingValue::Int(60_000)));
    assert_eq!(get("report_pct"), Some(SettingValue::Int(25)));
    assert_eq!(get("internal_doctor_pct"), Some(SettingValue::Int(25)));
    assert_eq!(get("arabic_numerals"), Some(SettingValue::Bool(false)));
    assert!(svc.list("unscoped").await.unwrap().is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test settings_phase02 reconcile_then_list_yields_tenant_scoped_money_values`
Expected: FAIL only if `reconcile_scope` is not yet callable in this binary — since Task 3 added it, this test may actually PASS at the service level. If it PASSES, that confirms the service contract; the state wiring below is the real deliverable of Task 4 and is verified by build + manual smoke (Step 6). Keep the test as a guard.

- [ ] **Step 3: Add `reconcile_and_warm_settings` to `AppState`**

In `src-tauri/src/state.rs`, add to `impl AppState` (near `entity_id_tenant`, after `set_setting`/`get_setting`):

```rust
    /// Fold any stale `'unscoped'` settings into `tenant_entity_id`, then re-warm
    /// the in-memory cache from the tenant scope so the money engine reads the
    /// tenant's configured values for the rest of the session. Called right after
    /// `set_current_user`. A reconcile failure is logged and swallowed (login must
    /// not fail on a settings fold); the tenant re-warm still runs.
    pub async fn reconcile_and_warm_settings(&self, tenant_entity_id: &str) -> AppResult<()> {
        let Some(svc) = self.settings_service() else {
            tracing::warn!("settings service unavailable; scope reconcile skipped");
            return Ok(());
        };
        match svc.reconcile_scope(tenant_entity_id).await {
            Ok(out) => tracing::info!(
                repointed = out.repointed,
                tombstoned = out.tombstoned,
                tenant = %tenant_entity_id,
                "settings scope reconciled"
            ),
            Err(e) => tracing::warn!(error = %e, "settings scope reconcile failed; re-warming anyway"),
        }
        match svc.list(tenant_entity_id).await {
            Ok(settings) => {
                for s in &settings {
                    self.set_setting(s.key.clone(), s.value.to_cache_json()).await;
                }
                tracing::info!(count = settings.len(), tenant = %tenant_entity_id, "settings cache re-warmed for tenant");
            }
            Err(e) => tracing::warn!(error = %e, "tenant settings re-warm failed"),
        }
        Ok(())
    }
```

Confirm `AppResult` and `tracing` are already imported in `state.rs` (they are used elsewhere); if `AppResult` is not in scope, add `use crate::error::AppResult;`.

- [ ] **Step 4: Wire it into all four `set_current_user` sites**

In `src-tauri/src/domains/auth/commands.rs`, immediately after each `set_current_user(...)`, add a reconcile+warm call using the entity_id that was just set. For each site, insert one line:

Site 1 — `auth_login_impl` (`:87`), after `state.set_current_user(ctx).await;`:
```rust
    let _ = state.reconcile_and_warm_settings(&result.entity_id).await;
```

Site 2 — session restore (`:340`), after `state.set_current_user(session.user.clone()).await;`:
```rust
    let _ = state
        .reconcile_and_warm_settings(&session.user.entity_id)
        .await;
```

Site 3 — first-admin online (`:716-723`), after the `set_current_user(UserContext { .. }).await;` block:
```rust
    let _ = state.reconcile_and_warm_settings(&result.entity_id).await;
```

Site 4 — first-admin offline fallback (`:744-751`), after the `set_current_user(UserContext { .. }).await;` block:
```rust
    let _ = state.reconcile_and_warm_settings(&user.entity_id).await;
```

Use the exact field already in scope at each site (`result.entity_id`, `session.user.entity_id`, `user.entity_id`). Verify each name resolves via `cargo check --lib`.

- [ ] **Step 5: Run the guard test + build**

```
cargo test --test settings_phase02 reconcile_then_list_yields_tenant_scoped_money_values
cargo check --lib
cargo clippy --lib -- -D warnings
```
Expected: test PASS, `cargo check --lib` clean, clippy clean.

- [ ] **Step 6: Manual smoke (the exact bug)**

Reproduce against the real local DB (which has the unscoped/tenant split confirmed above):
```
pnpm tauri dev
```
Log in as the accountant/superadmin, then lock a visit that carries dye + a house/internal or reporting-doctor cut. Confirm the money breakdown uses the TENANT values (dye 60,000; report 25%; internal 25%) — not the seed defaults (10,000 / 20% / 30%). Then fully quit and relaunch, log in again, lock another visit, and confirm it STILL uses the tenant values (proves the split-brain is closed across restarts). **Stop the dev server before ending the task** (per the standing "kill dev servers" rule).

- [ ] **Step 7: Commit**

```bash
cd src-tauri && cargo fmt
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src/state.rs src/domains/auth/commands.rs tests/settings_phase02.rs
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(settings): reconcile + tenant re-warm at every login choke point"
```

---

## Task 5: Convenience migration `024` + schema-version lockstep

**Files:**
- Create: `src-tauri/migrations/024_settings_tenant_reconcile.sql`
- Modify: `src-tauri/src/db/migrations.rs` (register `024`)
- Modify: `sync-server/src/app/common/version.ts` (`SERVER_SCHEMA_VERSION` → 24)
- Test: `src-tauri/tests/settings_phase02.rs` (fresh-DB migration behavior)

**Interfaces:**
- Consumes: the settings table + partial unique index from migrations 002/018.
- Produces: a forward-only migration that, on a device that already has a `users` row, performs the SAME fold as `reconcile_scope` in SQL (tombstone unscoped money dupes, re-point unscoped singletons), guarded to no-op when no users exist. After registration, `SYNC_SCHEMA_VERSION == 24` and `SERVER_SCHEMA_VERSION == 24`.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/tests/settings_phase02.rs`:

Two tests: one proves migration 024 no-ops on a fresh install (no user), and one proves the fold branch by replaying the two migration UPDATEs against a pool seeded with a user. To avoid duplicating the migration SQL in the test, load the migration file's text and execute it, so the test exercises the EXACT statements that ship.

```rust
/// Migration 024 must NO-OP on a fresh install: fresh_pool runs ALL migrations
/// (incl. 024) before any user exists, so the 'unscoped' seed rows stay live.
#[tokio::test]
async fn migration_024_noops_when_no_user_exists() {
    let pool = fresh_pool().await;
    let repo = app_lib::domains::settings::infrastructure::SqliteSettingRepo::new(pool.clone());
    let unscoped = SettingRepo::list_live_by_entity(&repo, "unscoped")
        .await
        .unwrap();
    assert!(
        !unscoped.is_empty(),
        "no user at migration time -> 024 no-ops, unscoped seed stays live"
    );
    // And no rows were spuriously moved to any tenant.
    let any_tenant: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM settings WHERE entity_id <> 'unscoped' AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(any_tenant.0, 0, "fresh install has no tenant-scoped settings");
}

/// Migration 024 fold branch: with a user present, replaying the migration's
/// exact SQL tombstones unscoped money dupes and re-points unscoped singletons.
#[tokio::test]
async fn migration_024_folds_unscoped_into_tenant_when_user_present() {
    let pool = fresh_pool().await;
    let tenant = "3627804e-3594-4d6f-9e8c-b157e460e7f4";

    // Seed a user under the tenant (satisfies every NOT NULL users column).
    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at, entity_id) \
         VALUES (?, 'a@b.co', 'Admin', 'x', 'superadmin', \
                 strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'), ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    // Create a tenant-scoped dye row so dye is the "dupe -> tombstone" case;
    // every other seed key has no tenant row -> "re-point" case.
    let (svc, repo) = make_service(&pool, "dev-A");
    svc.update(
        Uuid::now_v7(),
        UserRole::Superadmin,
        tenant,
        "dye_cost_iqd",
        SettingValue::Int(60_000),
    )
    .await
    .unwrap();

    // Replay the SHIPPING migration SQL (no duplication of statements in-test).
    let sql = include_str!("../migrations/024_settings_tenant_reconcile.sql");
    for stmt in sql.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() || stmt.starts_with("--") {
            continue;
        }
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }

    // No live 'unscoped' rows remain.
    let unscoped = SettingRepo::list_live_by_entity(&repo, "unscoped")
        .await
        .unwrap();
    assert!(unscoped.is_empty(), "unscoped folded, got {unscoped:?}");

    // dye kept the tenant's edited value (tombstone won, not re-point).
    let dye = repo.get_by_key("dye_cost_iqd", tenant).await.unwrap().unwrap();
    assert_eq!(dye.value, SettingValue::Int(60_000));

    // A config-only seed key was re-pointed to the tenant with its value.
    let an = repo
        .get_by_key("arabic_numerals", tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(an.value, SettingValue::Bool(false));
}

#[test]
fn schema_version_is_24() {
    assert_eq!(app_lib::db::migrations::SYNC_SCHEMA_VERSION, 24);
}
```

Note: the `include_str!` path is relative to `src-tauri/tests/settings_phase02.rs`, so `../migrations/024_settings_tenant_reconcile.sql` resolves to `src-tauri/migrations/024_settings_tenant_reconcile.sql`. The naive `split(';')` statement loop is acceptable here because migration 024 contains no semicolons inside string literals or comments on the same line as a statement terminator (verify when writing 024 — keep each `strftime(...)` and subquery free of a trailing `;` except the real statement terminators).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test settings_phase02 schema_version_is_24`
Expected: FAIL — `SYNC_SCHEMA_VERSION` is still 23 (migration 024 not yet registered).

- [ ] **Step 3: Write migration `024`**

Create `src-tauri/migrations/024_settings_tenant_reconcile.sql`:

```sql
-- Settings entity_id split-brain reconcile (convenience / self-heal).
--
-- Money/config settings were seeded under entity_id = 'unscoped' (migrations
-- 002, 018) but edited and synced under the real tenant entity_id. The partial
-- unique index settings(entity_id, key) WHERE deleted_at IS NULL keeps both a
-- stale 'unscoped' seed row and the tenant row live, so the cache (warmed from
-- 'unscoped' at boot) reads seed defaults instead of the configured values.
--
-- This migration performs the SAME fold as the runtime reconcile
-- (SettingsService::reconcile_scope) for an ALREADY-LOGGED-IN device, so it
-- self-heals on next launch without waiting for a re-login. The runtime code
-- path remains the source of truth and covers fresh installs (no users at
-- migration time -> this migration no-ops).
--
-- Tenant = the first non-deleted user's entity_id. On a fresh install there is
-- no user yet, so every statement below no-ops (the subquery is NULL and every
-- WHERE fails). Only 'unscoped' live rows are touched; tombstoned rows
-- (e.g. report_cost_iqd) are skipped by the deleted_at IS NULL predicate.
--
-- Conflict policy: settings stays last-write-wins per (entity_id, key). Every
-- changed row bumps version and sets dirty = 1 so the tombstone / re-point
-- syncs and other devices + the server converge to one live tenant row per key.

-- 1) Tombstone each live 'unscoped' row whose key already has a live tenant row
--    (the tenant row holds the accountant's edit and is authoritative).
UPDATE settings
   SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       version    = version + 1,
       dirty      = 1
 WHERE entity_id = 'unscoped'
   AND deleted_at IS NULL
   AND EXISTS (
     SELECT 1 FROM users u WHERE u.deleted_at IS NULL
   )
   AND EXISTS (
     SELECT 1 FROM settings t
      WHERE t.key = settings.key
        AND t.deleted_at IS NULL
        AND t.entity_id = (
          SELECT entity_id FROM users
           WHERE deleted_at IS NULL
           ORDER BY created_at ASC LIMIT 1
        )
   );

-- 2) Re-point each remaining live 'unscoped' row (no tenant row for that key)
--    to the tenant, keeping its value.
UPDATE settings
   SET entity_id  = (
         SELECT entity_id FROM users
          WHERE deleted_at IS NULL
          ORDER BY created_at ASC LIMIT 1
       ),
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       version    = version + 1,
       dirty      = 1
 WHERE entity_id = 'unscoped'
   AND deleted_at IS NULL
   AND EXISTS (
     SELECT 1 FROM users u WHERE u.deleted_at IS NULL
   )
   AND NOT EXISTS (
     SELECT 1 FROM settings t
      WHERE t.key = settings.key
        AND t.deleted_at IS NULL
        AND t.entity_id = (
          SELECT entity_id FROM users
           WHERE deleted_at IS NULL
           ORDER BY created_at ASC LIMIT 1
        )
   );
```

- [ ] **Step 4: Register `024` and bump the server version**

In `src-tauri/src/db/migrations.rs`, add before the closing `];` (currently `:107`):

```rust
    (
        "024_settings_tenant_reconcile.sql",
        include_str!("../../migrations/024_settings_tenant_reconcile.sql"),
    ),
```

In `sync-server/src/app/common/version.ts`, change `:60`:

```typescript
export const SERVER_SCHEMA_VERSION = 24
```

Also update the drifting comment just above it (`:53-56`) from "currently 23 ... migration 023" to "currently 24 ... migration 024 which folds stale 'unscoped' settings into the tenant scope".

- [ ] **Step 5: Run tests to verify they pass**

```
cargo test --test settings_phase02 schema_version_is_24
cargo test --test settings_phase02 migration_024_noops_when_no_user_exists
cargo test --test settings_phase02 migration_024_folds_unscoped_into_tenant_when_user_present
```
Expected: PASS all three. The migration-count assertion in `migrations.rs` unit tests (`:264`) also validates all 24 apply cleanly:
```
cargo test --lib db::migrations
```
Expected: PASS (24 migrations recorded).

- [ ] **Step 6: Verify the sync server still type-checks**

The server change is a single constant + comment; confirm no TS break:
```
cd sync-server && pnpm exec tsc --noEmit
```
Expected: no errors. (If `tsc` is slow/heavy, `pnpm lint` on the changed file is an acceptable lighter check.)

- [ ] **Step 7: Commit (both surfaces, one commit — lockstep)**

```bash
cd /home/haithem/Projects/idc-system
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add src-tauri/migrations/024_settings_tenant_reconcile.sql \
      src-tauri/src/db/migrations.rs \
      src-tauri/tests/settings_phase02.rs \
      sync-server/src/app/common/version.ts
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "feat(settings): migration 024 unscoped->tenant fold; schema version 24 lockstep"
```

---

## Task 6: Pre-push validation + status doc

**Files:**
- Modify: `docs/idc-system/status.md` (or the active plan's status doc) — completion note.

**Interfaces:** none (verification + docs).

- [ ] **Step 1: Full Rust validation (per-target only)**

From `src-tauri/`, run the settings/auth binaries and the changed lib modules (never the crate-wide `cargo test`):
```
cargo fmt --check
cargo clippy --lib -- -D warnings
cargo test --lib domains::settings
cargo test --lib db::migrations
cargo test --test settings_phase02
cargo test --test auth_phase02
```
Expected: all PASS / clean. Fix root causes; never `--no-verify`.

- [ ] **Step 2: Frontend + server lint (unchanged surfaces, sanity only)**

The frontend is untouched by this fix. Confirm the server constant edit builds:
```
cd sync-server && pnpm lint
```
Expected: clean.

- [ ] **Step 3: Update the status doc**

In `docs/idc-system/status.md`, append a completion note under "Blockers & Notes": what landed (Setting scope methods, reconcile-enumerator repo methods + hardened reads, `reconcile_scope`, `reconcile_and_warm_settings` wired at all four login sites, migration 024, schema version 24 lockstep), verification results (clippy clean; settings/auth/migration tests green; manual restart smoke confirms tenant money values persist), and the note that **prod needs no manual surgery** — it already holds only tenant-scoped rows, so client reconciles are idempotent on the server. Bump any cumulative counts the doc tracks (migrations 23 → 24).

- [ ] **Step 4: Commit the status update**

```bash
cd /home/haithem/Projects/idc-system
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  add docs/idc-system/status.md
git -c user.name="Haithem" -c user.email="cloud.torchcorp@gmail.com" \
  commit -m "docs(status): settings entity_id reconcile + schema v24"
```

- [ ] **Step 5: Deploy + prod convergence check (do NOT run without explicit user go-ahead)**

This is a deploy step; the user must explicitly authorize the push + deploy. When authorized:
1. Push the branch and merge to `main` (per repo git rules — no Claude authorship).
2. Deploy the sync server per root `CLAUDE.md` (`git pull` on the VPS + `docker compose ... up -d --build sync-server`).
3. Verify the server schema version:
   ```bash
   ssh root@Personal 'docker exec idc-sync-server sh -lc "grep SERVER_SCHEMA_VERSION /app/dist/app/common/version.js"'
   ```
   Expected: `24`.
4. After a client with this build logs in and syncs, confirm prod still has exactly one live row per key (it already does; the client tombstones/re-points are no-ops on the server because prod has no 'unscoped' rows):
   ```bash
   ssh root@Personal 'docker exec idc-sync-db psql -U postgres -d idc_sync -tAc "SELECT entity_id, key, value FROM settings WHERE deleted_at IS NULL ORDER BY key;"'
   ```
   Expected: one tenant-scoped row per live key, no `'unscoped'` rows.

---

## Self-Review

**Spec coverage:**
- §3.1 reconcile-then-warm at `set_current_user` → Tasks 3 (fold) + 4 (state helper + all four login sites + tenant re-warm). ✓
- §3.2 harden `get_setting`/`find_by_key` reads → Task 2 (deterministic `ORDER BY ... LIMIT 1` on `get_by_key`, ordered `list`). ✓ (Note: the codebase method is `get_by_key`, not `find_by_key` as the spec wrote — plan uses the real name.)
- §3.3 convenience migration → Task 5. ✓
- §4 surfaces (SQLite migration + version 24; Rust reconcile + reads; no frontend; server version lockstep) → Tasks 2–5. ✓
- §5 data reconciliation (client-driven convergence; no prod surgery) → Task 6 Step 5 + context section. ✓ Corrected against verified prod state: prod has no 'unscoped' rows.
- §6 verification (exact bug, idempotency, re-point path, read hardening, sync round-trip, schema lockstep, fresh install) → covered across Task 3 tests (idempotency, re-point, tombstone), Task 4 Step 6 (exact bug across restart), Task 2 tests (read hardening enumerators), Task 5 tests (schema lockstep + fresh-install no-op). ✓
- §7 non-goals honored: no conflict-policy change, no setting-semantics change, no money-engine change, single-tenant. ✓

**Placeholder scan:** No TBD/TODO; every code step shows the code; commands have expected outcomes. The one soft spot is Task 4 Step 1's "prefer settings_phase02.rs" guidance — resolved by giving a concrete service-level test that works in that binary, so there is no unfilled blank.

**Type consistency:** `reconcile_scope(&self, &str) -> AppResult<ReconcileOutcome>` used identically in Tasks 3 and 4. `ReconcileOutcome { repointed, tombstoned }` fields consistent across all tests. `repointed_to(&str)` / `tombstoned()` from Task 1 used verbatim in Task 3. `list_live_by_entity` / `has_live_key` from Task 2 used verbatim in Task 3. `reconcile_and_warm_settings(&str) -> AppResult<()>` from Task 4 used at all four call sites. `SYNC_SCHEMA_VERSION`/`SERVER_SCHEMA_VERSION` both → 24 in the same commit (Task 5). ✓

**Correction baked in:** unlike the spec's assumption that prod may hold duplicate rows, the plan states (verified) that prod is already tenant-only, so no prod data surgery — only client-driven convergence.
