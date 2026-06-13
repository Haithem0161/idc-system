import { useQueryClient } from "@tanstack/react-query"
import { useEffect } from "react"

import { listenEvent, SYNC_EVENTS } from "@/lib/ipc"
import {
  ConflictSchema,
  SyncStatusSchema,
  type Conflict,
  type SyncStatus,
} from "@/lib/schemas/sync"
import { useSyncStatusStore } from "@/stores/sync-status-store"

import { syncKeys } from "./queries"

/**
 * Maps a synced entity table to the React Query key roots whose caches must be
 * invalidated when that entity changes via a pull. Multiple roots per entity
 * cover derived views (e.g. a visit change affects reports too).
 */
const ENTITY_QUERY_ROOTS: Record<string, readonly string[]> = {
  patients: ["patients", "visits"],
  visits: ["visits", "reports"],
  settings: ["settings"],
  check_types: ["catalog"],
  check_subtypes: ["catalog"],
  doctors: ["catalog"],
  doctor_check_pricing: ["catalog"],
  operators: ["catalog"],
  operator_specialties: ["catalog"],
  inventory_items: ["inventory"],
  inventory_consumption_map: ["inventory", "catalog"],
  inventory_adjustments: ["inventory"],
  operator_shifts: ["shifts", "reports"],
  users: ["users"],
  audit_log: ["audit"],
}

export function rootsForEntities (entities: string[]): string[] {
  const roots = new Set<string>()
  for (const e of entities) {
    for (const r of ENTITY_QUERY_ROOTS[e] ?? []) roots.add(r)
  }
  return [...roots]
}

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
  const setUpgradeRequired = useSyncStatusStore((s) => s.setUpgradeRequired)
  const queryClient = useQueryClient()

  useEffect(() => {
    // `listenEvent` resolves asynchronously, so under StrictMode the effect's
    // cleanup can run BEFORE a `.then` pushes its unlisten fn. Track a
    // `cancelled` flag: any listener that registers after cleanup is removed
    // immediately, and the cleanup tears down everything already collected.
    let cancelled = false
    const unsubs: Array<() => void> = []

    const collect = (promise: Promise<() => void>) => {
      promise
        .then((unlisten) => {
          if (cancelled) {
            unlisten()
          } else {
            unsubs.push(unlisten)
          }
        })
        .catch(() => void 0)
    }

    // When a pull applies remote rows, invalidate exactly the affected caches
    // so mounted screens refetch the new data instead of showing stale rows
    // until a remount. This is the missing "auto-update from the server" link.
    collect(
      listenEvent<{ entities?: string[] }>(SYNC_EVENTS.APPLIED, (payload) => {
        const entities = Array.isArray(payload?.entities) ? payload.entities : []
        for (const root of rootsForEntities(entities)) {
          void queryClient.invalidateQueries({ queryKey: [root] })
        }
      })
    )

    collect(
      listenEvent<SyncStatus>(SYNC_EVENTS.STATUS, (payload) => {
        const parsed = SyncStatusSchema.safeParse(payload)
        if (parsed.success) {
          setStatus(parsed.data)
        }
        if (parsed.success && parsed.data !== "error") setError(null)
      })
    )

    // DEF-007 G11: stamp lastPushedAt only on a REAL push. The engine emits
    // sync:progress { pushed } when it actually acks ops; the old code stamped
    // it on every idle transition (including idle pulls and no-op cycles), so
    // the UserMenu red dot was permanently suppressed.
    collect(
      listenEvent<{ pushed?: number }>(SYNC_EVENTS.PROGRESS, (payload) => {
        if (typeof payload?.pushed === "number" && payload.pushed > 0) {
          setLastPushedAt(Date.now())
        }
      })
    )

    collect(
      listenEvent<Conflict>(SYNC_EVENTS.CONFLICT, (payload) => {
        const parsed = ConflictSchema.safeParse(payload)
        if (parsed.success) {
          addConflict(parsed.data)
          // Mirror the new conflict into the React Query cache so the
          // resolver page refreshes the moment a conflict arrives, instead
          // of only when the store-backed badge re-renders.
          void queryClient.invalidateQueries({ queryKey: syncKeys.conflicts })
        }
      })
    )

    collect(
      listenEvent<void>(SYNC_EVENTS.AUTH_EXPIRED, () => {
        setError("session_expired")
      })
    )

    collect(
      listenEvent<void>(SYNC_EVENTS.UPGRADE_REQUIRED, () => {
        setUpgradeRequired(true)
      })
    )

    return () => {
      cancelled = true
      for (const u of unsubs) {
        try {
          u()
        } catch {
          // ignore
        }
      }
    }
  }, [setStatus, setError, addConflict, setLastPushedAt, setUpgradeRequired, queryClient])
}
