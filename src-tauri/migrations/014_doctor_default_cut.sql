-- Phase: doctor archive / inline-create. Give every referring doctor an
-- OPTIONAL default cut so a doctor with no per-check DoctorCheckPricing row
-- still earns their negotiated share instead of silently falling to 0.
--
-- Resolution order (money engine `cuts()`):
--   per-check DoctorCheckPricing override -> use it
--   else doctor default cut               -> use it   (NEW)
--   else                                  -> 0
--
-- Both columns are nullable. A doctor with no default behaves exactly as
-- before (cut = 0 when no per-check row), so this is backward-compatible;
-- only future locks of doctors that GET a default change.
--
-- Idempotency: the migration runner records applied files by name in
-- `_migrations` and runs each file exactly once, inside a transaction. SQLite
-- `ALTER TABLE ADD COLUMN` has no IF NOT EXISTS, but the name-guard guarantees
-- this file never re-runs, so plain ADD COLUMN is safe.
--
-- `default_cut_kind` is constrained to ('pct','fixed') in the Rust entity,
-- not via a DB CHECK -- SQLite cannot add a CHECK to an existing table through
-- ALTER. `pct` values are 0..=100; `fixed` values are >= 0 (IQD). Both halves
-- are written together: a non-null kind implies a non-null value and vice
-- versa (enforced in the entity).

ALTER TABLE doctors ADD COLUMN default_cut_kind TEXT NULL;
ALTER TABLE doctors ADD COLUMN default_cut_value INTEGER NULL;
