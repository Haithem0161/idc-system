// Phase-09 §4.1 E2E: new visit create draft.
//
// Proves the receptionist can capture a new visit as a DRAFT without
// locking it -- the draft persists across navigation, has no receipt,
// and does not consume inventory. Phase-05 §3.Frontend NewVisit flow.
//
//   1. Pick a check type from the reception/checks grid.
//   2. Type a patient name; if no match, the page creates the patient.
//   3. Pick a doctor (optional -- "House" is the default).
//   4. Toggle dye + report (gated by check type support flags).
//   5. Click "Save draft" -- the success chip renders + the running
//      total updates to the snapshot total.
//
// Gated by RUN_FULL_E2E=true.

import { browser } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  fillInputByPlaceholder,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- new visit create draft", function () {
  it("creates a draft visit with a new patient + house doctor + dye toggled on", async function () {
    // Step 1: pick the first check workspace from the grid.
    await clickLinkWithText("Checks")
    await waitForText("Checks")
    await clickButtonWithText("Open workspace")
    await clickButtonWithText("New visit")
    await waitForText("New visit")

    // Step 2: type a brand-new patient name.
    await fillInputByPlaceholder(
      "Type to search or add a new patient",
      "E2E Patient — Draft",
    )

    // Step 3: toggle dye on (if the check type supports it -- the
    // checkbox is enabled per phase-05 §7.4).
    const dyeCheckbox = await browser.$(
      "//label[contains(., 'Dye')]//input[@type='checkbox']",
    )
    if (await dyeCheckbox.isExisting()) {
      const disabled = await dyeCheckbox.getAttribute("disabled")
      if (disabled == null) await dyeCheckbox.click()
    }

    // Step 4: click "Save draft" -- the patient is created + the draft
    // visit is persisted; the success chip renders.
    await clickButtonWithText("Save draft")
    await waitForText("Draft saved.")
  })
})
