-- Phase 1 foundation migration.
-- Creates outbox, sync_state, audit_log, metrics_events.
-- Idempotent (CREATE TABLE IF NOT EXISTS).

CREATE TABLE IF NOT EXISTS outbox (
  op_id            TEXT PRIMARY KEY,
  entity           TEXT NOT NULL,
  entity_id        TEXT NOT NULL,
  op               TEXT NOT NULL CHECK (op = 'upsert'),
  payload          BLOB NOT NULL,
  created_at       TEXT NOT NULL,
  attempts         INTEGER NOT NULL DEFAULT 0,
  next_attempt_at  TEXT NOT NULL,
  last_error       TEXT NULL,
  parked           INTEGER NOT NULL DEFAULT 0 CHECK (parked IN (0, 1))
);
CREATE INDEX IF NOT EXISTS outbox_next_attempt
  ON outbox(next_attempt_at)
  WHERE attempts < 10 AND parked = 0;

CREATE TABLE IF NOT EXISTS sync_state (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  pull_cursor     TEXT NULL,
  last_pulled_at  TEXT NULL,
  last_pushed_at  TEXT NULL,
  device_id       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_log (
  id                TEXT PRIMARY KEY,
  actor_user_id     TEXT NOT NULL,
  action            TEXT NOT NULL,
  entity            TEXT NOT NULL,
  entity_id         TEXT NOT NULL,
  delta             TEXT NOT NULL,
  ip                TEXT NULL,
  device_id         TEXT NOT NULL,
  at                TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id_tenant  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS audit_log_entity
  ON audit_log(entity, entity_id, at);
CREATE INDEX IF NOT EXISTS audit_log_actor
  ON audit_log(actor_user_id, at);
CREATE INDEX IF NOT EXISTS audit_log_tenant_at
  ON audit_log(entity_id_tenant, at DESC);

CREATE TABLE IF NOT EXISTS metrics_events (
  id           TEXT PRIMARY KEY,
  kind         TEXT NOT NULL CHECK (kind IN (
                  'lock_start','lock_end',
                  'receipt_print_ok','receipt_print_fail',
                  'sync_push_ok','sync_push_fail',
                  'sync_pull_ok','sync_pull_fail',
                  'sync_conflict')),
  at           TEXT NOT NULL,
  payload_json TEXT NULL,
  entity_id    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS metrics_events_kind_at
  ON metrics_events(entity_id, kind, at);
