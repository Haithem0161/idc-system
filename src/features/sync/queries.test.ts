// Phase-01 §2.4 React Query hook tests for the sync feature.
//
// Every hook test runs in both `dir=ltr` and `dir=rtl` per the plan's
// `describe.each` invariant. Hooks themselves do not render DOM, but the
// wrapper toggles `document.documentElement.dir` so any later component
// test added to this file inherits the dual-mode pattern automatically.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

// Mock the IPC layer BEFORE importing the hooks under test.
vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
    listenEvent: vi.fn(async () => async () => undefined),
  }
})

import { invoke } from "@/lib/ipc"
import {
  syncKeys,
  useOutboxCount,
  useSyncConflicts,
  useSyncStatus,
  useTriggerPull,
  useTriggerPush,
} from "@/features/sync/queries"

const directions = [["ltr"], ["rtl"]] as const

function makeWrapper(): {
  wrapper: (props: { children: ReactNode }) => ReturnType<typeof createElement>
  client: QueryClient
} {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children)
  return { wrapper, client }
}

describe.each(directions)("Phase-01 §2.4 sync feature hooks (dir=%s)", (dir) => {
  beforeEach(() => {
    document.documentElement.dir = dir
    vi.mocked(invoke).mockReset()
  })

  afterEach(() => {
    document.documentElement.dir = ""
  })

  describe("useSyncStatus", () => {
    it("calls sync_status IPC and returns the snapshot", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({
        status: "idle",
        pendingOps: 3,
      })
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useSyncStatus(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toEqual({ status: "idle", pendingOps: 3, stuckOps: 0 })
      expect(invoke).toHaveBeenCalledWith("sync_status")
    })

    it("normalises pending_ops snake_case from Rust into pendingOps", async () => {
      // The IPC layer's CommandMap types pendingOps as required; the Rust
      // serde rename emits snake_case and the hook normalises it. Cast to
      // satisfy the strict CommandMap shape without losing the test
      // intent.
      vi.mocked(invoke).mockResolvedValueOnce({
        status: "pushing",
        pending_ops: 7,
      } as unknown as Awaited<ReturnType<typeof invoke<"sync_status">>>)
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useSyncStatus(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toEqual({ status: "pushing", pendingOps: 7, stuckOps: 0 })
    })

    it("does not run when not in Tauri (web preview / SSR)", async () => {
      const ipc = await import("@/lib/ipc")
      vi.mocked(ipc.isTauri).mockReturnValueOnce(false)
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useSyncStatus(), { wrapper })
      // Query is disabled; React Query reports `isPending: true` with
      // fetchStatus: 'idle' for disabled queries (v5 behaviour).
      expect(result.current.fetchStatus).toBe("idle")
      expect(invoke).not.toHaveBeenCalled()
    })
  })

  describe("useOutboxCount", () => {
    it("returns the numeric count from sync_outbox_count", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(42)
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useOutboxCount(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toBe(42)
      expect(invoke).toHaveBeenCalledWith("sync_outbox_count")
    })

    it("coerces non-number IPC results to 0 (defensive)", async () => {
      vi.mocked(invoke).mockResolvedValueOnce("not-a-number" as unknown as number)
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useOutboxCount(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toBe(0)
    })
  })

  describe("useSyncConflicts", () => {
    it("returns parsed conflicts from sync_list_conflicts", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([
        {
          opId: "op-1",
          entity: "audit_log",
          entityId: "row-1",
          serverPayload: { v: 2 },
          localPayload: { v: 1 },
          reason: "AUDIT_IMMUTABLE",
        },
      ])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useSyncConflicts(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toHaveLength(1)
      expect(result.current.data?.[0]?.opId).toBe("op-1")
      expect(invoke).toHaveBeenCalledWith("sync_list_conflicts", {
        limit: 100,
        offset: 0,
      })
    })

    it("returns empty array when IPC returns a non-array (defensive)", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(null as unknown as never)
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useSyncConflicts(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toEqual([])
    })
  })

  describe("useTriggerPush", () => {
    it("invokes sync_trigger_push and invalidates status + outbox-count caches", async () => {
      vi.mocked(invoke).mockResolvedValue(null)
      const { wrapper, client } = makeWrapper()
      const invalidateSpy = vi.spyOn(client, "invalidateQueries")

      const { result } = renderHook(() => useTriggerPush(), { wrapper })
      await result.current.mutateAsync()

      expect(invoke).toHaveBeenCalledWith("sync_trigger_push")
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: syncKeys.status })
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: syncKeys.outboxCount })
    })
  })

  describe("useTriggerPull", () => {
    it("invokes sync_trigger_pull and invalidates the status cache", async () => {
      vi.mocked(invoke).mockResolvedValue(null)
      const { wrapper, client } = makeWrapper()
      const invalidateSpy = vi.spyOn(client, "invalidateQueries")

      const { result } = renderHook(() => useTriggerPull(), { wrapper })
      await result.current.mutateAsync()

      expect(invoke).toHaveBeenCalledWith("sync_trigger_pull")
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: syncKeys.status })
    })
  })
})
