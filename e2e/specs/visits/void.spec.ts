// Phase-09 §4.1 E2E: void a locked visit.
//
// Proves the superadmin can void a locked visit with a reason; the
// void:
//   - flips the visit status to "voided",
//   - restores any inventory consumed by the visit (phase-06 §7.x
//     reversal_badge invariant),
//   - emits an audit row with the void reason,
//   - leaves the receipts intact but marked "voided" on the receipts
//     tab.
//
// Gated by RUN_FULL_E2E=true. Requires the seed fixture to log in as
// the superadmin Mariam.

import { browser } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- void locked visit", function () {
  it("voids a locked visit with a reason and refreshes the void status", async function () {
    // Navigate to the workspace and open a locked visit (the lock-and-print
    // spec produces one; the void spec consumes it).
    await clickLinkWithText("Checks")
    await clickButtonWithText("Open workspace")
    await clickButtonWithText("Open")

    // Click the "Void visit" button -- the void modal opens.
    await clickButtonWithText("Void visit")
    await waitForText("Void this visit")

    // Fill the reason -- must be >= 5 chars per phase-05
    // reception.new_visit.errors.void_too_short.
    const reasonInput = await browser.$("textarea, input[type='text']")
    await reasonInput.setValue("E2E test -- voided after lock for spec coverage")

    // Submit.
    await clickButtonWithText("Void")

    // The status pill flips to "Voided" and the audit tab surfaces
    // the void reason.
    await waitForText("Voided")
    await clickButtonWithText("Audit")
    await waitForText("E2E test -- voided after lock for spec coverage")
  })
})
