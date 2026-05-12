-- Phase 7 accounting & reports.
-- See docs/idc-system/phase-07.md §1 / §5: no new tables; this migration adds
-- covering indexes that the report queries rely on for predictable latency on
-- the v1 row volumes (PRD §11 budget: 10k visits/year * 5 years).

-- Locked-visits-by-date scan (Dashboard KPIs + Visits Report + Daily Close +
-- Doctor / Operator earnings all gate on locked_at within a date range).
CREATE INDEX IF NOT EXISTS visits_locked_at_idx
  ON visits(entity_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';

-- Voided-by-date scan (Daily Close voided count + value; voided rows persist
-- the snapshots but transition out of `locked`).
CREATE INDEX IF NOT EXISTS visits_voided_at_idx
  ON visits(entity_id, voided_at)
  WHERE deleted_at IS NULL AND status = 'voided';

-- Per-doctor aggregate (DoctorEarnings; partial index on locked rows only).
CREATE INDEX IF NOT EXISTS visits_locked_doctor_idx
  ON visits(entity_id, doctor_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';

-- Per-operator aggregate (OperatorEarnings + lines-run-today join target).
CREATE INDEX IF NOT EXISTS visits_locked_operator_idx
  ON visits(entity_id, operator_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';

-- Per-check-type aggregate (Daily Close per-check breakdown + Top Check Types).
CREATE INDEX IF NOT EXISTS visits_locked_check_type_idx
  ON visits(entity_id, check_type_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';

-- consume_visit consumption value aggregation (Daily Close inventory value).
CREATE INDEX IF NOT EXISTS inventory_adjustments_consume_idx
  ON inventory_adjustments(entity_id, created_at)
  WHERE deleted_at IS NULL AND reason = 'consume_visit';

-- Operator-shifts windowed scan (Operator drilldown shifts list + hours
-- aggregation). check_in_at is non-null on every row by construction.
CREATE INDEX IF NOT EXISTS operator_shifts_window_idx
  ON operator_shifts(entity_id, operator_id, check_in_at)
  WHERE deleted_at IS NULL;
