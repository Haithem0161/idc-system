-- Phase: collected-cash reconciliation on the frozen daily close.
--
-- The receptionist price override (migration 016) lets a visit be collected for
-- less than its billed total. The daily close must therefore report two more
-- materialized totals so a SIGNED/FROZEN close stays self-contained and
-- tamper-evident even after the override semantics existed:
--
--   total_collected_iqd  -- SUM over locked visits of the cash actually taken:
--                           COALESCE(amount_paid_override_iqd, total_amount_iqd_snapshot)
--   total_discount_iqd   -- total_revenue_iqd (billed) - total_collected_iqd.
--                           >= 0 in normal use; stored explicitly so the frozen
--                           snapshot does not have to be recomputed to show it.
--
-- `net_iqd` already tracks collected cash from this phase on (collected minus
-- doctor cuts, operator cuts, inventory value), so the frozen `net_iqd` column
-- needs no schema change -- only its computed value moves to a collected basis.
--
-- Forward-only, idempotent: NOT NULL DEFAULT 0 so pre-existing frozen closes
-- (written before overrides existed, where collected == billed) read back a
-- discount of 0 and a collected equal to revenue once recomputed; the stored
-- defaults are a safe, neutral baseline for any legacy row.

ALTER TABLE daily_close ADD COLUMN total_collected_iqd INTEGER NOT NULL DEFAULT 0;
ALTER TABLE daily_close ADD COLUMN total_discount_iqd INTEGER NOT NULL DEFAULT 0;
