-- Phase 9: Pre-Ship Hardening.
--
-- DEF-008 fix: `doctors_au` AFTER UPDATE trigger from migration 003 issued
-- an unconditional FTS delete on update. When a soft-deleted doctor was
-- restored (`deleted_at: NOT NULL -> NULL`), the trigger tried to delete a
-- row that had never been indexed (because the original insert path skips
-- FTS for soft-deleted rows), which raises
-- `database disk image is malformed` on external-content FTS5 indexes.
--
-- The fix guards the FTS delete with a `WHERE old.deleted_at IS NULL`
-- predicate so it only fires when the row WAS indexed. Mirrors the insert
-- arm's `WHERE new.deleted_at IS NULL` predicate that already existed.
--
-- Idempotent: DROP TRIGGER IF EXISTS lets the migration replay safely on
-- a populated DB.

DROP TRIGGER IF EXISTS doctors_au;

CREATE TRIGGER doctors_au AFTER UPDATE ON doctors BEGIN
  INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty)
    SELECT 'delete', old.rowid, old.name, COALESCE(old.specialty, '')
    WHERE old.deleted_at IS NULL;
  INSERT INTO doctors_fts(rowid, name, specialty)
    SELECT new.rowid, new.name, COALESCE(new.specialty, '')
    WHERE new.deleted_at IS NULL;
END;
