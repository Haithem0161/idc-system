-- Phase 4 operator shifts (PRD §6.1.8).
-- Adds `operator_shifts` plus partial unique enforcement of "one open shift
-- per operator" and an index covering the history_today query.
-- ON DELETE RESTRICT on user FKs documents the explicit no-hard-delete
-- intent for users (phase-04 §7.14).

CREATE TABLE IF NOT EXISTS operator_shifts (
  id                       TEXT PRIMARY KEY,
  operator_id              TEXT NOT NULL REFERENCES operators(id),
  check_in_at              TEXT NOT NULL,
  check_out_at             TEXT NULL,
  check_in_by_user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  check_out_by_user_id     TEXT NULL REFERENCES users(id) ON DELETE RESTRICT,
  note                     TEXT NULL,
  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL,
  deleted_at               TEXT NULL,
  version                  INTEGER NOT NULL DEFAULT 0,
  dirty                    INTEGER NOT NULL DEFAULT 1,
  last_synced_at           TEXT NULL,
  origin_device_id         TEXT NULL,
  entity_id                TEXT NOT NULL,
  CHECK (check_out_at IS NULL OR check_out_at >= check_in_at)
);

-- Partial unique index: only one open shift per operator (deleted rows are
-- never open, so the WHERE deleted_at IS NULL clause keeps it tight).
CREATE UNIQUE INDEX IF NOT EXISTS operator_shifts_open
  ON operator_shifts(operator_id)
  WHERE check_out_at IS NULL AND deleted_at IS NULL;

-- Index covering history_today() and operator-scoped history scans
-- (§7.2). The (entity_id, check_in_at) prefix is also useful for the
-- planned daily-close per-operator slice (phase-07).
CREATE INDEX IF NOT EXISTS operator_shifts_today
  ON operator_shifts(entity_id, check_in_at)
  WHERE deleted_at IS NULL;

-- Reverse index for FK joins from users/operators.
CREATE INDEX IF NOT EXISTS operator_shifts_by_operator
  ON operator_shifts(operator_id) WHERE deleted_at IS NULL;
