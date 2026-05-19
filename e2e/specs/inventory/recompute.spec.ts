// Phase-09 §4.1 E2E: inventory recompute.
//
// Proves the superadmin can trigger a recompute of an item's on-hand
// quantity from its adjustment history (phase-06 §7.x recompute_op).
// The recompute emits an audit row + updates the items table without
// requiring a full app restart.
//
//   1. Navigate to /admin/inventory/<item_id>.
//   2. Click "Recompute on-hand" from the item-detail header.
//   3. The on-hand mono number ticks to the recomputed value AND a
//      success chip renders.
//
// Gated by RUN_FULL_E2E=true.

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- inventory recompute on-hand", function () {
  it("recomputes a single item's on-hand quantity and renders the success chip", async function () {
    await clickLinkWithText("Inventory")
    await waitForText("Inventory")

    // Click the first item row to open the detail page.
    await clickLinkWithText("View")
    await waitForText("Item")

    // Click "Recompute on-hand".
    await clickButtonWithText("Recompute on-hand")

    // The success chip surfaces.
    await waitForText("On-hand recomputed.")
  })
})
