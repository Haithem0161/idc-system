-- IDC sync-server: raw-SQL backstop applied after `prisma db push`.
-- Phase-09 §2: collects constraints that Prisma's declarative schema cannot
-- express. Each statement is idempotent so the Dockerfile.dev start hook can
-- re-run it on every container boot without side effects.

-- Phase-03 §7.20: DoctorCheckPricing paired partial unique index.
-- One pricing row per (doctor, check_type, check_subtype) tuple — including
-- the NULL-subtype case which Postgres treats as distinct under a plain
-- unique. Two partial indexes cover both arms.
DROP INDEX IF EXISTS doctor_check_pricing_uniq_with_subtype;
CREATE UNIQUE INDEX doctor_check_pricing_uniq_with_subtype
  ON doctor_check_pricing (doctor_id, check_type_id, check_subtype_id)
  WHERE check_subtype_id IS NOT NULL AND deleted_at IS NULL;

DROP INDEX IF EXISTS doctor_check_pricing_uniq_without_subtype;
CREATE UNIQUE INDEX doctor_check_pricing_uniq_without_subtype
  ON doctor_check_pricing (doctor_id, check_type_id)
  WHERE check_subtype_id IS NULL AND deleted_at IS NULL;

-- Phase-03 §7.21: InventoryConsumptionMap paired partial unique index.
-- Mirror of the doctor pricing rule for (item, check_type, check_subtype).
DROP INDEX IF EXISTS inventory_consumption_map_uniq_with_subtype;
CREATE UNIQUE INDEX inventory_consumption_map_uniq_with_subtype
  ON inventory_consumption_map (item_id, check_type_id, check_subtype_id)
  WHERE check_subtype_id IS NOT NULL AND deleted_at IS NULL;

DROP INDEX IF EXISTS inventory_consumption_map_uniq_without_subtype;
CREATE UNIQUE INDEX inventory_consumption_map_uniq_without_subtype
  ON inventory_consumption_map (item_id, check_type_id)
  WHERE check_subtype_id IS NULL AND deleted_at IS NULL;

-- Phase-05 §7.33: inventory_adjustments BEFORE UPDATE trigger.
-- Adjustments are append-only. The push-service validator enforces this at
-- the API boundary; the trigger is defense-in-depth.
CREATE OR REPLACE FUNCTION inventory_adjustments_block_update()
RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'inventory_adjustments are append-only; tried to update row %', OLD.id
    USING ERRCODE = 'check_violation';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS inventory_adjustments_no_update ON inventory_adjustments;
CREATE TRIGGER inventory_adjustments_no_update
  BEFORE UPDATE ON inventory_adjustments
  FOR EACH ROW
  EXECUTE FUNCTION inventory_adjustments_block_update();

-- Phase-06 §7.14: per-reason delta-sign CHECK on inventory_adjustments.
-- Matches the prepared migration at
-- prisma/migrations/20260512000000_inventory_adjustments_delta_sign/.
ALTER TABLE inventory_adjustments
  DROP CONSTRAINT IF EXISTS inventory_adjustments_delta_sign;
ALTER TABLE inventory_adjustments
  ADD CONSTRAINT inventory_adjustments_delta_sign CHECK (
        (reason = 'receive'          AND delta > 0)
     OR (reason = 'writeoff'         AND delta < 0)
     OR (reason = 'count_correction' AND delta <> 0)
     OR (reason = 'consume_visit')
  );

-- Daily close: in-force partial-unique index. Mirrors the desktop SQLite
-- partial index `daily_close_active_per_day` (client migration 015). At most
-- one IN-FORCE close per (entity_id, target_date): a not-yet-reopened,
-- not-deleted row. A reopened day falls out of the index (reopened_at set), so
-- it can be frozen again as a fresh row -- which is exactly why a plain
-- `@@unique([entityId, targetDate])` is WRONG here (it would block the
-- legitimate reopen-then-refreeze flow). Conflict policy stays LWW-by-id; this
-- index only backstops the business uniqueness rule as defense-in-depth.
DROP INDEX IF EXISTS daily_close_active_per_day;
CREATE UNIQUE INDEX daily_close_active_per_day
  ON daily_close (entity_id, target_date)
  WHERE reopened_at IS NULL AND deleted_at IS NULL;

-- Phase-05 §7.53: visits CHECK extension for the 7 name-snapshot columns.
-- A locked visit must carry all required snapshots; a draft must not. The
-- service layer (push-service.validateVisit) enforces this at the API
-- boundary; this CHECK is defense-in-depth for any future writer.
ALTER TABLE visits
  DROP CONSTRAINT IF EXISTS visits_locked_name_snapshots;
ALTER TABLE visits
  ADD CONSTRAINT visits_locked_name_snapshots CHECK (
    status <> 'locked'
    OR (
      patient_name_snapshot IS NOT NULL
      AND operator_name_snapshot IS NOT NULL
      AND check_type_name_ar_snapshot IS NOT NULL
    )
  );
