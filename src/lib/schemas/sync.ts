import { z } from "zod"

export const SyncStatusSchema = z.enum(["idle", "pushing", "pulling", "offline", "error"])
export type SyncStatus = z.infer<typeof SyncStatusSchema>

// Phase-01 §1.2: serverPayload / localPayload accept arbitrary shapes
// (the conflict resolver renders both blindly), but the KEYS themselves
// must be present. `z.unknown()` alone would treat missing-as-undefined,
// silently accepting malformed envelopes from the server. The custom
// schemas below require presence; their value space is still unknown.
const requiredUnknown = z.custom<unknown>((v) => v !== undefined, {
  message: "payload is required",
})

export const ConflictSchema = z.object({
  opId: z.string(),
  entity: z.string(),
  entityId: z.string(),
  serverPayload: requiredUnknown,
  localPayload: requiredUnknown,
  reason: z.string(),
})
export type Conflict = z.infer<typeof ConflictSchema>

export const SyncStatusSnapshotSchema = z.object({
  status: SyncStatusSchema,
  pendingOps: z.number().int().nonnegative(),
})
export type SyncStatusSnapshot = z.infer<typeof SyncStatusSnapshotSchema>
