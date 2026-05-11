import { z } from "zod"

export const SyncStatusSchema = z.enum(["idle", "pushing", "pulling", "offline", "error"])
export type SyncStatus = z.infer<typeof SyncStatusSchema>

export const ConflictSchema = z.object({
  opId: z.string(),
  entity: z.string(),
  entityId: z.string(),
  serverPayload: z.unknown(),
  localPayload: z.unknown(),
  reason: z.string(),
})
export type Conflict = z.infer<typeof ConflictSchema>

export const SyncStatusSnapshotSchema = z.object({
  status: SyncStatusSchema,
  pendingOps: z.number().int().nonnegative(),
})
export type SyncStatusSnapshot = z.infer<typeof SyncStatusSnapshotSchema>
