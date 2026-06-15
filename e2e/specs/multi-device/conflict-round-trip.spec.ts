// Phase-10 multi-device E2E: conflict round-trip across two real binaries.
//
// Proves the offline-first MANUAL-policy invariant
// (.claude/rules/offline-first.md §Conflict Resolution) end-to-end across two
// actual desktop instances: when both devices edit the same settings row before
// reconnecting, the second pusher is PARKED (not silently overwritten), the
// conflict surfaces locally, and resolving it propagates the chosen value.
//
// Settings are the manual-policy probe: the phase-10 work (T1) makes the client
// skip applying a pulled row while the local copy is dirty, so the dirty edit
// pushes and the server parks it (detectSettingConflict). This spec is the
// end-to-end proof of that path through two webviews.
//
// Flow:
//   1. Device A sets settings[K] = "alpha", pushes. Server holds "alpha".
//   2. Device B pulls "alpha" (now in sync).
//   3. BOTH edit K concurrently: A -> "from-A" (pushes first, server = from-A);
//      B -> "from-B" (still at the old base version).
//   4. B pushes -> server PARKS it (version divergence) -> a conflict row
//      appears on device B.
//   5. B resolves choosing SERVER -> B converges to "from-A"; no clobber.
//
// REQUIREMENTS: identical to pull-fan-out.spec.ts (display, tauri-driver, a
// debug binary, a reachable sync server, a server-side user). See that file's
// header. Gated by RUN_FULL_E2E=true && MULTI_DEVICE=true.

import { browser } from "@wdio/globals"

import { multiDeviceDescribe } from "../../support/gate.js"
import { startSecondDevice, type SecondDevice } from "../../support/multi-device.js"
import {
  provision,
  loginVia,
  triggerPush,
  triggerPull,
  setTextSettingLocally,
  getTextSettingLocally,
  listConflicts,
  resolveConflict,
  type Device,
} from "../../support/sync-driver.js"

const SYNC_URL = process.env.E2E_SYNC_URL ?? "http://localhost:3161"
const EMAIL = process.env.E2E_LOGIN_EMAIL ?? "admin@idc.local"
const PASSWORD = process.env.E2E_LOGIN_PASSWORD ?? "hunter22pw"
const TENANT = process.env.E2E_TENANT_ID ?? "clinic-1"

// A run-unique key so reruns against the same persistent server never collide
// on a stale parked conflict. Derived from the wall clock, formatted in-spec
// (Date.now is fine in a test, unlike in a Workflow script).
const KEY = `e2e_conflict_probe_${Date.now().toString(36)}`

multiDeviceDescribe("Phase-10 -- conflict round-trip across two devices", function () {
  this.timeout(180_000)

  let deviceB: SecondDevice | undefined

  before(async function () {
    deviceB = await startSecondDevice()
  })

  after(async function () {
    if (deviceB) await deviceB.stop()
  })

  it("a divergent settings edit parks on the second pusher and resolves to the server value", async function () {
    const a: Device = { browser, label: "A" }
    const b: Device = { browser: deviceB!.browser, label: "B" }

    await provision(a, SYNC_URL)
    await provision(b, SYNC_URL)
    await loginVia(a, { email: EMAIL, password: PASSWORD, tenant: TENANT })
    await loginVia(b, { email: EMAIL, password: PASSWORD, tenant: TENANT })

    // 1. A establishes the base value; 2. B pulls it (both at the same version).
    await setTextSettingLocally(a, KEY, "alpha")
    await triggerPush(a)
    await triggerPull(b)
    if ((await getTextSettingLocally(b, KEY)) !== "alpha") {
      throw new Error("device B did not receive the base settings value before the conflict")
    }

    // 3. Concurrent divergent edits. A pushes first -> server = "from-A".
    await setTextSettingLocally(a, KEY, "from-A")
    await triggerPush(a)
    await setTextSettingLocally(b, KEY, "from-B")

    // 4. B pushes its stale-base edit -> server parks it -> conflict on B.
    await triggerPush(b)
    const conflicts = await listConflicts(b)
    const mine = conflicts.find((c) => c.entity === "settings")
    if (!mine) {
      throw new Error(
        `device B divergent push was not parked as a conflict (conflicts=${JSON.stringify(conflicts)})`,
      )
    }

    // 5. Resolve choosing the server value -> B converges to "from-A".
    await resolveConflict(b, mine.op_id, "server")
    await triggerPull(b)
    const finalB = await getTextSettingLocally(b, KEY)
    if (finalB !== "from-A") {
      throw new Error(`device B did not converge to the server value after resolve (got ${finalB})`)
    }
  })
})
