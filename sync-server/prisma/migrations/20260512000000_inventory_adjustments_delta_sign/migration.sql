-- Phase 6 §7.14 (Pass-3 GAP-C-4): server-side per-reason delta-sign CHECK on
-- `inventory_adjustments`. Mirrors the local SQLite constraint (phase-05 §1 +
-- the count_correction-nonzero trigger from phase-06/006_inventory_ops.sql).
--
-- Source-of-truth enforcement: the TypeBox validator + role gate in
-- `sync-server/src/app/sync/service/push-service.ts::validateAdjustment` runs
-- before this CHECK ever sees the row, but the CHECK is the defense-in-depth
-- backstop should a future writer bypass the service layer.
--
-- Prepared per the migration-ordering convention introduced by phase-05 §7.51:
-- raw-SQL files are lex-ordered alongside `prisma migrate dev` artifacts and
-- applied via `prisma migrate deploy` once the server moves off the
-- MemorySyncStore. Until then the validator above is the runtime enforcer.

ALTER TABLE inventory_adjustments
  ADD CONSTRAINT inventory_adjustments_delta_sign CHECK (
        (reason = 'receive'          AND delta > 0)
     OR (reason = 'writeoff'         AND delta < 0)
     OR (reason = 'count_correction' AND delta <> 0)
     OR (reason = 'consume_visit')
  );
