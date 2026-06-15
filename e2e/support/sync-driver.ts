// Phase-10 device-agnostic sync drivers.
//
// The multi-device specs drive TWO independent webviews (device A = the base
// wdio `browser`; device B = a second binary from support/multi-device.ts).
// Both expose the same IPC surface. These helpers wrap that surface so a spec
// reads as "device A creates a patient; device B pulls" rather than as a wall
// of `browser.execute` boilerplate.
//
// Every helper goes through window.__TAURI__.core.invoke -- available because
// tauri.conf.json sets `withGlobalTauri: true`. We invoke the REAL IPC
// commands (the same ones the UI calls): config_set_sync_server_url, auth_login,
// patients_create/get, sync_trigger_push/pull. That means these specs exercise
// the actual offline-first path (local SQLite commit -> outbox -> push -> server
// -> pull -> remote SQLite), not a UI-only illusion.
//
// IPC arg-shape note: several commands take a NESTED `{ args: {...} }` envelope
// (auth_login, patients_create, patients_get) -- mirror src/lib/ipc.ts exactly.
// Top-level snake_case params map to camelCase on the JS side, but inner struct
// fields do NOT convert (so `entity_id_hint`, not `entityIdHint`).

import type { Browser } from "webdriverio"

export interface Device {
  readonly browser: Browser
  /** Short label for error messages ("A" / "B"). */
  readonly label: string
}

export interface LoginArgs {
  readonly email: string
  readonly password: string
  readonly tenant: string
}

// Run an IPC invoke inside the device's webview and return its result. The
// function body runs in the webview realm, so it can only reference globals
// (window.__TAURI__) and its serializable arguments -- not this module's scope.
async function invokeInWebview<T>(
  device: Device,
  command: string,
  args: Record<string, unknown> | undefined,
): Promise<T> {
  const result = await device.browser.execute(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    async (cmd: string, a: any) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tauri = (window as any).__TAURI__
      if (!tauri?.core?.invoke) {
        throw new Error("window.__TAURI__.core.invoke unavailable (withGlobalTauri off?)")
      }
      return tauri.core.invoke(cmd, a)
    },
    command,
    args ?? {},
  )
  return result as T
}

/** Point a device at the sync server (the first-launch URL step). */
export async function provision(device: Device, syncUrl: string): Promise<void> {
  await invokeInWebview(device, "config_set_sync_server_url", { url: syncUrl })
}

/** Authenticate a device. Throws if login does not return an online session. */
export async function loginVia(device: Device, args: LoginArgs): Promise<void> {
  const res = await invokeInWebview<{ mode?: string }>(device, "auth_login", {
    args: { email: args.email, password: args.password, entity_id_hint: args.tenant },
  })
  if (res?.mode !== "online") {
    throw new Error(`device ${device.label} login did not return an online session (mode=${res?.mode})`)
  }
}

/**
 * Create a patient in the device's LOCAL SQLite (commits locally + enqueues an
 * outbox op). Returns the client-generated id so another device can assert it
 * fanned out. Patients are last-write-wins, so this is a safe fan-out probe.
 */
export async function createPatientLocally(device: Device, name: string): Promise<string> {
  const rec = await invokeInWebview<{ id: string }>(device, "patients_create", {
    args: { name },
  })
  if (!rec?.id) throw new Error(`device ${device.label} patients_create returned no id`)
  return rec.id
}

/** True iff the patient row exists in the device's local SQLite. */
export async function patientExistsLocally(device: Device, id: string): Promise<boolean> {
  try {
    const rec = await invokeInWebview<{ id?: string } | null>(device, "patients_get", {
      args: { id },
    })
    return !!rec && rec.id === id
  } catch {
    // patients_get errors when the row is absent -> not yet pulled.
    return false
  }
}

/**
 * Set a text setting in the device's LOCAL SQLite. Settings are a MANUAL-policy
 * entity, so a divergent push from a second device parks server-side -- which
 * is exactly what the conflict-round-trip spec relies on. Returns the row's id.
 */
export async function setTextSettingLocally(
  device: Device,
  key: string,
  value: string,
): Promise<string> {
  const rec = await invokeInWebview<{ id: string }>(device, "settings_update", {
    args: { key, value: { valueType: "text", value } },
  })
  if (!rec?.id) throw new Error(`device ${device.label} settings_update returned no id`)
  return rec.id
}

/** Read a text setting's value from the device's local SQLite (null if absent). */
export async function getTextSettingLocally(
  device: Device,
  key: string,
): Promise<string | null> {
  const rec = await invokeInWebview<{ value?: { value?: string } } | null>(
    device,
    "settings_get",
    { args: { key } },
  )
  const v = rec?.value?.value
  return typeof v === "string" ? v : null
}

/** Force the push loop and wait briefly for the outbox to drain. */
export async function triggerPush(device: Device): Promise<void> {
  await invokeInWebview(device, "sync_trigger_push", {})
  await settle(device)
}

/** Force the pull loop and wait briefly for the applied changes to commit. */
export async function triggerPull(device: Device): Promise<void> {
  await invokeInWebview(device, "sync_trigger_pull", {})
  await settle(device)
}

/** List parked conflicts on a device (drives the conflict-round-trip spec). */
export async function listConflicts(
  device: Device,
): Promise<Array<{ op_id: string; entity: string; entity_id: string }>> {
  const rows = await invokeInWebview<Array<{ op_id: string; entity: string; entity_id: string }>>(
    device,
    "sync_list_conflicts",
    { limit: 100, offset: 0 },
  )
  return Array.isArray(rows) ? rows : []
}

/** Resolve a parked conflict by choosing local or server. */
export async function resolveConflict(
  device: Device,
  opId: string,
  choice: "local" | "server",
): Promise<void> {
  await invokeInWebview(device, "sync_resolve_conflict", {
    args: { opId, choice },
  })
  await settle(device)
}

// The trigger commands kick the async sync task; the IPC call returns before
// the round trip + local commit finish. Poll the outbox to a quiet state
// instead of a blind sleep, with a hard ceiling so a stuck op fails loudly.
async function settle(device: Device, timeoutMs = 20_000): Promise<void> {
  const deadline = Date.now() + timeoutMs
  // eslint-disable-next-line no-constant-condition
  while (true) {
    const n = await invokeInWebview<number>(device, "sync_outbox_count", {})
    const status = await invokeInWebview<{ status?: string }>(device, "sync_status", {})
    const busy = status?.status === "pushing" || status?.status === "pulling"
    if ((typeof n !== "number" || n === 0) && !busy) return
    if (Date.now() > deadline) return // ceiling: let the assertion report the real state
    await device.browser.pause(300)
  }
}
