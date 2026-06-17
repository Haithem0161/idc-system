-- Fix the patients_fts UPDATE trigger so restoring a soft-deleted patient does
-- not corrupt the FTS index.
--
-- The original `patients_au` (migration 005) always issued an FTS5 'delete'
-- for the OLD row, then re-inserted when `new.deleted_at IS NULL`. But a
-- soft-delete already removed the row from the index (its 'delete' ran with
-- old.deleted_at NULL, no re-insert). Restoring then fired a second 'delete'
-- for a rowid no longer present in the external-content FTS index, which
-- raises "database disk image is malformed".
--
-- The corrected trigger only deletes from FTS when the OLD row was actually
-- indexed (old.deleted_at IS NULL), and only inserts when the NEW row should
-- be indexed (new.deleted_at IS NULL). This makes every transition correct:
--   live   -> live    : delete old, insert new   (rename)
--   live   -> deleted : delete old               (soft-delete)
--   deleted-> live    : insert new               (restore)
--   deleted-> deleted : no-op

DROP TRIGGER IF EXISTS patients_au;

CREATE TRIGGER patients_au AFTER UPDATE ON patients BEGIN
  INSERT INTO patients_fts(patients_fts, rowid, name)
    SELECT 'delete', old.rowid, old.name WHERE old.deleted_at IS NULL;
  INSERT INTO patients_fts(rowid, name)
    SELECT new.rowid, new.name WHERE new.deleted_at IS NULL;
END;
