// Phase-09 §4.3 multi-device E2E: conflict round-trip.
//
// Proves the offline-first conflict-policy invariants when TWO
// devices edit the same row before reconnecting:
//
//   - Two devices A and B start synced.
//   - Both go offline.
//   - Each edits the same manual-policy entity (e.g. a visit row).
//   - Both reconnect; the server returns 409 to the second pusher.
//   - The conflict resolver panel surfaces on both devices.
//   - Resolving on one device propagates the chosen payload to the
//     other on the next pull.
//
// This spec runs ONLY when both `RUN_FULL_E2E=true` AND
// `MULTI_DEVICE=true`. The default CI path skips it.
//
// Setup notes:
//
//   - The wdio config currently spins ONE tauri-driver against ONE
//     binary instance. The multi-device spec is the placeholder for a
//     future per-spec custom config that spins TWO binaries against a
//     shared sync-server stack. The current scaffold documents the
//     intended flow; the actual two-binary harness is a follow-up
//     (phase-10 task, not phase-09 ship-blocker).

import { multiDeviceDescribe } from "../../support/gate.js"

multiDeviceDescribe("Phase-09 §4.3 -- conflict round-trip across two devices", function () {
  it("two devices edit the same row offline; reconnect produces a resolver flow on the second pusher", async function () {
    // TODO(phase-10): wire the two-binary harness. The current
    // scaffold proves the gate logic and reserves the spec name in
    // the test inventory. Implementation requires:
    //
    //   1. Spawn a second tauri binary via child_process inside this
    //      `it` block, pointing at the same sync-server backend.
    //   2. Drive both binaries through their respective WebdriverIO
    //      sessions (browser.newSession() per binary).
    //   3. Execute the offline-edit + reconnect + 409 sequence.
    //   4. Assert the resolver panel surfaces on the second pusher.
    //
    // The flow itself is asserted by the Rust integration suite in
    // src-tauri/tests/sync_phase01.rs + src-tauri/tests/preship_phase09.rs;
    // this spec adds the E2E layer once the harness lands.
    this.skip()
  })
})
