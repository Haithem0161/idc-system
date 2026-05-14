import { beforeEach, describe, expect, it } from "vitest"

import type { Conflict } from "@/lib/schemas/sync"
import { useSyncStatusStore } from "@/stores/sync-status-store"

function makeConflict(opId: string, overrides: Partial<Conflict> = {}): Conflict {
  return {
    opId,
    entity: "visits",
    entityId: `v-${opId}`,
    serverPayload: { v: "server" },
    localPayload: { v: "local" },
    reason: "conflict",
    ...overrides,
  }
}

describe("useSyncStatusStore", () => {
  beforeEach(() => {
    useSyncStatusStore.setState({
      status: "idle",
      pendingOps: 0,
      lastError: null,
      conflicts: [],
    })
  })

  it("initialises with status idle, pendingOps 0, no conflicts", () => {
    const s = useSyncStatusStore.getState()
    expect(s.status).toBe("idle")
    expect(s.pendingOps).toBe(0)
    expect(s.lastError).toBeNull()
    expect(s.conflicts).toEqual([])
  })

  it("setStatus transitions through every legal state", () => {
    for (const status of ["pushing", "pulling", "offline", "error", "idle"] as const) {
      useSyncStatusStore.getState().setStatus(status)
      expect(useSyncStatusStore.getState().status).toBe(status)
    }
  })

  it("setPendingOps updates the counter", () => {
    useSyncStatusStore.getState().setPendingOps(7)
    expect(useSyncStatusStore.getState().pendingOps).toBe(7)
    useSyncStatusStore.getState().setPendingOps(0)
    expect(useSyncStatusStore.getState().pendingOps).toBe(0)
  })

  it("setError stores and clears the last error message", () => {
    useSyncStatusStore.getState().setError("boom")
    expect(useSyncStatusStore.getState().lastError).toBe("boom")
    useSyncStatusStore.getState().setError(null)
    expect(useSyncStatusStore.getState().lastError).toBeNull()
  })

  it("addConflict appends a new conflict", () => {
    useSyncStatusStore.getState().addConflict(makeConflict("op-A"))
    useSyncStatusStore.getState().addConflict(makeConflict("op-B"))
    const conflicts = useSyncStatusStore.getState().conflicts
    expect(conflicts.map((c) => c.opId)).toEqual(["op-A", "op-B"])
  })

  it("addConflict deduplicates by opId (phase-01 §1.2 invariant)", () => {
    useSyncStatusStore.getState().addConflict(makeConflict("op-A"))
    useSyncStatusStore.getState().addConflict(
      makeConflict("op-A", { reason: "different reason" }),
    )
    const conflicts = useSyncStatusStore.getState().conflicts
    expect(conflicts).toHaveLength(1)
    // First conflict wins; later attempts with the same opId are no-ops.
    expect(conflicts[0]?.reason).toBe("conflict")
  })

  it("clearConflicts empties the conflict list", () => {
    useSyncStatusStore.getState().addConflict(makeConflict("op-A"))
    useSyncStatusStore.getState().addConflict(makeConflict("op-B"))
    useSyncStatusStore.getState().clearConflicts()
    expect(useSyncStatusStore.getState().conflicts).toEqual([])
  })

  it("dedup on duplicate opId does not return a new reference (no re-render)", () => {
    useSyncStatusStore.getState().addConflict(makeConflict("op-A"))
    const before = useSyncStatusStore.getState().conflicts
    useSyncStatusStore.getState().addConflict(makeConflict("op-A"))
    const after = useSyncStatusStore.getState().conflicts
    // Implementation returns the existing state object when dedup hits, so
    // identity is preserved. This protects subscribers from spurious renders.
    expect(after).toBe(before)
  })
})
