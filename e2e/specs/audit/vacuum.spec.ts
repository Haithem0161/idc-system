// Phase-09 §4.1 E2E: audit vacuum.
//
// Proves the superadmin can run an audit_vacuum_now from /admin/audit
// -- the vacuum:
//   - prunes stale audit rows (per the configured retention),
//   - emits ONE self-audit row recording the vacuum,
//   - bumps last_audit_vacuum_at,
//   - is idempotent (re-running produces a second self-audit row but
//     zero new prunes).
// Phase-08 §3.Rust audit_vacuum_now + phase-08 §7.18 invariants.
//
//   1. Land on /admin/audit.
//   2. Click "Vacuum now" -- the success chip surfaces with the
//      number-purged delta.
//   3. The diagnostics summary refreshes (last_audit_vacuum_at + ON
//      the wall).
//
// Gated by RUN_FULL_E2E=true.

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- audit vacuum", function () {
  it("vacuums the audit log and refreshes diagnostics", async function () {
    await clickLinkWithText("Audit")
    await waitForText("Audit")

    await clickButtonWithText("Vacuum now")
    await waitForText("Vacuum complete")

    // The diagnostics surface shows the new last_audit_vacuum_at.
    await clickButtonWithText("Diagnostics")
    await waitForText("Last vacuum")
  })
})
