// Phase-09 §4.3 multi-device E2E: pull fan-out.
//
// Proves a server-side change (created by device A) lands on device B
// via the pull loop -- the load-bearing sync-engine fan-out invariant
// from `.claude/rules/offline-first.md` §2.
//
//   - Device A creates a visit and pushes.
//   - Device B's pull loop runs (or is manually triggered).
//   - Device B's UI now shows the visit in the workspace.
//
// Gated by RUN_FULL_E2E=true + MULTI_DEVICE=true.

import { multiDeviceDescribe } from "../../support/gate.js"

multiDeviceDescribe("Phase-09 §4.3 -- pull fan-out across two devices", function () {
  it("server change from device A surfaces on device B via the pull loop", async function () {
    // TODO(phase-10): wire the two-binary harness (see
    // conflict-round-trip.spec.ts for the implementation plan). This
    // scaffold reserves the spec name and proves the gate logic.
    this.skip()
  })
})
