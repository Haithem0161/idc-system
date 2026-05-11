import { create } from "zustand"

import type { Conflict, SyncStatus } from "@/lib/schemas/sync"

interface SyncStatusState {
  status: SyncStatus
  pendingOps: number
  lastError: string | null
  conflicts: Conflict[]
  setStatus: (status: SyncStatus) => void
  setPendingOps: (count: number) => void
  setError: (msg: string | null) => void
  addConflict: (c: Conflict) => void
  clearConflicts: () => void
}

export const useSyncStatusStore = create<SyncStatusState>((set) => ({
  status: "idle",
  pendingOps: 0,
  lastError: null,
  conflicts: [],
  setStatus: (status) => set({ status }),
  setPendingOps: (pendingOps) => set({ pendingOps }),
  setError: (lastError) => set({ lastError }),
  addConflict: (c) =>
    set((state) =>
      state.conflicts.some((existing) => existing.opId === c.opId)
        ? state
        : { conflicts: [...state.conflicts, c] }
    ),
  clearConflicts: () => set({ conflicts: [] }),
}))
