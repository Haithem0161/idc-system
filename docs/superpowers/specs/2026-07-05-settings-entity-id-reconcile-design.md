# Settings entity_id Split-Brain Fix — Design

**Date:** 2026-07-05
**Status:** approved (design), ready for implementation plan
**Surfaces:** SQLite + Tauri/Rust, Sync Server (Prisma/Fastify) for schema-version lockstep only

## 1. Problem

Money settings (`dye_cost_iqd`, `report_pct`, `internal_doctor_pct`, `reporting_doctor_name`, etc.)
have two disjoint `entity_id` scopes that never reconcile, so the money engine reads stale
seed defaults instead of the accountant's configured values.

- **Seed** (`migration 002_users_settings.sql:43-45`): rows inserted with `entity_id = 'unscoped'`
  (dye 10,000, report_pct 20 after migration 018, internal_doctor_pct 30).
- **Startup cache-warm** (`lib.rs:355`, `lib.rs:691`): `warm_settings_cache(app, "unscoped")` — runs at
  boot, before login, when no tenant is known. Loads the `'unscoped'` rows into `AppState.settings_cache`.
- **Edits + sync** (`settings/commands.rs:228`): `update_batch(..., &ctx.entity_id, ...)` — writes under
  the logged-in user's REAL tenant `entity_id` (e.g. `3627804e-…`).

The unique index is `(entity_id, key) WHERE deleted_at IS NULL`, so the `'unscoped'` seed row and the
tenant row do NOT collide — both stay live. `get_setting` reads from the cache, which was warmed from
`'unscoped'`, so an accountant's edit lands in the tenant row but never reaches the money engine across
an app restart.

**Observed on prod (device `019f0de3`):** two live rows per money key — `'unscoped'` seed (dye 10k,
report 20, internal 30) and tenant `3627804e-…` (dye 60k, report 25, internal 25). The money breakdown
computed with the seed values (e.g. doctor cut off `price − 10000` and report at 20%), not the
configured values (`price − 60000`, report 25%).

**Scope decision:** settings are PER-TENANT (scoped to the real `entity_id`), consistent with every other
entity (users/visits/doctors/operators all use the real tenant id; only seed `settings` use `'unscoped'`).
The fix unifies everything on the real tenant `entity_id`.

## 2. Root-cause timing

`migrations::run` executes at startup (`lib.rs:351`), BEFORE any user exists. `users_create_first_admin`
and login happen later (a user action). So:
- A pure migration cannot reliably fix fresh installs (no tenant to fold into at migration time).
- The tenant becomes known only at `set_current_user` (called after login AND after
  session-restore-on-startup: `auth/commands.rs:87, 340`).

Therefore the reconcile must run in CODE at `set_current_user`, where the tenant is known. This covers
both fresh installs (first-admin creation → login) and existing installs (every login re-checks).

## 3. Target design

### 3.1 Reconcile-then-warm at `set_current_user` (the core fix)

Introduce one idempotent function, `reconcile_settings_scope(tenant_entity_id)`, called from the same
choke point that sets the user context. For each live `'unscoped'` settings row:

1. If a live tenant-scoped row already exists for that `key` → the tenant row is authoritative
   (it holds the accountant's edit or the prior reconcile). **Tombstone the `'unscoped'` duplicate**
   (soft-delete: set `deleted_at`, bump `version`, `dirty = 1`) so the tombstone syncs and other
   devices + prod converge.
2. If NO live tenant-scoped row exists for that `key` (never edited) → **re-point the `'unscoped'` row
   to the tenant**: update its `entity_id` to `tenant_entity_id`, bump `version`, set `dirty = 1` so the
   re-scope syncs. (Keeps the value; only the scope changes.)

Run inside one transaction. Idempotent: after it runs once, there are no live `'unscoped'` money rows,
so re-running is a no-op. It executes on every `set_current_user` but does meaningful work only while
`'unscoped'` rows remain.

Immediately AFTER reconcile, **re-warm the settings cache with the tenant `entity_id`** (not `'unscoped'`),
so the money engine reads the tenant's values for the rest of the session. The existing pre-login
`'unscoped'` warm at `lib.rs:691` stays (the login screen / pre-auth paths keep defaults); the tenant
re-warm layers on top once a user is present.

**Conflict / sync semantics:** `settings` is last-write-wins per row (unchanged). The reconcile produces
two kinds of syncable change — a tombstone on the `'unscoped'` row and/or a re-pointed tenant row — both
ride the normal LWW settings sync. When multiple devices run the reconcile, they converge: the tombstone
of the `'unscoped'` row is idempotent, and the re-point creates the same tenant `(entity_id, key)` which
LWW-merges by `updated_at`.

### 3.2 Harden `get_setting` / `find_by_key` (defense-in-depth)

`find_by_key` (`sqlite_setting_repo.rs:59`) is `SELECT * FROM settings WHERE key = ? AND entity_id = ?
AND deleted_at IS NULL` — no `LIMIT 1`. The cache warm's `list(entity_id)` similarly returns all rows for
the scope. After 3.1 there is one live row per (tenant, key), but to be resilient if a duplicate ever
reappears, make the read deterministic: order so the tenant-scoped, newest row wins, and `LIMIT 1` on the
single-key read. This guarantees the engine never silently reads a stale duplicate again.

### 3.3 Convenience migration (existing logged-in installs)

`024_settings_tenant_reconcile.sql` — a forward-only migration that performs the SAME fold as 3.1 in SQL,
guarded to no-op when no users exist. It is INCLUDED (not optional) and drives the schema-version bump,
because its tombstones / re-points are syncable changes. It is belt-and-suspenders relative to the code
path (3.1), which remains the source of truth:

```
tenant = (SELECT entity_id FROM users WHERE deleted_at IS NULL ORDER BY created_at LIMIT 1)
-- only when tenant IS NOT NULL:
--   tombstone each 'unscoped' money row whose key already has a live tenant row
--   re-point each remaining 'unscoped' money row to tenant
```

This is a convenience so an already-logged-in device converges on next launch even before a login event
fires. Because the code path (3.1) is the source of truth and runs at `set_current_user`, the migration is
belt-and-suspenders; it is included because it self-heals existing installs through the normal migration
path without waiting for a re-login. Fresh installs (no users at migration time) rely on 3.1.

## 4. Surfaces touched

- **SQLite**: one migration `024_settings_tenant_reconcile.sql`. `SYNC_SCHEMA_VERSION` → 24
  (= migration count). **Server `SERVER_SCHEMA_VERSION` → 24 in lockstep** — settings sync, so the
  tombstones / re-points propagate; a version mismatch would surface as opaque push VALIDATION_ERRORs.
  No Prisma model change (settings columns already exist).
- **Rust**: `reconcile_settings_scope` (new; in the settings service/domain), called from
  `set_current_user` (state.rs / auth commands) followed by the tenant re-warm. `find_by_key` +
  `list` read hardening. No new IPC command.
- **Frontend**: none. The settings page already writes under the tenant.

## 5. Data reconciliation (existing bad data)

Existing duplicate rows on every device + prod converge via §3.1 (runs at next login) and/or §3.3
(runs at next launch). The `'unscoped'` rows are tombstoned or re-pointed; the tenant values are kept as
authoritative. No manual prod DB surgery. After deploy + a client sync round-trip, prod's `settings`
table has exactly one live tenant-scoped row per key.

## 6. Verification

1. **The exact bug:** on a DB seeded like prod (unscoped seed + tenant edits), log in, restart the app,
   lock a visit — confirm the doctor cut / report use the TENANT values (dye 60k, report 25%,
   internal 25%), not the seed defaults (dye 10k, report 20%, internal 30%).
2. **Reconcile idempotency:** run `reconcile_settings_scope` twice; second run is a no-op; exactly one
   live row per (tenant, key); `'unscoped'` money rows are tombstoned.
3. **Re-point path:** a key that was NEVER edited (only the `'unscoped'` seed exists) ends up re-pointed
   to the tenant with its value intact.
4. **Read hardening:** with a duplicate injected, `find_by_key` returns the tenant row deterministically.
5. **Sync round-trip:** the `'unscoped'` tombstone + re-pointed tenant rows propagate; a second device /
   prod dedups to one live row per key.
6. **Schema lockstep:** `SYNC_SCHEMA_VERSION == SERVER_SCHEMA_VERSION == 24`.
7. **Fresh install:** no users at migration time → migration 024 no-ops; first-admin creation → login →
   §3.1 folds the seed rows into the new tenant; money engine reads correct values.

## 7. Non-goals

- No change to the settings sync conflict policy (stays LWW per key).
- No change to which settings exist or their semantics.
- No change to the money engine itself (the paid-basis feature is correct; it was reading the wrong rows).
- No multi-tenant support beyond the single real tenant per install that already exists.
