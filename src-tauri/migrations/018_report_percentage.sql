-- Phase: report re-model -- "report" becomes a percentage carve-out paid to an
-- internal reporting doctor, deducted from the clinic's NET gain, instead of a
-- flat surcharge ADDED to the patient's bill.
--
-- OLD model (migrations 002 / 005):
--   * `settings.report_cost_iqd` was a flat global IQD amount.
--   * A visit's `report` flag added that flat cost to the patient total:
--       total = price + dye + report_cost          (DB CHECK, entity, server)
--   * The money silently became clinic revenue -- it INCREASED net.
--   * `check_types.report_supported` gated which checks could carry a report.
--
-- NEW model (this migration):
--   * `settings.report_pct` is a global percentage (0..=100).
--   * `settings.reporting_doctor_name` names the single internal reporting
--     doctor who receives every report amount (a label, not a doctors row).
--   * The patient total no longer includes report:
--       total = price + dye
--   * The report amount is computed AFTER the doctor cut, on the price basis,
--     excluding dye and the operator cut:
--       report_amount = report_pct * (price - doctor_cut) / 100   (when report on)
--   * That amount is OWED to the reporting doctor and SUBTRACTED from net:
--       net = collected - doctor_cut - operator_cut - report_amount
--   * `report_supported` is removed -- every check can carry a report.
--
-- Also adds a `dalal` mode to visits: a built-in doctor substitute ("دلال")
-- that takes a FLAT 10 IQD doctor cut with no referring-doctor row. It is a
-- third money mode distinct from house and doctor:
--   * house : doctor_id NULL, dalal 0 -> internal_pct set, doctor_cut = pct*price
--   * doctor: doctor_id set,  dalal 0 -> internal_pct NULL, doctor_cut from cut model
--   * dalal : doctor_id NULL, dalal 1 -> internal_pct NULL, doctor_cut = 10 (flat)
-- `dalal` is mutually exclusive with a referring doctor.
--
-- No production history exists yet (pre-launch), so this migration does NOT
-- backfill or recompute any locked visits; it only changes the schema and the
-- billed invariant going forward. The visits table is rebuilt to swap the
-- locked-state CHECK, which SQLite cannot alter in place.
--
-- Conflict policy: unchanged. `visits` stays manual/version-based; the new
-- report snapshots ride inside the same versioned visit snapshot. `settings`
-- stays last-write-wins per key.

-- ---- settings: drop flat report cost, add percentage + reporting doctor -----
-- Soft-delete the obsolete flat-cost key (tombstone so the change syncs) and
-- seed the two new keys. INSERT OR IGNORE keeps this idempotent across re-runs
-- and across devices that already pulled the new keys.
UPDATE settings
   SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
       version    = version + 1,
       dirty      = 1
 WHERE key = 'report_cost_iqd' AND deleted_at IS NULL;

INSERT OR IGNORE INTO settings (id, key, value, value_type, created_at, updated_at, version, dirty, entity_id) VALUES
  ('01000000-0000-7000-8000-000000000010','report_pct','20','int',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,1,'unscoped'),
  ('01000000-0000-7000-8000-000000000011','reporting_doctor_name','','text',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,1,'unscoped');

-- ---- check_types: drop the per-check report gate -----------------------------
-- Report is now universally available on every check type. SQLite 3.35+ DROP
-- COLUMN. No IF EXISTS guard is needed: the runner records each migration in
-- `_migrations` and never re-applies it, so this statement runs exactly once.
ALTER TABLE check_types DROP COLUMN report_supported;

-- ---- visits: rebuild to swap the locked-state CHECK and rename the report ---
-- ---- snapshot column from report_cost_snapshot_iqd -> report_amount_snapshot_iqd,
-- ---- and add report_pct_snapshot + reporting_doctor_name_snapshot. -----------
--
-- The runner wraps this whole file in one transaction. `PRAGMA foreign_keys`
-- is a no-op mid-transaction, so we DEFER the single inbound FK
-- (inventory_adjustments.visit_id) until COMMIT instead; ids are preserved by
-- the copy, so every child row re-validates cleanly at commit.
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
  -- دلال is a doctor substitute: it can never coexist with a referring doctor.
  -- Holds in every status (draft/locked/voided).
  CHECK (dalal = 0 OR doctor_id IS NULL),
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
                       -- Patient total no longer includes report; report is a
                       -- net-side carve-out paid to the reporting doctor.
                       AND total_amount_iqd_snapshot = price_snapshot_iqd
                                                   + dye_cost_snapshot_iqd
                       -- report flag <-> report snapshots coherence: when off,
                       -- the amount is 0 and the pct/name snapshots are absent;
                       -- when on, the amount is present (>= 0) and so is a pct.
                       AND ((report = 0 AND report_amount_snapshot_iqd = 0
                                        AND report_pct_snapshot IS NULL
                                        AND reporting_doctor_name_snapshot IS NULL)
                         OR (report = 1 AND report_pct_snapshot IS NOT NULL))
                       -- internal_pct marks HOUSE mode only: a real referring
                       -- doctor and دلال both leave it NULL; house sets it.
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

-- Copy every row across, TRANSFORMING already-locked/voided visits from the old
-- flat-surcharge model to the new carve-out model. Old locked snapshots were
-- frozen as `total = price + dye + report_cost`, which the new total CHECK
-- (`total = price + dye`) and the report-coherence CHECK reject. Drafts carry
-- all-NULL snapshots, so they are unaffected and copy through unchanged.
--
-- Per migrated locked/voided row:
--   total_amount_iqd_snapshot -> price + dye           (strip old report out of the patient total)
--   report_amount_snapshot_iqd -> old report_cost      (preserve the historical report value as the carve-out; 0 when report off)
--   report_pct_snapshot       -> 0 when report on, NULL when off
--                                (the legacy amount was a flat fee with no percentage basis to recover;
--                                 0 is a valid 0..=100 pct that satisfies the report=1 -> pct NOT NULL coherence rule.
--                                 New locks compute a real pct going forward.)
--   reporting_doctor_name_snapshot -> NULL             (no reporting doctor was recorded under the old model)
--   dalal -> 0                                         (the dalal mode did not exist before this migration)
-- The doctor/operator cuts, internal_pct, and all name snapshots are unchanged,
-- so the house/doctor/internal_pct CHECK clauses keep holding as before.
INSERT INTO visits_new (
  id, patient_id, status, receptionist_user_id, check_type_id, check_subtype_id,
  doctor_id, operator_id, dye, report, dalal, locked_at, voided_at, voided_by_user_id,
  void_reason, price_snapshot_iqd, dye_cost_snapshot_iqd,
  report_amount_snapshot_iqd, report_pct_snapshot, reporting_doctor_name_snapshot,
  doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, internal_pct_snapshot,
  total_amount_iqd_snapshot, amount_paid_override_iqd, patient_name_snapshot,
  doctor_name_snapshot, operator_name_snapshot, check_type_name_ar_snapshot,
  check_type_name_en_snapshot, check_subtype_name_ar_snapshot,
  check_subtype_name_en_snapshot, created_at, updated_at, deleted_at, version,
  dirty, last_synced_at, origin_device_id, entity_id
)
SELECT
  id, patient_id, status, receptionist_user_id, check_type_id, check_subtype_id,
  doctor_id, operator_id, dye, report, 0, locked_at, voided_at, voided_by_user_id,
  void_reason, price_snapshot_iqd, dye_cost_snapshot_iqd,
  -- report_amount: 0 when report off, else the old flat report_cost (NULL-safe).
  CASE WHEN price_snapshot_iqd IS NULL THEN NULL
       WHEN report = 1 THEN COALESCE(report_cost_snapshot_iqd, 0)
       ELSE 0 END,
  -- report_pct: NULL when report off (or draft), 0 for migrated report-on rows.
  CASE WHEN price_snapshot_iqd IS NULL THEN NULL
       WHEN report = 1 THEN 0
       ELSE NULL END,
  NULL,
  doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, internal_pct_snapshot,
  -- total: drafts keep NULL; locked/voided rebase to price + dye (no report).
  CASE WHEN price_snapshot_iqd IS NULL THEN NULL
       ELSE price_snapshot_iqd + COALESCE(dye_cost_snapshot_iqd, 0) END,
  amount_paid_override_iqd, patient_name_snapshot,
  doctor_name_snapshot, operator_name_snapshot, check_type_name_ar_snapshot,
  check_type_name_en_snapshot, check_subtype_name_ar_snapshot,
  check_subtype_name_en_snapshot, created_at, updated_at, deleted_at, version,
  dirty, last_synced_at, origin_device_id, entity_id
FROM visits;

DROP TABLE visits;

ALTER TABLE visits_new RENAME TO visits;

-- Recreate every index that lived on the old visits table (005 + 007). Indexes
-- are dropped with the old table, so they must be rebuilt here verbatim.
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

-- 007_reports.sql covering indexes (locked/voided by date, per-doctor/operator/
-- check-type aggregates).
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
