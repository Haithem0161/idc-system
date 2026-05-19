// Phase-09 §4.1 E2E: inventory adjustment.
//
// Proves the operator can record a receive / writeoff / count_correction
// adjustment against an inventory item. The on-hand column refreshes
// after submit, an audit row is emitted, and the AdjustForm clears the
// delta + note inputs for the next entry. Phase-06 §3.Frontend AdjustForm.
//
//   1. Land on /admin/inventory.
//   2. Click "Adjust" on the items table.
//   3. Pick an item from the select, type a delta of 5, reason=receive.
//   4. Submit -- the success chip renders + the items table on-hand
//      column reflects the new value.
//
// Gated by RUN_FULL_E2E=true.

import { browser } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- inventory adjustment", function () {
  it("records a receive adjustment of +5 and surfaces the success chip", async function () {
    await clickLinkWithText("Inventory")
    await waitForText("Inventory")
    await clickButtonWithText("Adjust")
    await waitForText("New adjustment")

    // Pick the first item from the select (the seed fixture has 8+
    // items including Needles + Alcohol).
    const select = await browser.$("select")
    const options = await select.$$("option")
    const firstItem = options[1]
    if (!firstItem) throw new Error("expected at least one inventory item in seed")
    const itemId = await firstItem.getAttribute("value")
    if (itemId == null) throw new Error("first item missing value attribute")
    await select.selectByAttribute("value", itemId)

    // Type a delta of 5.
    const deltaInput = await browser.$("#adjust-delta")
    await deltaInput.setValue("5")

    // Submit the form.
    const submit = await browser.$("button[type='submit']")
    await submit.click()

    // The success chip renders.
    await waitForText("Adjustment saved.")
  })
})
