-- Phase 8 polish migration.
--
-- Adds `sync_state.last_audit_vacuum_at` (phase-08 §7.19) which the audit
-- vacuum scheduler reads on app start to detect missed runs and writes after
-- each successful daily vacuum (phase-08 §7.2).
--
-- Adds a single covering index on `audit_log(action, at DESC)` to back the
-- filter-pill chip queries (the action enum is one of 14 values).
--
-- Forward-only; the `ALTER TABLE ... ADD COLUMN` is idempotent only via the
-- `_migrations` gate (SQLite has no `ADD COLUMN IF NOT EXISTS`).

ALTER TABLE sync_state ADD COLUMN last_audit_vacuum_at TEXT NULL;

CREATE INDEX IF NOT EXISTS audit_log_action_at
  ON audit_log(entity_id_tenant, action, at DESC);

CREATE INDEX IF NOT EXISTS audit_log_actor_at
  ON audit_log(entity_id_tenant, actor_user_id, at DESC);
