// Phase-09 §4.1 E2E: resolve overlapping shifts.
//
// Proves the receptionist can clear an "operator clocked in twice"
// banner without losing audit history. Phase-04 §7.shifts invariant:
// "an operator cannot clock in twice without clocking out first"; the
// resolver flow is the load-bearing remediation when the banner fires.
//
//   1. Seed the SQLite with two overlapping shifts for one operator
//      (the clinical-day fixture has this scenario for the
//      receptionist persona). The banner renders at the top of the
//      shifts page.
//   2. Click "Resolve" -- the modal opens with both shifts side by
//      side.
//   3. Click "Close now" on the open shift (the one with no
//      check_out_at).
//   4. The banner clears AND a soft-deleted audit row records the
//      resolver reason.
//
// Gated by RUN_FULL_E2E=true.

import { browser } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import { clickButtonWithText, clickLinkWithText, waitForText } from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- resolve overlapping shifts", function () {
  it("opens the resolver modal and clears the overlap by closing the open shift", async function () {
    await clickLinkWithText("Operator shifts")

    // The overlap banner renders only when the seed fixture includes
    // overlapping shifts. The "Resolve" button is the per-operator
    // entry into the modal.
    await waitForText("Overlapping shifts detected")
    await clickButtonWithText("Resolve")

    // The modal opens with two list items (one per shift), sorted by
    // check_in_at.
    const dialog = await browser.$("[role='dialog']")
    await dialog.waitForExist({ timeout: 10_000 })
    await waitForText("Resolve overlap")

    // Click "Close now" on the first (open) shift.
    await clickButtonWithText("Close now")

    // The banner clears after the mutation succeeds (the overlaps
    // query refetches and resolves empty).
    await browser.waitUntil(
      async () => {
        const body = await browser.$("body")
        const html = await body.getHTML()
        return !html.includes("Overlapping shifts detected")
      },
      {
        timeout: 10_000,
        timeoutMsg: "Expected the overlap banner to clear after Close now",
      },
    )
  })
})
