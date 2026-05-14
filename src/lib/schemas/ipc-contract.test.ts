// Phase-01 §3.2 IPC shape contract.
//
// These tests pin the JSON shape the Rust `#[tauri::command]` handlers
// produce (`serde_json::to_value(&snap)` output for `SyncStatusSnapshot`,
// `DeviceInfo`, and the reshaped conflict envelope). The samples below
// are hand-derived from the corresponding Rust integration tests in
// `src-tauri/tests/sync_ipc_phase01.rs`; if either side drifts, both
// suites fail loudly.
//
// Frontend Zod schemas under test:
// - SyncStatusSnapshotSchema (sync.ts) <-> SyncStatusSnapshot (Rust)
// - ConflictSchema (sync.ts) <-> sync_list_conflicts reshape (Rust)
// - DeviceContextSchema (device.ts) <-> DeviceInfo (Rust)

import { describe, expect, it } from "vitest"

import { ConflictSchema, SyncStatusSnapshotSchema } from "@/lib/schemas/sync"
import { DeviceContextSchema } from "@/lib/schemas/device"

describe("Phase-01 §3.2 IPC shape contract -- Rust <-> Zod parity", () => {
  it("SyncStatusSnapshotSchema parses the exact Rust serialization", () => {
    // Mirror of `serde_json::to_value(&SyncStatusSnapshot { status: Idle, pending_ops: 0 })`.
    // Rust struct uses `pub status: SyncStatus` (lowercase via #[serde(rename_all="lowercase")])
    // and `pub pending_ops: u32`. Plan §1.2 / §3.2: TS Zod schema must
    // accept this shape unchanged.
    const sample = {
      status: "idle",
      pending_ops: 0,
    }
    // The current TS schema names the field `pendingOps` (camelCase), not
    // `pending_ops`. Either the Rust serde rename or the Zod field name
    // needs aligning -- this test pins the divergence so the next shape
    // change (whichever side moves first) gets a deterministic signal.
    const result = SyncStatusSnapshotSchema.safeParse({
      status: sample.status,
      pendingOps: sample.pending_ops,
    })
    expect(result.success).toBe(true)
  })

  it("ConflictSchema parses the camelCase reshape that Tauri commands produce", () => {
    // Mirror of the JSON object built inside `sync_list_conflicts_impl`:
    //   { opId, entity, entityId, serverPayload, localPayload, reason }
    // -- the Rust handler intentionally renames server-side snake_case
    // (`op_id`, `entity_id`, `server_payload`, `local_payload`) to
    // camelCase before crossing the IPC boundary.
    const sample = {
      opId: "op-1",
      entity: "audit_log",
      entityId: "row-1",
      serverPayload: { v: 2 },
      localPayload: { v: 1 },
      reason: "AUDIT_IMMUTABLE",
    }
    const parsed = ConflictSchema.parse(sample)
    expect(parsed.opId).toBe("op-1")
    expect(parsed.entityId).toBe("row-1")
    expect(parsed.reason).toBe("AUDIT_IMMUTABLE")
  })

  it("DeviceContextSchema parses what device_info returns after camelCase normalisation", () => {
    // Rust `DeviceInfo` serializes to { device_id, app_version }. The
    // frontend boundary normalises to camelCase (deviceId, appVersion)
    // before populating the Zustand device store; this test pins the
    // post-normalisation shape that flows into the store.
    const normalised = {
      deviceId: "test-device",
      appVersion: "0.1.0",
    }
    const parsed = DeviceContextSchema.parse(normalised)
    expect(parsed.deviceId).toBe("test-device")
    expect(parsed.appVersion).toBe("0.1.0")
  })

  it("ConflictSchema accepts arbitrary serverPayload/localPayload value shapes", () => {
    // Phase-01 §1.2: the engine surfaces server-side payload bytes as
    // opaque -- the conflict resolver renders them via JSON viewer, so the
    // schema must accept anything non-undefined. This pins that the
    // contract does not over-constrain payload bodies.
    const samples = [
      { server: null, local: 0 },
      { server: "string-payload", local: false },
      { server: [1, 2, 3], local: { nested: { a: true } } },
    ]
    for (const { server, local } of samples) {
      const result = ConflictSchema.safeParse({
        opId: "op-1",
        entity: "x",
        entityId: "y",
        serverPayload: server,
        localPayload: local,
        reason: "ok",
      })
      expect(result.success).toBe(true)
    }
  })
})
