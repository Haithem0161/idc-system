-- Per-check-type and per-subtype dye price replaces the global dye_cost_iqd
-- setting and the check_types.dye_supported flag.
--
-- dye_price_iqd is nullable: NULL = dye not offered here; a value (incl. 0 =
-- free dye) = dye available at that price. Resolution mirrors base_price:
-- subtype's value when the check type has subtypes, else the check type's.
--
-- No backfill: every dye_price_iqd starts NULL, so dye is off clinic-wide until
-- the accountant configures each price. Pre-launch, no dye history to preserve.
--
-- SQLite 3.35+ ALTER ADD/DROP COLUMN (see migration 018). No table rebuild.
-- Forward-only, idempotent within the migration runner (runs exactly once).

ALTER TABLE check_types ADD COLUMN dye_price_iqd INTEGER NULL
  CHECK (dye_price_iqd IS NULL OR dye_price_iqd >= 0);

ALTER TABLE check_subtypes ADD COLUMN dye_price_iqd INTEGER NULL
  CHECK (dye_price_iqd IS NULL OR dye_price_iqd >= 0);

ALTER TABLE check_types DROP COLUMN dye_supported;

-- Retire the obsolete global dye price setting (tombstone so it syncs and
-- stops appearing in the settings UI/cache). Mirrors migration 018's
-- report_cost_iqd tombstone.
UPDATE settings
   SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       version    = version + 1,
       dirty      = 1
 WHERE key = 'dye_cost_iqd' AND deleted_at IS NULL;
