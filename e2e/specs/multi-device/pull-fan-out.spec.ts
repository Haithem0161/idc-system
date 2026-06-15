// Phase-10 multi-device E2E: pull fan-out across two real binaries.
//
// Proves the load-bearing offline-first fan-out invariant
// (.claude/rules/offline-first.md §The Sync Engine): a row created on
// device A and pushed reaches device B's local SQLite via the pull loop.
//
// Unlike the sync-server's own roundtrip.mjs (which drives the HTTP contract
// directly), this drives TWO actual desktop webviews end-to-end: device A is
// the base wdio session (`browser`), device B is a second binary with an
// isolated app-data dir spun up by support/multi-device.ts. Each has its own
// SQLite, outbox, and pinned key -- exactly like two physical workstations
// pointed at one sync server.
//
// REQUIREMENTS (this only RUNS on a maintainer machine, not CI):
//   - A display (the webview must render).
//   - tauri-driver (`cargo install tauri-driver`) + the platform webview
//     driver (Linux: `webkit2gtk-driver`).
//   - A debug binary: `pnpm tauri build --no-bundle --debug`.
//   - A reachable sync server. Point both devices at it via E2E_SYNC_URL
//     (default http://localhost:3161 -- e.g. the one
//     sync-server/tools/run-roundtrip-e2e.sh stands up).
//   - A user that exists on that server. Supply E2E_LOGIN_EMAIL /
//     E2E_LOGIN_PASSWORD / E2E_TENANT_ID (defaults match the roundtrip gate's
//     bootstrap admin: admin@idc.local / hunter22pw / clinic-1).
//
// Gated by RUN_FULL_E2E=true && MULTI_DEVICE=true.

import { browser } from "@wdio/globals"

import { multiDeviceDescribe } from "../../support/gate.js"
import { startSecondDevice, type SecondDevice } from "../../support/multi-device.js"
import {
  provision,
  loginVia,
  triggerPush,
  triggerPull,
  createPatientLocally,
  patientExistsLocally,
  type Device,
} from "../../support/sync-driver.js"

const SYNC_URL = process.env.E2E_SYNC_URL ?? "http://localhost:3161"
const EMAIL = process.env.E2E_LOGIN_EMAIL ?? "admin@idc.local"
const PASSWORD = process.env.E2E_LOGIN_PASSWORD ?? "hunter22pw"
const TENANT = process.env.E2E_TENANT_ID ?? "clinic-1"

multiDeviceDescribe("Phase-10 -- pull fan-out across two devices", function () {
  // The two-binary lifecycle is slow (binary launch + WebDriver session +
  // sync round trips); give the suite headroom over the 60s mocha default.
  this.timeout(180_000)

  let deviceB: SecondDevice | undefined

  before(async function () {
    deviceB = await startSecondDevice()
  })

  after(async function () {
    if (deviceB) await deviceB.stop()
  })

  it("a patient created on device A surfaces on device B via the pull loop", async function () {
    const a: Device = { browser, label: "A" }
    const b: Device = { browser: deviceB!.browser, label: "B" }

    // Both devices point at the same server and authenticate as the same
    // tenant user -- the JWT entityId is what scopes the fan-out.
    await provision(a, SYNC_URL)
    await provision(b, SYNC_URL)
    await loginVia(a, { email: EMAIL, password: PASSWORD, tenant: TENANT })
    await loginVia(b, { email: EMAIL, password: PASSWORD, tenant: TENANT })

    // Device A creates a patient locally (commits to A's SQLite + outbox),
    // then pushes it to the server.
    const patientId = await createPatientLocally(a, "Fan-Out Patient")
    await triggerPush(a)

    // Device B pulls; the server fans A's push down to B's local SQLite.
    await triggerPull(b)

    const landed = await patientExistsLocally(b, patientId)
    if (!landed) {
      throw new Error(`patient ${patientId} did not fan out to device B after pull`)
    }
  })
})
