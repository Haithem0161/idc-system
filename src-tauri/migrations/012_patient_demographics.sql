-- Phase: patient archive. Extend the identity-only patients table with
-- OPTIONAL demographics so the archive can disambiguate same-named patients,
-- search/dedupe by phone, and show age. All columns are nullable; the
-- new-visit flow stays name-only and leaves these NULL.
--
-- Idempotency: the migration runner records applied files by name in
-- `_migrations` and runs each file exactly once, inside a transaction. SQLite
-- `ALTER TABLE ADD COLUMN` has no IF NOT EXISTS, but the name-guard guarantees
-- this file never re-runs, so plain ADD COLUMN is safe.
--
-- `sex` is constrained to ('M','F') in the Rust entity (update_demographics),
-- not via a DB CHECK -- SQLite cannot add a CHECK constraint to an existing
-- table through ALTER.

ALTER TABLE patients ADD COLUMN phone TEXT NULL;
ALTER TABLE patients ADD COLUMN sex TEXT NULL;
ALTER TABLE patients ADD COLUMN birth_date TEXT NULL;
ALTER TABLE patients ADD COLUMN file_no TEXT NULL;
ALTER TABLE patients ADD COLUMN notes TEXT NULL;

-- Exact-match phone lookup for duplicate detection (normalized in Rust before
-- comparison). Partial index keeps it small: only live rows with a phone.
CREATE INDEX IF NOT EXISTS patients_phone
  ON patients(entity_id, phone)
  WHERE phone IS NOT NULL AND deleted_at IS NULL;
