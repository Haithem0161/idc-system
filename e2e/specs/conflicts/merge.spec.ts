// Phase-09 §4.1 E2E: conflict resolver -- merge editor.
//
// Proves the superadmin can resolve a manual-policy conflict via the
// merge editor (choice="merged"). Phase-08 §3.Frontend MergeEditor +
// the "additive merge edits both columns" invariant.
//
//   1. Land on /admin/conflicts.
//   2. Open the first conflict.
//   3. Click "Merge" -- the merge editor opens with both payload
//      columns side-by-side and a JSON textarea seeded from local.
//   4. Edit the textarea to a hand-crafted merged value.
//   5. Submit -- sync_resolve_conflict fires with the merged JSON.
//
// Gated by RUN_FULL_E2E=true.

import { browser } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- conflict resolver merge editor", function () {
  it("resolves a conflict by submitting a merged payload", async function () {
    await clickLinkWithText("Conflicts")
    await clickButtonWithText("Open")

    // Flip choice to "merged" via the aria-pressed pill.
    await clickButtonWithText("Merge")

    // Edit the merge textarea (the editor renders a textarea seeded
    // from the local payload as JSON).
    const textarea = await browser.$("textarea")
    await textarea.waitForExist({ timeout: 10_000 })
    await textarea.setValue(`{"merged_field": "e2e-merge"}`)

    // Submit -- sync_resolve_conflict fires with choice=merged.
    await clickButtonWithText("Submit")
    await waitForText("Conflict resolved")
  })
})
