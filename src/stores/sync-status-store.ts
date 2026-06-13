import { create } from "zustand"

import type { Conflict, SyncStatus } from "@/lib/schemas/sync"

interface SyncStatusState {
  status: SyncStatus
  pendingOps: number
  lastError: string | null
  /// Set when the server rejects this app version with 426. The shell shows a
  /// blocking upgrade banner; sync is paused until the user updates.
  upgradeRequired: boolean
  setUpgradeRequired: (v: boolean) => void
  conflicts: Conflict[]
  /// DEF-007 G11: epoch-ms timestamp of the last successful sync push.
  /// `null` when no push has succeeded yet this session. The
  /// `<UserMenu>` red dot derives from this + `pendingOps`.
  lastPushedAt: number | null
  setStatus: (status: SyncStatus) => void
  setPendingOps: (count: number) => void
  setError: (msg: string | null) => void
  setLastPushedAt: (epochMs: number | null) => void
  // NOTE (audit L-dead-store): `conflicts`/`addConflict`/`clearConflicts`,
  // `lastError`, and `setPendingOps` are currently write-only. The conflict
  // resolver reads parked conflicts via React Query (syncKeys.conflicts), which
  // the sync:conflict listener invalidates, so the store array is a redundant
  // mirror retained only for its dedupe-by-opId unit test. Prefer wiring a
  // consumer (e.g. a session-expired notice off `lastError`) over deleting.
  addConflict: (c: Conflict) => void
  clearConflicts: () => void
}

export const useSyncStatusStore = create<SyncStatusState>((set) => ({
  status: "idle",
  pendingOps: 0,
  lastError: null,
  upgradeRequired: false,
  setUpgradeRequired: (upgradeRequired) => set({ upgradeRequired }),
  conflicts: [],
  lastPushedAt: null,
  setStatus: (status) => set({ status }),
  setPendingOps: (pendingOps) => set({ pendingOps }),
  setError: (lastError) => set({ lastError }),
  setLastPushedAt: (lastPushedAt) => set({ lastPushedAt }),
  addConflict: (c) =>
    set((state) =>
      state.conflicts.some((existing) => existing.opId === c.opId)
        ? state
        : { conflicts: [...state.conflicts, c] }
    ),
  clearConflicts: () => set({ conflicts: [] }),
}))

/// DEF-007 G11: 5-minute threshold for the `<UserMenu>` red-dot indicator.
/// Exported as a named constant so tests can pin the boundary without
/// hard-coding 300_000 inline.
export const USER_MENU_STALE_THRESHOLD_MS = 5 * 60 * 1000

/// DEF-007 G11: derived "stale push" predicate. The avatar red dot
/// renders WHEN AND ONLY WHEN both conditions hold:
///   1. `pendingOps > 0` (something is queued in the outbox), AND
///   2. `(now - lastPushedAt) > 5 minutes` (or no push has happened yet).
/// An empty outbox suppresses the dot even after a long idle gap --
/// the dot signals "stuck push", not "stale screen".
export function isUserMenuStale (input: {
  pendingOps: number
  lastPushedAt: number | null
  now: number
}): boolean {
  if (input.pendingOps <= 0) return false
  if (input.lastPushedAt == null) return true
  return input.now - input.lastPushedAt > USER_MENU_STALE_THRESHOLD_MS
}
