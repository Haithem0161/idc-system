import { useEffect } from "react"

import { listenEvent, SYNC_EVENTS } from "@/lib/ipc"
import {
  ConflictSchema,
  SyncStatusSchema,
  type Conflict,
  type SyncStatus,
} from "@/lib/schemas/sync"
import { useSyncStatusStore } from "@/stores/sync-status-store"

/**
 * Subscribes to `sync:*` Tauri events and pushes them into the Zustand store.
 *
 * Mount this hook ONCE inside `<AppShell>` so the entire app sees a coherent
 * status without duplicate listeners.
 */
export function useSyncEvents (): void {
  const setStatus = useSyncStatusStore((s) => s.setStatus)
  const setError = useSyncStatusStore((s) => s.setError)
  const addConflict = useSyncStatusStore((s) => s.addConflict)
  const setLastPushedAt = useSyncStatusStore((s) => s.setLastPushedAt)

  useEffect(() => {
    const unsubs: Array<() => void> = []

    listenEvent<SyncStatus>(SYNC_EVENTS.STATUS, (payload) => {
      const parsed = SyncStatusSchema.safeParse(payload)
      if (parsed.success) {
        setStatus(parsed.data)
        // DEF-007 G11: a transition to `idle` after a `pushing` phase
        // signals a successful push. The store's `lastPushedAt`
        // timestamp drives the `<UserMenu>` red dot threshold.
        if (parsed.data === "idle") setLastPushedAt(Date.now())
      }
      if (parsed.success && parsed.data !== "error") setError(null)
    })
      .then((unlisten) => unsubs.push(unlisten))
      .catch(() => void 0)

    listenEvent<Conflict>(SYNC_EVENTS.CONFLICT, (payload) => {
      const parsed = ConflictSchema.safeParse(payload)
      if (parsed.success) addConflict(parsed.data)
    })
      .then((unlisten) => unsubs.push(unlisten))
      .catch(() => void 0)

    listenEvent<void>(SYNC_EVENTS.AUTH_EXPIRED, () => {
      setError("session_expired")
    })
      .then((unlisten) => unsubs.push(unlisten))
      .catch(() => void 0)

    return () => {
      for (const u of unsubs) {
        try {
          u()
        } catch {
          // ignore
        }
      }
    }
  }, [setStatus, setError, addConflict, setLastPushedAt])
}
