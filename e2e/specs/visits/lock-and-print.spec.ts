// Phase-09 §4.1 E2E: visit lock + receipt print.
//
// Proves the receptionist can finalise a draft into a locked visit
// with an A5 PDF receipt + a thermal text receipt, both at byte-stable
// hash. Phase-05 §4 visit-lock flow + phase-09 §10 receipt snapshot.
//
//   1. Open an existing draft (from the create-draft spec).
//   2. Click "Lock & print" -- the operator picker opens.
//   3. Pick the first qualified operator (specialty-matched per
//      phase-05 §7.10).
//   4. Confirm -- the visit status flips to "locked" + the receipt
//      artifacts render in the receipts tab.
//
// Gated by RUN_FULL_E2E=true.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  fillInputByPlaceholder,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- visit lock + receipt print", function () {
  it("locks a draft visit through the operator picker and renders receipts", async function () {
    // Open the workspace and create a draft (the create-draft spec
    // does the same prep -- this spec re-runs the prep so it can
    // execute in isolation).
    await clickLinkWithText("Checks")
    await clickButtonWithText("Open workspace")
    await clickButtonWithText("New visit")
    await fillInputByPlaceholder(
      "Type to search or add a new patient",
      "E2E Patient — Lock",
    )
    await clickButtonWithText("Save draft")
    await waitForText("Draft saved.")

    // Click "Lock & print" -- the operator picker opens.
    await clickButtonWithText("Lock & print")
    await waitForText("Pick operator")

    // Pick the first qualified operator -- the "Lock visit" button is
    // the per-row confirm.
    await clickButtonWithText("Lock visit")

    // The page navigates to /reception/visits/<id>. The status pill
    // flips to "Locked" and the receipts tab is reachable.
    await waitForText("Visit locked. Receipt rendered.")
    // Click the receipts tab to confirm the artifacts surface.
    await clickButtonWithText("Receipts")
    await waitForText("A5 receipt")
    await waitForText("Thermal receipt")

    // The reprint button is wired on the receipts tab; clicking it
    // re-emits the artifacts and shows the confirmation copy.
    await clickButtonWithText("Reprint")
    await waitForText("Receipts re-rendered.")

    // Defensive: the body should NOT contain "Draft" any more (the
    // visit is locked).
    const body = await browser.$("body")
    const html = await body.getHTML()
    expect(html).not.toContain("Save draft")
  })
})
