import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri } from "@/lib/ipc"
import {
  ConflictSchema,
  SyncStatusSchema,
  type Conflict,
  type SyncStatus,
} from "@/lib/schemas/sync"

export const syncKeys = {
  status: ["sync", "status"] as const,
  conflicts: ["sync", "conflicts"] as const,
  outboxCount: ["sync", "outbox-count"] as const,
  stuck: ["sync", "stuck"] as const,
}

export interface SyncStatusSnapshot {
  status: SyncStatus
  pendingOps: number
  stuckOps: number
}

function normalizeSnapshot (raw: unknown): SyncStatusSnapshot {
  if (raw && typeof raw === "object") {
    const obj = raw as Record<string, unknown>
    const status = SyncStatusSchema.parse(obj.status ?? "idle")
    const pending = obj.pendingOps ?? obj.pending_ops ?? 0
    const stuck = obj.stuckOps ?? obj.stuck_ops ?? 0
    return {
      status,
      pendingOps: typeof pending === "number" ? pending : 0,
      stuckOps: typeof stuck === "number" ? stuck : 0,
    }
  }
  return { status: "offline", pendingOps: 0, stuckOps: 0 }
}

export function useSyncStatus () {
  return useQuery({
    queryKey: syncKeys.status,
    enabled: isTauri(),
    queryFn: async (): Promise<SyncStatusSnapshot> => {
      const raw = await invoke("sync_status")
      return normalizeSnapshot(raw)
    },
    refetchInterval: 5_000,
    staleTime: 2_000,
  })
}

export function useOutboxCount () {
  return useQuery({
    queryKey: syncKeys.outboxCount,
    enabled: isTauri(),
    queryFn: async (): Promise<number> => {
      const n = await invoke("sync_outbox_count")
      return typeof n === "number" ? n : 0
    },
    refetchInterval: 2_000,
  })
}

export function useSyncConflicts () {
  return useQuery({
    queryKey: syncKeys.conflicts,
    enabled: isTauri(),
    queryFn: async (): Promise<Conflict[]> => {
      const rows = await invoke("sync_list_conflicts", { limit: 100, offset: 0 })
      if (!Array.isArray(rows)) return []
      return rows.map((row) => ConflictSchema.parse(row))
    },
  })
}

export function useTriggerPush () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      await invoke("sync_trigger_push")
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: syncKeys.status })
      void qc.invalidateQueries({ queryKey: syncKeys.outboxCount })
    },
  })
}

export function useTriggerPull () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      await invoke("sync_trigger_pull")
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: syncKeys.status })
    },
  })
}

export interface StuckOp {
  opId: string
  entity: string
  entityId: string
  attempts: number
  parked: boolean
  lastError: string | null
  createdAt: string
}

export function useStuckOps () {
  return useQuery({
    queryKey: syncKeys.stuck,
    enabled: isTauri(),
    queryFn: async (): Promise<StuckOp[]> => {
      const rows = await invoke("sync_list_stuck")
      if (!Array.isArray(rows)) return []
      return rows.map((r) => ({
        opId: r.op_id,
        entity: r.entity,
        entityId: r.entity_id,
        attempts: r.attempts,
        parked: r.parked,
        lastError: r.last_error,
        createdAt: r.created_at,
      }))
    },
    refetchInterval: 10_000,
  })
}

export function useRequeueOp () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (opId: string) => {
      await invoke("sync_requeue_op", { opId })
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: syncKeys.stuck })
      void qc.invalidateQueries({ queryKey: syncKeys.status })
      void qc.invalidateQueries({ queryKey: syncKeys.outboxCount })
    },
  })
}
