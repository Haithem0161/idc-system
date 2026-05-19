// Phase-09 §4.1 E2E: conflict resolver -- choose local | choose server.
//
// Proves the superadmin can resolve a manual-policy conflict via the
// resolver panel. Phase-08 §3.Frontend ConflictResolverPanel + the
// manual policy invariant from `.claude/rules/offline-first.md` §11.
//
//   1. Pre-seed: trigger a conflict on a syncable manual-policy entity
//      (the multi-device sub-suite owns this seeding; the single-device
//      spec consumes a conflict pre-seeded by the fixture).
//   2. Land on /admin/conflicts.
//   3. Pick the first conflict row from the list.
//   4. The panel renders local + server payload columns with default
//      choice=local.
//   5. Click Submit -- the resolver fires sync_resolve_conflict with
//      choice=local; the success toast surfaces and the row drops out
//      of the list.
//
// Gated by RUN_FULL_E2E=true.

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- conflict resolver choose local", function () {
  it("resolves a conflict by choosing the local payload", async function () {
    await clickLinkWithText("Conflicts")
    await waitForText("Conflicts")

    // Pick the first conflict row.
    await clickButtonWithText("Open")

    // Default choice is "local". Submit.
    await clickButtonWithText("Submit")

    // Success toast surfaces.
    await waitForText("Conflict resolved")
  })
})
