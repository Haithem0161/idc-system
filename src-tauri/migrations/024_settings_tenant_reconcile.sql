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
-- WHERE fails). Only 'unscoped' live rows are touched -- tombstoned rows
-- (e.g. report_cost_iqd) are skipped by the deleted_at IS NULL predicate.
--
-- Conflict policy: settings stays last-write-wins per (entity_id, key). Every
-- changed row bumps version and sets dirty = 1 so the tombstone / re-point
-- syncs and other devices + the server converge to one live tenant row per key.
--
-- Statement 1 tombstones each live 'unscoped' row whose key already has a live
-- tenant row (the tenant row holds the accountant's edit and is authoritative).
-- Statement 2 re-points each remaining live 'unscoped' row (no tenant row for
-- that key) to the tenant, keeping its value.
SELECT 1;

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
