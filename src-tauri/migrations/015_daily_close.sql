-- Phase: signed & frozen daily close (the PRD §11.1 / phase-07 §7.12 "Horizon-1"
-- entity, now materialized). v1 derived the close on demand from `visits`; this
-- table makes a reconciled day PERMANENT and TAMPER-EVIDENT once an accountant
-- "signs and freezes" it.
--
-- Semantics:
--   * One frozen close per (entity_id, target_date) at a time. The accountant
--     can only freeze when there are zero pending-sync ops (no provisional
--     data), so the frozen snapshot is authoritative for that day.
--   * `input_hash` is the BLAKE3 freeze key (phase-07 §7.12): the fingerprint of
--     the exact inputs (sorted visit ids + settings snapshot + totals). The
--     frozen row stores the totals AND the hash, so any later recomputation can
--     be checked against the frozen snapshot for tamper detection.
--   * Once frozen, the day is IMMUTABLE: `VisitService::lock`/`::void` reject any
--     visit whose local-day is frozen. A superadmin may REOPEN (unfreeze) a close
--     to make a genuine correction -- that tombstones the row (`reopened_at`),
--     re-allows edits for the day, and is audited (`daily_close_reopen`). A new
--     freeze after a reopen inserts a fresh row.
--
-- Conflict policy: ADDITIVE-ONLY. A frozen close is written once and never
-- mutated in place (reopen sets the tombstone columns via a versioned LWW-style
-- update, but the freeze itself is INSERT OR IGNORE on the id). If two devices
-- freeze the same day while offline, the first to reach the server wins and the
-- second identical close is ignored -- no conflict, no data loss.
--
-- Sync columns follow the standard offline-first convention (offline-first.md):
-- id/created_at/updated_at/deleted_at/version/dirty/last_synced_at/
-- origin_device_id, plus entity_id for tenant scoping. `deleted_at` is unused for
-- this additive entity (a reopen uses `reopened_at`, not a tombstone, so the
-- historical record of the freeze survives), but is kept for schema uniformity.
--
-- Idempotency: the migration runner records applied files by name and runs each
-- exactly once inside a transaction.

CREATE TABLE IF NOT EXISTS daily_close (
  id                                     TEXT PRIMARY KEY,
  target_date                            TEXT NOT NULL,        -- YYYY-MM-DD local day
  tz_offset                              TEXT NOT NULL,        -- e.g. +03:00
  input_hash                             TEXT NOT NULL,        -- BLAKE3 freeze key

  -- materialized snapshot of the reconciliation at freeze time
  total_revenue_iqd                      INTEGER NOT NULL DEFAULT 0,
  total_doctor_cuts_iqd                  INTEGER NOT NULL DEFAULT 0,
  total_operator_cuts_iqd                INTEGER NOT NULL DEFAULT 0,
  total_inventory_consumption_value_iqd  INTEGER NOT NULL DEFAULT 0,
  net_iqd                                INTEGER NOT NULL DEFAULT 0,
  locked_count                           INTEGER NOT NULL DEFAULT 0,
  voided_count                           INTEGER NOT NULL DEFAULT 0,
  voided_value_iqd                       INTEGER NOT NULL DEFAULT 0,

  -- signer attestation
  signed_by_user_id                      TEXT NOT NULL,
  signed_by_name                         TEXT NOT NULL,        -- name snapshot at sign time
  signed_at                              TEXT NOT NULL,        -- RFC3339 UTC

  -- reopen (superadmin unfreeze) tombstone -- non-null means this close is no
  -- longer in force; the day is editable again until re-frozen.
  reopened_at                            TEXT NULL,
  reopened_by_user_id                    TEXT NULL,
  reopen_reason                          TEXT NULL,

  -- sync columns
  created_at                             TEXT NOT NULL,
  updated_at                             TEXT NOT NULL,
  deleted_at                             TEXT NULL,
  version                                INTEGER NOT NULL DEFAULT 0,
  dirty                                  INTEGER NOT NULL DEFAULT 1,
  last_synced_at                         TEXT NULL,
  origin_device_id                       TEXT NULL,
  entity_id                              TEXT NOT NULL
);

-- One IN-FORCE close per day: a partial unique index over not-yet-reopened,
-- not-deleted rows. A reopened day can be frozen again (new row, the old one has
-- reopened_at set so it falls out of this index).
CREATE UNIQUE INDEX IF NOT EXISTS daily_close_active_per_day
  ON daily_close(entity_id, target_date)
  WHERE reopened_at IS NULL AND deleted_at IS NULL;

-- Lookup by day (month overview, is-frozen checks) newest-first.
CREATE INDEX IF NOT EXISTS daily_close_by_date
  ON daily_close(entity_id, target_date DESC);
