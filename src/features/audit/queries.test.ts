// Phase-08 §2.4 React Query hook tests for the audit + diagnostics feature
// surface. Every hook test runs in both `dir=ltr` and `dir=rtl` per the
// plan's RTL invariant (`.claude/rules/testing.md` §14 anti-pattern row
// "RTL never tested").

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
  }
})

import { invoke, isTauri } from "@/lib/ipc"
import {
  auditKeys,
  useAuditQuery,
  useAuditVacuum,
  useDiagnosticsSummary,
} from "@/features/audit/queries"

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

const SAMPLE_ROW = {
  id: "row-1",
  at: "2026-05-13T12:00:00Z",
  actor_user_id: "0190f3a0-f1c0-7000-8000-000000000abc",
  action: "create",
  entity: "doctors",
  entity_id: "ent-1",
  delta: { k: "v" },
  device_id: "device-1",
  version: 1,
  dirty: false,
  source: "local" as const,
}

const SAMPLE_DIAGNOSTICS = {
  lock_latency_p95_ms: 95,
  outbox_depth: 12,
  last_sync_at: "2026-05-13T12:00:00Z",
  conflict_count_7d: 3,
  receipt_print_success_rate_30d: 0.987,
}

describe.each(directions)(
  "Phase-08 §2.4 audit feature hooks (dir=%s)",
  (dir) => {
    beforeEach(() => {
      document.documentElement.dir = dir
      vi.mocked(invoke).mockReset()
    })

    afterEach(() => {
      document.documentElement.dir = ""
    })

    describe("auditKeys", () => {
      it("derives a stable query key from the filter shape", () => {
        const filter = { action: "lock" as const }
        const key = auditKeys.query(filter)
        expect(key[0]).toBe("audit")
        expect(key[1]).toBe("query")
        expect(key[2]).toEqual(filter)
      })

      it("pins the diagnostics key to a stable tuple", () => {
        expect(auditKeys.diagnostics).toEqual(["diagnostics", "summary"])
      })

      it("treats two filter objects with identical shape as equal", () => {
        const a = auditKeys.query({ action: "lock" })
        const b = auditKeys.query({ action: "lock" })
        expect(a).toEqual(b)
      })
    })

    describe("useAuditQuery", () => {
      it("calls audit_query IPC and parses the page through Zod", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          rows: [SAMPLE_ROW],
          mode: "local",
          next_offset: null,
        })
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useAuditQuery({}), { wrapper })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.rows).toHaveLength(1)
        expect(result.current.data?.mode).toBe("local")
        expect(invoke).toHaveBeenCalledWith("audit_query", { args: {} })
      })

      it("forwards filter args verbatim into the IPC envelope", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          rows: [],
          mode: "local",
          next_offset: null,
        })
        const { wrapper } = makeWrapper()
        const filters = {
          action: "lock" as const,
          entity: "visits" as const,
          entity_id_prefix: "abcd",
          limit: 25,
        }
        renderHook(() => useAuditQuery(filters), { wrapper })
        await waitFor(() => expect(invoke).toHaveBeenCalled())
        expect(invoke).toHaveBeenCalledWith("audit_query", { args: filters })
      })

      it("does not run when not in Tauri (web preview / SSR)", async () => {
        vi.mocked(isTauri).mockReturnValueOnce(false)
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useAuditQuery({}), { wrapper })
        expect(result.current.fetchStatus).toBe("idle")
        expect(invoke).not.toHaveBeenCalled()
      })

      it("surfaces zod parse errors when the IPC envelope drifts", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          rows: [{ ...SAMPLE_ROW, source: "bogus" }],
          mode: "local",
          next_offset: null,
        } as unknown as Awaited<ReturnType<typeof invoke<"audit_query">>>)
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useAuditQuery({}), { wrapper })
        await waitFor(() => expect(result.current.isError).toBe(true))
      })

      it("renders next_offset of zero correctly (not falsy)", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          rows: [SAMPLE_ROW],
          mode: "merged",
          next_offset: 0,
        })
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useAuditQuery({}), { wrapper })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.next_offset).toBe(0)
      })

      it("propagates mode=server in the parsed page", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          rows: [],
          mode: "server",
          next_offset: null,
        })
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useAuditQuery({}), { wrapper })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.mode).toBe("server")
      })

      it("propagates mode=merged for cross-boundary queries", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          rows: [SAMPLE_ROW],
          mode: "merged",
          next_offset: 50,
        })
        const { wrapper } = makeWrapper()
        const { result } = renderHook(
          () =>
            useAuditQuery({
              from_utc: "2026-01-01T00:00:00Z",
              to_utc: "2026-05-01T00:00:00Z",
            }),
          { wrapper },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.mode).toBe("merged")
        expect(result.current.data?.next_offset).toBe(50)
      })
    })

    describe("useAuditVacuum", () => {
      it("invokes audit_vacuum_now and invalidates audit queries on success", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          audit_purged: 5,
          metrics_purged: 2,
        })
        const { wrapper, client } = makeWrapper()
        const spy = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useAuditVacuum(), { wrapper })
        await result.current.mutateAsync()
        expect(invoke).toHaveBeenCalledWith("audit_vacuum_now")
        expect(spy).toHaveBeenCalledWith({ queryKey: ["audit"] })
      })

      it("surfaces IPC errors as mutation errors", async () => {
        vi.mocked(invoke).mockRejectedValueOnce(new Error("forbidden"))
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useAuditVacuum(), { wrapper })
        await expect(result.current.mutateAsync()).rejects.toThrow(
          /forbidden/,
        )
      })
    })

    describe("useDiagnosticsSummary", () => {
      it("calls diagnostics_summary IPC and parses through Zod", async () => {
        vi.mocked(invoke).mockResolvedValueOnce(SAMPLE_DIAGNOSTICS)
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useDiagnosticsSummary(), {
          wrapper,
        })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.outbox_depth).toBe(12)
        expect(result.current.data?.conflict_count_7d).toBe(3)
        expect(invoke).toHaveBeenCalledWith("diagnostics_summary")
      })

      it("does not run when not in Tauri", async () => {
        vi.mocked(isTauri).mockReturnValueOnce(false)
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useDiagnosticsSummary(), {
          wrapper,
        })
        expect(result.current.fetchStatus).toBe("idle")
        expect(invoke).not.toHaveBeenCalled()
      })

      it("accepts an all-null summary (fresh install)", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          lock_latency_p95_ms: null,
          outbox_depth: 0,
          last_sync_at: null,
          conflict_count_7d: 0,
          receipt_print_success_rate_30d: null,
        })
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useDiagnosticsSummary(), {
          wrapper,
        })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.lock_latency_p95_ms).toBeNull()
        expect(result.current.data?.outbox_depth).toBe(0)
      })

      it("surfaces zod errors when the rust DTO drifts shape", async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
          lock_latency_p95_ms: 95,
          outbox_depth: -5, // invalid: nonnegative required
          last_sync_at: null,
          conflict_count_7d: 0,
          receipt_print_success_rate_30d: null,
        } as unknown as Awaited<
          ReturnType<typeof invoke<"diagnostics_summary">>
        >)
        const { wrapper } = makeWrapper()
        const { result } = renderHook(() => useDiagnosticsSummary(), {
          wrapper,
        })
        await waitFor(() => expect(result.current.isError).toBe(true))
      })
    })
  },
)
