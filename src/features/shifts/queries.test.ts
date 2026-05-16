// Phase-04 §2.4 React Query hook tests for the shifts feature surface.
// Every test runs in both `dir=ltr` and `dir=rtl` per the plan's RTL
// invariant (`.claude/rules/testing.md` §14 anti-pattern row "RTL never
// tested").

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
import type {
  ShiftOverlapPair,
  ShiftRecord,
  ShiftWithMetaRecord,
} from "@/lib/ipc"
import {
  shiftKeys,
  useOpenShifts,
  useShiftClockIn,
  useShiftClockOut,
  useShiftEdit,
  useShiftHistoryToday,
  useShiftOverlaps,
  useShiftSoftDelete,
} from "@/features/shifts/queries"

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

const UUID_SHIFT = "0190f3a0-f1c0-7000-8000-0000000a0001"
const UUID_OPERATOR = "0190f3a0-f1c0-7000-8000-0000000b0002"
const UUID_USER = "0190f3a0-f1c0-7000-8000-0000000c0003"

function shiftFixture(overrides: Partial<ShiftRecord> = {}): ShiftRecord {
  return {
    id: UUID_SHIFT,
    operator_id: UUID_OPERATOR,
    check_in_at: "2026-05-14T10:00:00Z",
    check_out_at: null,
    check_in_by_user_id: UUID_USER,
    check_out_by_user_id: null,
    note: null,
    created_at: "2026-05-14T10:00:00Z",
    updated_at: "2026-05-14T10:00:00Z",
    deleted_at: null,
    version: 1,
    entity_id: "tenant-x",
    ...overrides,
  }
}

function shiftWithMetaFixture(
  overrides: Partial<ShiftWithMetaRecord> = {}
): ShiftWithMetaRecord {
  return {
    ...shiftFixture(),
    operator_name: "Kareem",
    operator_phone: "07700000000",
    ...overrides,
  }
}

const mockOnce = (value: unknown) => {
  vi.mocked(invoke).mockResolvedValueOnce(value as never)
}

describe.each(directions)("Phase-04 §2.4 shifts hooks (dir=%s)", (dir) => {
  beforeEach(() => {
    document.documentElement.dir = dir
    vi.mocked(invoke).mockReset()
    vi.mocked(isTauri).mockReturnValue(true)
  })

  afterEach(() => {
    document.documentElement.dir = ""
  })

  it("shiftKeys exposes the documented cache keys including overlaps `all` sentinel", () => {
    expect(shiftKeys.all).toEqual(["shifts"])
    expect(shiftKeys.open).toEqual(["shifts", "open"])
    expect(shiftKeys.historyToday).toEqual(["shifts", "today"])
    expect(shiftKeys.overlaps(undefined)).toEqual(["shifts", "overlaps", "all"])
    expect(shiftKeys.overlaps("op-1")).toEqual(["shifts", "overlaps", "op-1"])
  })

  it("useOpenShifts dispatches `shifts_list_open` and caches under shifts.open", async () => {
    const fixture = [shiftWithMetaFixture()]
    mockOnce(fixture)
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useOpenShifts(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_list_open")
    expect(client.getQueryData(shiftKeys.open)).toEqual(fixture)
  })

  it("useOpenShifts is disabled outside Tauri (no IPC call fires)", () => {
    vi.mocked(isTauri).mockReturnValue(false)
    const { wrapper } = makeWrapper()
    renderHook(() => useOpenShifts(), { wrapper })
    expect(invoke).not.toHaveBeenCalled()
  })

  it("useShiftHistoryToday dispatches `shifts_history_today` and caches under shifts.today", async () => {
    const fixture = [shiftWithMetaFixture({ check_out_at: "2026-05-14T12:00:00Z" })]
    mockOnce(fixture)
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useShiftHistoryToday(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_history_today")
    expect(client.getQueryData(shiftKeys.historyToday)).toEqual(fixture)
  })

  it("useShiftHistoryToday is disabled outside Tauri", () => {
    vi.mocked(isTauri).mockReturnValue(false)
    const { wrapper } = makeWrapper()
    renderHook(() => useShiftHistoryToday(), { wrapper })
    expect(invoke).not.toHaveBeenCalled()
  })

  it("useShiftOverlaps with undefined operatorId keys under [shifts, overlaps, all]", async () => {
    mockOnce([] as ShiftOverlapPair[])
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useShiftOverlaps(undefined), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_list_overlaps", {
      args: { operator_id: undefined },
    })
    expect(client.getQueryData(["shifts", "overlaps", "all"])).toEqual([])
  })

  it("useShiftOverlaps with operatorId keys under [shifts, overlaps, op-id]", async () => {
    mockOnce([])
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useShiftOverlaps("op-1"), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_list_overlaps", {
      args: { operator_id: "op-1" },
    })
    expect(client.getQueryData(["shifts", "overlaps", "op-1"])).toEqual([])
  })

  it("useShiftClockIn passes note: null when caller omits note", async () => {
    mockOnce(shiftFixture())
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useShiftClockIn(), { wrapper })
    result.current.mutate({ operator_id: UUID_OPERATOR })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_clock_in", {
      args: { operator_id: UUID_OPERATOR, note: null },
    })
  })

  it("useShiftClockIn invalidates all shifts keys on success", async () => {
    mockOnce(shiftFixture())
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useShiftClockIn(), { wrapper })
    result.current.mutate({ operator_id: UUID_OPERATOR })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: shiftKeys.all })
  })

  it("useShiftClockIn surfaces a typed AppError to the caller without invalidating", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "CONFLICT_PARKED",
      message: "operator already has an open shift",
    } as never)
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useShiftClockIn(), { wrapper })
    result.current.mutate({ operator_id: UUID_OPERATOR })
    await waitFor(() => expect(result.current.isError).toBe(true))
    const err = result.current.error as unknown as { code: string; message: string }
    expect(err.code).toBe("CONFLICT_PARKED")
    expect(invalidateSpy).not.toHaveBeenCalled()
  })

  it("useShiftClockOut dispatches `shifts_clock_out` and invalidates shifts keys", async () => {
    mockOnce(shiftFixture({ check_out_at: "2026-05-14T12:00:00Z" }))
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useShiftClockOut(), { wrapper })
    result.current.mutate({ shift_id: UUID_SHIFT })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_clock_out", {
      args: { shift_id: UUID_SHIFT },
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: shiftKeys.all })
  })

  it("useShiftEdit sends note: { value: null } verbatim when the caller clears the note", async () => {
    mockOnce(shiftFixture())
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useShiftEdit(), { wrapper })
    const payload = {
      shift_id: UUID_SHIFT,
      check_in_at: "2026-05-14T10:00:00Z",
      check_out_at: null,
      note: { value: null },
    } as const
    result.current.mutate(payload)
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_edit", { args: payload })
  })

  it("useShiftEdit invalidates all shifts keys on success (P04-G27)", async () => {
    mockOnce(shiftFixture())
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useShiftEdit(), { wrapper })
    result.current.mutate({
      shift_id: UUID_SHIFT,
      check_in_at: "2026-05-14T10:00:00Z",
    })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: shiftKeys.all })
  })

  it("useShiftSoftDelete dispatches `shifts_soft_delete` and invalidates the cache", async () => {
    mockOnce(null)
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useShiftSoftDelete(), { wrapper })
    result.current.mutate({ shift_id: UUID_SHIFT, reason: "duplicate" })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("shifts_soft_delete", {
      args: { shift_id: UUID_SHIFT, reason: "duplicate" },
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: shiftKeys.all })
  })
})
