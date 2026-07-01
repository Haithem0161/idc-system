-- Phase 12: مندوب (mandoub / representative) catalog entity.
--
-- A new syncable catalog entity modeled on `operators` (a simple name + phone +
-- notes + is_active record) but WITHOUT a stored cut: the مندوب's per-visit cut
-- (500 or 1000 IQD) is chosen on the visit, not on the مندوب row. The مندوب is
-- referenced by a visit only when a real referring doctor is selected, and its
-- cut is a net-side carve-out (deducted from clinic profit, never on the patient
-- bill, and it does not change the doctor cut or the report base).
--
-- Name is FTS-searchable (the receptionist picks a مندوب via a search+select
-- combobox), so this mirrors the `doctors_fts` pattern (migration 003), NOT the
-- plain-LIKE operators. Soft-deleted rows are removed from the FTS index.
--
-- Conflict policy: last-write-wins per row (same as operators / doctors).
-- Forward-only, idempotent.

CREATE TABLE IF NOT EXISTS mandoubs (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  phone             TEXT NULL,
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  notes             TEXT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL,
  CHECK (length(trim(name)) > 0)
);
CREATE INDEX IF NOT EXISTS mandoubs_name ON mandoubs(entity_id, name) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS mandoubs_active ON mandoubs(entity_id, is_active) WHERE deleted_at IS NULL;

-- FTS index over the name (mirrors doctors_fts). Triggers keep it in sync and
-- filter soft-deleted rows out.
CREATE VIRTUAL TABLE IF NOT EXISTS mandoubs_fts USING fts5(name, content='mandoubs', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS mandoubs_ai AFTER INSERT ON mandoubs WHEN new.deleted_at IS NULL BEGIN
  INSERT INTO mandoubs_fts(rowid, name) VALUES (new.rowid, new.name);
END;
CREATE TRIGGER IF NOT EXISTS mandoubs_ad AFTER DELETE ON mandoubs BEGIN
  INSERT INTO mandoubs_fts(mandoubs_fts, rowid, name) VALUES ('delete', old.rowid, old.name);
END;
CREATE TRIGGER IF NOT EXISTS mandoubs_au AFTER UPDATE ON mandoubs BEGIN
  INSERT INTO mandoubs_fts(mandoubs_fts, rowid, name) VALUES ('delete', old.rowid, old.name);
  INSERT INTO mandoubs_fts(rowid, name)
    SELECT new.rowid, new.name WHERE new.deleted_at IS NULL;
END;
