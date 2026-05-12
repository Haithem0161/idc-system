-- Phase 6 inventory operations.
-- See docs/idc-system/phase-06.md §1 and gap fixes §7.1 (count_correction != 0 CHECK),
-- §7.10 (low-stock / negative-stock partial indexes).

-- §7.1: the phase-05 migration already covers `receive > 0` and `writeoff < 0`
-- per-reason CHECKs but missed `count_correction != 0`. Re-creating the table
-- with CHECK additions is not idempotent on existing data, so we enforce the
-- missing case via a BEFORE INSERT trigger (the only path to insert is via
-- application code; the immutability trigger from phase-05 §7.33 already
-- blocks UPDATE on `delta`/`reason`, so we do not need to police updates).
CREATE TRIGGER IF NOT EXISTS inventory_adjustments_count_correction_nonzero
BEFORE INSERT ON inventory_adjustments
FOR EACH ROW
WHEN NEW.reason = 'count_correction' AND NEW.delta = 0
BEGIN
  SELECT RAISE(ABORT, 'count_correction adjustments must have non-zero delta');
END;

-- §7.10: partial indexes that serve the inventory list status filters.
-- low_stock_threshold is non-negative (catalog invariant from phase-03); the
-- "LOW" pill includes items at-or-below threshold but still non-negative,
-- and the "NEG" pill is a strict subset for items at or below zero.
CREATE INDEX IF NOT EXISTS inventory_items_low_stock
  ON inventory_items(entity_id)
  WHERE deleted_at IS NULL AND quantity_on_hand <= low_stock_threshold;

CREATE INDEX IF NOT EXISTS inventory_items_negative
  ON inventory_items(entity_id)
  WHERE deleted_at IS NULL AND quantity_on_hand < 0;
