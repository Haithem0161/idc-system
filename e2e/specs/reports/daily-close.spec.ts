// Phase-09 §4.1 E2E: daily close.
//
// Proves the accountant can close a day from /accounting/daily-close.
// The closed-day row flips status from "Open / needs close" to
// "Closed" with a PDF artifact. Phase-07 §3.Frontend DailyClose +
// phase-07 §7.5 daily_close_run audit row.
//
//   1. Land on /accounting/daily-close.
//   2. Pick today (the seed fixture has unclosed shifts to be sealed).
//   3. Confirm the close -- the "Close day" button surfaces.
//   4. The status pill flips to "Closed" + the PDF artifact link
//      renders.
//
// Gated by RUN_FULL_E2E=true.

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- daily close", function () {
  it("closes today's day and renders the PDF artifact link", async function () {
    await clickLinkWithText("Daily close")
    await waitForText("Daily close")

    // Click "Close day" -- the close modal opens with the provisional
    // banner.
    await clickButtonWithText("Close day")
    await waitForText("Provisional")

    // Confirm the close -- the modal's Save / Confirm button submits.
    await clickButtonWithText("Confirm close")

    // The status pill flips to "Closed".
    await waitForText("Closed")
    // The PDF artifact link surfaces.
    await waitForText("Open PDF")
  })
})
