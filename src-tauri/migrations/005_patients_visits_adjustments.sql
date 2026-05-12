-- Phase 5 reception: patients (+ FTS5), visits, inventory_adjustments.
-- See docs/idc-system/phase-05.md §1 and gap fixes §7.1 / §7.2 / §7.5 / §7.8
-- / §7.17 / §7.33 / §7.41 / §7.53.

-- ---- patients (PRD §6.1.9) ------------------------------------------------
CREATE TABLE IF NOT EXISTS patients (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
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

CREATE INDEX IF NOT EXISTS patients_recent
  ON patients(entity_id, updated_at DESC)
  WHERE deleted_at IS NULL;

CREATE VIRTUAL TABLE IF NOT EXISTS patients_fts
  USING fts5(name, content='patients', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS patients_ai AFTER INSERT ON patients WHEN new.deleted_at IS NULL BEGIN
  INSERT INTO patients_fts(rowid, name) VALUES (new.rowid, new.name);
END;
CREATE TRIGGER IF NOT EXISTS patients_ad AFTER DELETE ON patients BEGIN
  INSERT INTO patients_fts(patients_fts, rowid, name) VALUES('delete', old.rowid, old.name);
END;
CREATE TRIGGER IF NOT EXISTS patients_au AFTER UPDATE ON patients BEGIN
  INSERT INTO patients_fts(patients_fts, rowid, name) VALUES('delete', old.rowid, old.name);
  INSERT INTO patients_fts(rowid, name)
    SELECT new.rowid, new.name WHERE new.deleted_at IS NULL;
END;

-- ---- visits (PRD §6.1.10) -------------------------------------------------
-- Status invariants enforced in a single state-conditional CHECK. The
-- locked clause also enforces the internal_pct <-> doctor_id iff (§7.1),
-- the total-equals-sum invariant (§7.2), and name-snapshot completeness
-- (§7.17 / §7.53).
CREATE TABLE IF NOT EXISTS visits (
  id                                TEXT PRIMARY KEY,
  patient_id                        TEXT NOT NULL REFERENCES patients(id),
  status                            TEXT NOT NULL CHECK (status IN ('draft','locked','voided')),
  receptionist_user_id              TEXT NOT NULL REFERENCES users(id),
  check_type_id                     TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id                  TEXT NULL REFERENCES check_subtypes(id),
  doctor_id                         TEXT NULL REFERENCES doctors(id),
  operator_id                       TEXT NULL REFERENCES operators(id),
  dye                               INTEGER NOT NULL DEFAULT 0 CHECK (dye IN (0,1)),
  report                            INTEGER NOT NULL DEFAULT 0 CHECK (report IN (0,1)),
  locked_at                         TEXT NULL,
  voided_at                         TEXT NULL,
  voided_by_user_id                 TEXT NULL REFERENCES users(id),
  void_reason                       TEXT NULL,
  price_snapshot_iqd                INTEGER NULL,
  dye_cost_snapshot_iqd             INTEGER NULL,
  report_cost_snapshot_iqd          INTEGER NULL,
  doctor_cut_snapshot_iqd           INTEGER NULL,
  operator_cut_snapshot_iqd         INTEGER NULL,
  internal_pct_snapshot             INTEGER NULL,
  total_amount_iqd_snapshot         INTEGER NULL,
  patient_name_snapshot             TEXT NULL,
  doctor_name_snapshot              TEXT NULL,
  operator_name_snapshot            TEXT NULL,
  check_type_name_ar_snapshot       TEXT NULL,
  check_type_name_en_snapshot       TEXT NULL,
  check_subtype_name_ar_snapshot    TEXT NULL,
  check_subtype_name_en_snapshot    TEXT NULL,
  created_at                        TEXT NOT NULL,
  updated_at                        TEXT NOT NULL,
  deleted_at                        TEXT NULL,
  version                           INTEGER NOT NULL DEFAULT 0,
  dirty                             INTEGER NOT NULL DEFAULT 1,
  last_synced_at                    TEXT NULL,
  origin_device_id                  TEXT NULL,
  entity_id                         TEXT NOT NULL,
  CHECK (
    (status = 'draft'  AND locked_at IS NULL AND voided_at IS NULL
                       AND price_snapshot_iqd IS NULL
                       AND total_amount_iqd_snapshot IS NULL)
    OR
    (status = 'locked' AND locked_at IS NOT NULL AND voided_at IS NULL
                       AND operator_id IS NOT NULL
                       AND price_snapshot_iqd IS NOT NULL
                       AND dye_cost_snapshot_iqd IS NOT NULL
                       AND report_cost_snapshot_iqd IS NOT NULL
                       AND doctor_cut_snapshot_iqd IS NOT NULL
                       AND operator_cut_snapshot_iqd IS NOT NULL
                       AND total_amount_iqd_snapshot IS NOT NULL
                       AND total_amount_iqd_snapshot = price_snapshot_iqd
                                                   + dye_cost_snapshot_iqd
                                                   + report_cost_snapshot_iqd
                       AND ((doctor_id IS NULL AND internal_pct_snapshot IS NOT NULL)
                         OR (doctor_id IS NOT NULL AND internal_pct_snapshot IS NULL))
                       AND patient_name_snapshot IS NOT NULL
                       AND check_type_name_ar_snapshot IS NOT NULL
                       AND operator_name_snapshot IS NOT NULL
                       AND ((doctor_id IS NULL AND doctor_name_snapshot IS NULL)
                         OR (doctor_id IS NOT NULL AND doctor_name_snapshot IS NOT NULL))
                       AND ((check_subtype_id IS NULL AND check_subtype_name_ar_snapshot IS NULL)
                         OR (check_subtype_id IS NOT NULL AND check_subtype_name_ar_snapshot IS NOT NULL)))
    OR
    (status = 'voided' AND locked_at IS NOT NULL AND voided_at IS NOT NULL
                       AND voided_by_user_id IS NOT NULL
                       AND void_reason IS NOT NULL
                       AND length(trim(void_reason)) >= 5)
  )
);
CREATE INDEX IF NOT EXISTS visits_status_date
  ON visits(entity_id, status, locked_at);
CREATE INDEX IF NOT EXISTS visits_check_type
  ON visits(entity_id, check_type_id, locked_at)
  WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS visits_doctor
  ON visits(entity_id, doctor_id, locked_at)
  WHERE deleted_at IS NULL AND doctor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS visits_operator
  ON visits(entity_id, operator_id, locked_at)
  WHERE deleted_at IS NULL AND operator_id IS NOT NULL;
-- §7.5: targeted index for draft workspace listing.
CREATE INDEX IF NOT EXISTS visits_drafts
  ON visits(entity_id, check_type_id, created_at DESC)
  WHERE status = 'draft' AND deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS visits_patient
  ON visits(patient_id)
  WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS visits_workspace_cursor
  ON visits(entity_id, check_type_id, created_at DESC, id DESC)
  WHERE deleted_at IS NULL;

-- ---- inventory_adjustments (PRD §6.1.14) -----------------------------------
CREATE TABLE IF NOT EXISTS inventory_adjustments (
  id                TEXT PRIMARY KEY,
  item_id           TEXT NOT NULL REFERENCES inventory_items(id),
  delta             INTEGER NOT NULL,
  reason            TEXT NOT NULL CHECK (reason IN ('receive','writeoff','count_correction','consume_visit')),
  visit_id          TEXT NULL REFERENCES visits(id),
  note              TEXT NULL,
  by_user_id        TEXT NOT NULL REFERENCES users(id),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL,
  CHECK (reason != 'consume_visit' OR visit_id IS NOT NULL),
  CHECK (reason != 'receive'           OR delta > 0),
  CHECK (reason != 'writeoff'          OR delta < 0)
);
CREATE INDEX IF NOT EXISTS inventory_adjustments_item
  ON inventory_adjustments(item_id, created_at)
  WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS inventory_adjustments_visit
  ON inventory_adjustments(visit_id)
  WHERE visit_id IS NOT NULL;
-- §7.41: totally-stable chrono scan (created_at, origin_device_id, id).
CREATE INDEX IF NOT EXISTS inventory_adjustments_chrono
  ON inventory_adjustments(entity_id, item_id, created_at, origin_device_id, id);

-- §7.33: immutability trigger. Updates that touch business columns abort.
-- Sync-metadata updates (version, dirty, last_synced_at, origin_device_id)
-- and tombstone soft-delete operations are intentionally NOT blocked here
-- because phase-05 §7.36 forbids deletion at the application layer and
-- the upsert path may stamp last_synced_at.
CREATE TRIGGER IF NOT EXISTS inventory_adjustments_no_update
BEFORE UPDATE ON inventory_adjustments
FOR EACH ROW
WHEN OLD.delta != NEW.delta
  OR OLD.reason != NEW.reason
  OR COALESCE(OLD.visit_id,'') != COALESCE(NEW.visit_id,'')
  OR OLD.item_id != NEW.item_id
  OR OLD.by_user_id != NEW.by_user_id
  OR (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'inventory_adjustments are append-only');
END;
