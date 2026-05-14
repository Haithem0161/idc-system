import { describe, expect, it } from "vitest"

import {
  ConflictSchema,
  SyncStatusSchema,
  SyncStatusSnapshotSchema,
} from "@/lib/schemas/sync"

describe("SyncStatusSchema", () => {
  it("parses each of the five legal states", () => {
    for (const state of ["idle", "pushing", "pulling", "offline", "error"] as const) {
      expect(SyncStatusSchema.parse(state)).toBe(state)
    }
  })

  it("rejects an unknown state with a ZodError", () => {
    const result = SyncStatusSchema.safeParse("syncing")
    expect(result.success).toBe(false)
  })

  it("rejects non-string input", () => {
    expect(SyncStatusSchema.safeParse(0).success).toBe(false)
    expect(SyncStatusSchema.safeParse(null).success).toBe(false)
    expect(SyncStatusSchema.safeParse(undefined).success).toBe(false)
  })
})

describe("ConflictSchema", () => {
  const validConflict = {
    opId: "01HZWAB000000000000000000",
    entity: "visits",
    entityId: "v-1",
    serverPayload: { status: "locked" },
    localPayload: { status: "draft" },
    reason: "conflict",
  }

  it("accepts a fully populated conflict envelope", () => {
    expect(ConflictSchema.parse(validConflict)).toEqual(validConflict)
  })

  it("requires opId", () => {
    const { opId: _opId, ...without } = validConflict
    const result = ConflictSchema.safeParse(without)
    expect(result.success).toBe(false)
  })

  it("rejects missing localPayload with the correct path (DEF-001 fix)", () => {
    const noLocal = { ...validConflict } as Record<string, unknown>
    delete noLocal.localPayload
    const result = ConflictSchema.safeParse(noLocal)
    expect(result.success).toBe(false)
    if (!result.success) {
      const paths = result.error.issues.map((i) => i.path.join("."))
      expect(paths).toContain("localPayload")
    }
  })

  it("rejects missing serverPayload with the correct path (DEF-001 fix)", () => {
    const noServer = { ...validConflict } as Record<string, unknown>
    delete noServer.serverPayload
    const result = ConflictSchema.safeParse(noServer)
    expect(result.success).toBe(false)
    if (!result.success) {
      const paths = result.error.issues.map((i) => i.path.join("."))
      expect(paths).toContain("serverPayload")
    }
  })

  it("rejects when both payload keys are missing", () => {
    const neither = { ...validConflict } as Record<string, unknown>
    delete neither.localPayload
    delete neither.serverPayload
    const result = ConflictSchema.safeParse(neither)
    expect(result.success).toBe(false)
    if (!result.success) {
      const paths = result.error.issues.map((i) => i.path.join("."))
      expect(paths).toContain("localPayload")
      expect(paths).toContain("serverPayload")
    }
  })

  it("accepts arbitrary unknown shapes inside the payload fields", () => {
    expect(
      ConflictSchema.parse({
        ...validConflict,
        serverPayload: null,
        localPayload: 42,
      }),
    ).toMatchObject({ opId: validConflict.opId })
  })

  it("requires the reason string", () => {
    const { reason: _reason, ...without } = validConflict
    expect(ConflictSchema.safeParse(without).success).toBe(false)
  })
})

describe("SyncStatusSnapshotSchema", () => {
  it("requires non-negative integer pendingOps", () => {
    expect(
      SyncStatusSnapshotSchema.parse({ status: "idle", pendingOps: 0 }),
    ).toEqual({ status: "idle", pendingOps: 0 })

    expect(
      SyncStatusSnapshotSchema.safeParse({ status: "idle", pendingOps: -1 })
        .success,
    ).toBe(false)
    expect(
      SyncStatusSnapshotSchema.safeParse({ status: "idle", pendingOps: 1.5 })
        .success,
    ).toBe(false)
  })

  it("rejects unknown status values inside the snapshot", () => {
    expect(
      SyncStatusSnapshotSchema.safeParse({ status: "syncing", pendingOps: 0 })
        .success,
    ).toBe(false)
  })
})
