-- Phase 11 follow-up: persist the report payable on a signed/frozen daily close.
--
-- Migration 018 re-modeled "report" as a percentage carve-out paid to an
-- internal reporting doctor, SUBTRACTED from net. The in-memory daily close and
-- the live accounting screens surface that report total, but the FROZEN
-- (signed) `daily_close` row stored only the final `net_iqd` and the doctor /
-- operator cut totals -- not the report component. That made the reporting-
-- doctor payable un-reconstructable from a reopened/historical frozen close.
--
-- This adds the missing column so a frozen close itemizes the report payable
-- exactly like doctor cuts and operator cuts. `net_iqd` is unchanged (it
-- already nets out report); this column is purely the additive breakdown line.
--
-- Forward-only, idempotent: nullable-by-default-zero add, no backfill. Existing
-- frozen rows (pre-launch, none in production) keep 0, which is the correct
-- historical value for any close signed before report had a percentage.

ALTER TABLE daily_close ADD COLUMN total_report_iqd INTEGER NOT NULL DEFAULT 0;
