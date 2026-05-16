// Phase-06 §2.4 React Query hook tests for the inventory feature surface.
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
  InventoryAdjustmentRecord,
  InventoryItemDetailRecord,
  InventoryItemWithStatusRecord,
} from "@/lib/ipc"
import {
  inventoryKeys,
  useInventoryAdjustmentCreate,
  useInventoryAdjustments,
  useInventoryItem,
  useInventoryItems,
  useInventoryRecompute,
} from "@/features/inventory/queries"

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

const UUID_ITEM = "0190f3a0-f1c0-7000-8000-0000000a0001"
const UUID_USER = "0190f3a0-f1c0-7000-8000-0000000b0002"
const UUID_ADJ = "0190f3a0-f1c0-7000-8000-0000000c0003"

function itemRecord(
  overrides: Partial<InventoryItemWithStatusRecord> = {}
): InventoryItemWithStatusRecord {
  return {
    id: UUID_ITEM,
    name_ar: "صنف",
    name_en: "Widget",
    unit: "pcs",
    quantity_on_hand: 5,
    low_stock_threshold: 3,
    is_active: true,
    status: "ok",
    updated_at: "2026-05-13T10:00:00Z",
    created_at: "2026-05-13T09:00:00Z",
    version: 1,
    dirty: true,
    last_synced_at: null,
    entity_id: "tenant-x",
    ...overrides,
  }
}

function adjustmentRecord(
  overrides: Partial<InventoryAdjustmentRecord> = {}
): InventoryAdjustmentRecord {
  return {
    id: UUID_ADJ,
    item_id: UUID_ITEM,
    delta: 5,
    reason: "receive",
    visit_id: null,
    note: null,
    by_user_id: UUID_USER,
    created_at: "2026-05-13T10:00:00Z",
    updated_at: "2026-05-13T10:00:00Z",
    version: 1,
    entity_id: "tenant-x",
    is_reversal: false,
    ...overrides,
  }
}

function detailRecord(): InventoryItemDetailRecord {
  return {
    item: itemRecord(),
    consumption_map: [],
    recent_adjustments: [adjustmentRecord()],
  }
}

const mockOnce = (value: unknown) => {
  vi.mocked(invoke).mockResolvedValueOnce(value as never)
}

describe.each(directions)("Phase-06 §2.4 inventory hooks (dir=%s)", (dir) => {
  beforeEach(() => {
    document.documentElement.dir = dir
    vi.mocked(invoke).mockReset()
    vi.mocked(isTauri).mockReturnValue(true)
  })

  afterEach(() => {
    document.documentElement.dir = ""
  })

  it("inventoryKeys exposes the documented cache key shapes", () => {
    expect(inventoryKeys.all).toEqual(["inventory"])
    expect(inventoryKeys.detail(UUID_ITEM)).toEqual([
      "inventory",
      "items",
      "detail",
      UUID_ITEM,
    ])
    expect(inventoryKeys.adjustments(UUID_ITEM)).toEqual([
      "inventory",
      "adjustments",
      UUID_ITEM,
    ])
    const listKey = inventoryKeys.list({ status: "low" })
    expect(listKey[0]).toBe("inventory")
    expect(listKey[1]).toBe("items")
    expect(listKey[2]).toBe("list")
    expect(listKey[3]).toEqual({ status: "low" })
  })

  it("inventoryKeys.list segments by filter shape (different filters -> different cache keys)", () => {
    const a = inventoryKeys.list({ status: "low" })
    const b = inventoryKeys.list({ status: "neg" })
    const c = inventoryKeys.list({})
    expect(a).not.toEqual(b)
    expect(a).not.toEqual(c)
    expect(b).not.toEqual(c)
  })

  it("useInventoryItems dispatches inventory_list_items and caches under list filter key", async () => {
    const fixture = [itemRecord({ status: "low" })]
    mockOnce(fixture)
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useInventoryItems({ status: "low" }), {
      wrapper,
    })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_list_items", {
      args: { status: "low", include_inactive: false, query: null },
    })
    expect(client.getQueryData(inventoryKeys.list({ status: "low" }))).toEqual(
      fixture
    )
  })

  it("useInventoryItems is disabled outside Tauri", () => {
    vi.mocked(isTauri).mockReturnValue(false)
    const { wrapper } = makeWrapper()
    renderHook(() => useInventoryItems(), { wrapper })
    expect(invoke).not.toHaveBeenCalled()
  })

  it("useInventoryItems trims and forwards a non-empty query", async () => {
    mockOnce([itemRecord()])
    const { wrapper } = makeWrapper()
    const { result } = renderHook(
      () => useInventoryItems({ query: "  lid  ", include_inactive: true }),
      { wrapper }
    )
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_list_items", {
      args: { status: undefined, include_inactive: true, query: "lid" },
    })
  })

  it("useInventoryItems collapses whitespace-only query to null", async () => {
    mockOnce([itemRecord()])
    const { wrapper } = makeWrapper()
    const { result } = renderHook(
      () => useInventoryItems({ query: "   " }),
      { wrapper }
    )
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_list_items", {
      args: { status: undefined, include_inactive: false, query: null },
    })
  })

  it("useInventoryItem dispatches inventory_get_item with the id arg", async () => {
    const fixture = detailRecord()
    mockOnce(fixture)
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useInventoryItem(UUID_ITEM), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_get_item", {
      args: { id: UUID_ITEM },
    })
    expect(client.getQueryData(inventoryKeys.detail(UUID_ITEM))).toEqual(fixture)
  })

  it("useInventoryItem is disabled when id is null/undefined", () => {
    const { wrapper } = makeWrapper()
    renderHook(() => useInventoryItem(null), { wrapper })
    renderHook(() => useInventoryItem(undefined), { wrapper })
    expect(invoke).not.toHaveBeenCalled()
  })

  it("useInventoryAdjustments dispatches inventory_list_adjustments with a default limit of 50", async () => {
    mockOnce([adjustmentRecord()])
    const { wrapper } = makeWrapper()
    const { result } = renderHook(
      () => useInventoryAdjustments(UUID_ITEM),
      { wrapper }
    )
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_list_adjustments", {
      args: { item_id: UUID_ITEM, limit: 50 },
    })
  })

  it("useInventoryAdjustments accepts a custom limit", async () => {
    mockOnce([adjustmentRecord()])
    const { wrapper } = makeWrapper()
    const { result } = renderHook(
      () => useInventoryAdjustments(UUID_ITEM, 200),
      { wrapper }
    )
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_list_adjustments", {
      args: { item_id: UUID_ITEM, limit: 200 },
    })
  })

  it("useInventoryAdjustments is disabled when itemId is absent", () => {
    const { wrapper } = makeWrapper()
    renderHook(() => useInventoryAdjustments(null), { wrapper })
    expect(invoke).not.toHaveBeenCalled()
  })

  it("useInventoryAdjustmentCreate forwards reason/delta and invalidates the inventory tree", async () => {
    mockOnce(adjustmentRecord({ reason: "writeoff", delta: -3 }))
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useInventoryAdjustmentCreate(), {
      wrapper,
    })
    result.current.mutate({
      item_id: UUID_ITEM,
      reason: "writeoff",
      delta: -3,
      note: "damaged",
    })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_create_adjustment", {
      args: {
        item_id: UUID_ITEM,
        reason: "writeoff",
        delta: -3,
        note: "damaged",
      },
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: inventoryKeys.all })
  })

  it("useInventoryAdjustmentCreate sends note: null when caller omits it", async () => {
    mockOnce(adjustmentRecord())
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useInventoryAdjustmentCreate(), {
      wrapper,
    })
    result.current.mutate({
      item_id: UUID_ITEM,
      reason: "receive",
      delta: 5,
    })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_create_adjustment", {
      args: { item_id: UUID_ITEM, reason: "receive", delta: 5, note: null },
    })
  })

  it("useInventoryAdjustmentCreate surfaces a typed AppError without invalidating", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "VALIDATION_ERROR",
      message: "this action requires one of: [Superadmin]",
    } as never)
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useInventoryAdjustmentCreate(), {
      wrapper,
    })
    result.current.mutate({
      item_id: UUID_ITEM,
      reason: "count_correction",
      delta: -1,
    })
    await waitFor(() => expect(result.current.isError).toBe(true))
    const err = result.current.error as unknown as {
      code: string
      message: string
    }
    expect(err.code).toBe("VALIDATION_ERROR")
    expect(err.message).toContain("Superadmin")
    expect(invalidateSpy).not.toHaveBeenCalled()
  })

  it("useInventoryRecompute dispatches inventory_recompute_on_hand and invalidates the inventory tree", async () => {
    mockOnce({ new_on_hand: 12 })
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useInventoryRecompute(), { wrapper })
    result.current.mutate({ item_id: UUID_ITEM })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("inventory_recompute_on_hand", {
      args: { item_id: UUID_ITEM },
    })
    expect(result.current.data).toEqual({ new_on_hand: 12 })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: inventoryKeys.all })
  })

  it("useInventoryRecompute surfaces forbidden as typed AppError", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "VALIDATION_ERROR",
      message: "this action requires one of: [Superadmin]",
    } as never)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useInventoryRecompute(), { wrapper })
    result.current.mutate({ item_id: UUID_ITEM })
    await waitFor(() => expect(result.current.isError).toBe(true))
    const err = result.current.error as unknown as {
      code: string
      message: string
    }
    expect(err.code).toBe("VALIDATION_ERROR")
  })
})
