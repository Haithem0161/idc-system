-- Phase 12: wire مندوب (representative) into visits + the daily close.
--
-- A visit may reference a مندوب ONLY when a real referring doctor is selected
-- (not house, not دلال). The مندوب carries a per-visit cut of 500 or 1000 IQD,
-- snapshotted at lock. The cut is a NET-SIDE carve-out (subtracted from clinic
-- net after the report; never on the patient bill; does not change the doctor
-- cut or the report base).
--
-- SQLite cannot alter a table CHECK in place, and migration 018 already rebuilt
-- `visits` via a table swap, so this migration repeats the same swap: it copies
-- the post-018 column set verbatim, adds three مندوب columns, extends the locked
-- CHECK, copies all rows (legacy rows get NULL مندوب), and recreates every
-- index. Existing locked/voided visits had no مندوب, so NULL is the correct
-- historical value and the new CHECK accepts them unchanged.
--
-- Also adds the daily-close مندوب payable column (additive, no rebuild needed).
-- Conflict policy: unchanged (visits manual/version-based; the مندوب snapshots
-- ride inside the same versioned visit snapshot).

-- ---- daily_close: مندوب payable line (mirror total_operator_cuts_iqd) --------
ALTER TABLE daily_close ADD COLUMN total_mandoub_cuts_iqd INTEGER NOT NULL DEFAULT 0;

-- ---- visits rebuild ----------------------------------------------------------
PRAGMA defer_foreign_keys = ON;

CREATE TABLE visits_new (
  id                                TEXT PRIMARY KEY,
  patient_id                        TEXT NOT NULL REFERENCES patients(id),
  status                            TEXT NOT NULL CHECK (status IN ('draft','locked','voided')),
  receptionist_user_id              TEXT NOT NULL REFERENCES users(id),
  check_type_id                     TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id                  TEXT NULL REFERENCES check_subtypes(id),
  doctor_id                         TEXT NULL REFERENCES doctors(id),
  operator_id                       TEXT NULL REFERENCES operators(id),
  mandoub_id                        TEXT NULL REFERENCES mandoubs(id),
  dye                               INTEGER NOT NULL DEFAULT 0 CHECK (dye IN (0,1)),
  report                            INTEGER NOT NULL DEFAULT 0 CHECK (report IN (0,1)),
  dalal                             INTEGER NOT NULL DEFAULT 0 CHECK (dalal IN (0,1)),
  locked_at                         TEXT NULL,
  voided_at                         TEXT NULL,
  voided_by_user_id                 TEXT NULL REFERENCES users(id),
  void_reason                       TEXT NULL,
  price_snapshot_iqd                INTEGER NULL,
  dye_cost_snapshot_iqd             INTEGER NULL,
  report_amount_snapshot_iqd        INTEGER NULL,
  report_pct_snapshot               INTEGER NULL,
  reporting_doctor_name_snapshot    TEXT NULL,
  doctor_cut_snapshot_iqd           INTEGER NULL,
  operator_cut_snapshot_iqd         INTEGER NULL,
  mandoub_cut_snapshot_iqd          INTEGER NULL,
  mandoub_name_snapshot             TEXT NULL,
  internal_pct_snapshot             INTEGER NULL,
  total_amount_iqd_snapshot         INTEGER NULL,
  amount_paid_override_iqd          INTEGER NULL,
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
  CHECK (dalal = 0 OR doctor_id IS NULL),
  -- مندوب requires a real referring doctor (the opposite polarity of dalal).
  -- Holds in every status.
  CHECK (mandoub_id IS NULL OR doctor_id IS NOT NULL),
  CHECK (
    (status = 'draft'  AND locked_at IS NULL AND voided_at IS NULL
                       AND price_snapshot_iqd IS NULL
                       AND total_amount_iqd_snapshot IS NULL)
    OR
    (status = 'locked' AND locked_at IS NOT NULL AND voided_at IS NULL
                       AND operator_id IS NOT NULL
                       AND price_snapshot_iqd IS NOT NULL
                       AND dye_cost_snapshot_iqd IS NOT NULL
                       AND report_amount_snapshot_iqd IS NOT NULL
                       AND doctor_cut_snapshot_iqd IS NOT NULL
                       AND operator_cut_snapshot_iqd IS NOT NULL
                       AND total_amount_iqd_snapshot IS NOT NULL
                       AND total_amount_iqd_snapshot = price_snapshot_iqd
                                                   + dye_cost_snapshot_iqd
                       AND ((report = 0 AND report_amount_snapshot_iqd = 0
                                        AND report_pct_snapshot IS NULL
                                        AND reporting_doctor_name_snapshot IS NULL)
                         OR (report = 1 AND report_pct_snapshot IS NOT NULL))
                       -- مندوب coherence: when set, a real doctor is present and
                       -- the cut is 500 or 1000 with the name snapshot captured;
                       -- when absent, both مندوب snapshots are NULL.
                       AND ((mandoub_id IS NOT NULL AND doctor_id IS NOT NULL
                                        AND mandoub_cut_snapshot_iqd IN (500, 1000)
                                        AND mandoub_name_snapshot IS NOT NULL)
                         OR (mandoub_id IS NULL AND mandoub_cut_snapshot_iqd IS NULL
                                        AND mandoub_name_snapshot IS NULL))
                       AND ((doctor_id IS NULL AND dalal = 0 AND internal_pct_snapshot IS NOT NULL)
                         OR (doctor_id IS NOT NULL AND internal_pct_snapshot IS NULL)
                         OR (dalal = 1 AND internal_pct_snapshot IS NULL))
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

-- Copy all rows. Legacy rows had no مندوب, so the three new columns are NULL --
-- the correct historical value, which the new CHECK accepts (mandoub_id NULL).
INSERT INTO visits_new (
  id, patient_id, status, receptionist_user_id, check_type_id, check_subtype_id,
  doctor_id, operator_id, mandoub_id, dye, report, dalal, locked_at, voided_at,
  voided_by_user_id, void_reason, price_snapshot_iqd, dye_cost_snapshot_iqd,
  report_amount_snapshot_iqd, report_pct_snapshot, reporting_doctor_name_snapshot,
  doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, mandoub_cut_snapshot_iqd,
  mandoub_name_snapshot, internal_pct_snapshot, total_amount_iqd_snapshot,
  amount_paid_override_iqd, patient_name_snapshot, doctor_name_snapshot,
  operator_name_snapshot, check_type_name_ar_snapshot, check_type_name_en_snapshot,
  check_subtype_name_ar_snapshot, check_subtype_name_en_snapshot, created_at,
  updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id
)
SELECT
  id, patient_id, status, receptionist_user_id, check_type_id, check_subtype_id,
  doctor_id, operator_id, NULL, dye, report, dalal, locked_at, voided_at,
  voided_by_user_id, void_reason, price_snapshot_iqd, dye_cost_snapshot_iqd,
  report_amount_snapshot_iqd, report_pct_snapshot, reporting_doctor_name_snapshot,
  doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, NULL,
  NULL, internal_pct_snapshot, total_amount_iqd_snapshot,
  amount_paid_override_iqd, patient_name_snapshot, doctor_name_snapshot,
  operator_name_snapshot, check_type_name_ar_snapshot, check_type_name_en_snapshot,
  check_subtype_name_ar_snapshot, check_subtype_name_en_snapshot, created_at,
  updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id
FROM visits;

DROP TABLE visits;

ALTER TABLE visits_new RENAME TO visits;

-- Recreate every index (005 + 007), verbatim, plus a مندوب earnings index.
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
CREATE INDEX IF NOT EXISTS visits_drafts
  ON visits(entity_id, check_type_id, created_at DESC)
  WHERE status = 'draft' AND deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS visits_patient
  ON visits(patient_id)
  WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS visits_workspace_cursor
  ON visits(entity_id, check_type_id, created_at DESC, id DESC)
  WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS visits_locked_at_idx
  ON visits(entity_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';
CREATE INDEX IF NOT EXISTS visits_voided_at_idx
  ON visits(entity_id, voided_at)
  WHERE deleted_at IS NULL AND status = 'voided';
CREATE INDEX IF NOT EXISTS visits_locked_doctor_idx
  ON visits(entity_id, doctor_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';
CREATE INDEX IF NOT EXISTS visits_locked_operator_idx
  ON visits(entity_id, operator_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';
CREATE INDEX IF NOT EXISTS visits_locked_check_type_idx
  ON visits(entity_id, check_type_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked';
-- New: per-مندوب earnings aggregate (locked rows with a مندوب).
CREATE INDEX IF NOT EXISTS visits_locked_mandoub_idx
  ON visits(entity_id, mandoub_id, locked_at)
  WHERE deleted_at IS NULL AND status = 'locked' AND mandoub_id IS NOT NULL;
