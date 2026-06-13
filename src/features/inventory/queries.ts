import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri } from "@/lib/ipc"
import type {
  AdjustmentReasonLiteral,
  InventoryAdjustmentRecord,
  InventoryItemDetailRecord,
  InventoryItemWithStatusRecord,
  StockStatusLiteral,
} from "@/lib/ipc"

// Inventory items live in TWO parallel cache namespaces: these operations
// hooks (`["inventory", ...]`) and the catalog-scoped hooks in
// features/catalog (`["catalog", "inventory", ...]`). Stock-changing
// mutations here must also invalidate the catalog root so the two namespaces
// stay coherent. Use a literal to avoid a circular import with catalog.
const CATALOG_ROOT = ["catalog"] as const

export const inventoryKeys = {
  all: ["inventory"] as const,
  list: (filter: {
    status?: StockStatusLiteral | null
    include_inactive?: boolean
    query?: string
  }) => ["inventory", "items", "list", filter] as const,
  detail: (id: string) => ["inventory", "items", "detail", id] as const,
  adjustments: (itemId: string) =>
    ["inventory", "adjustments", itemId] as const,
} as const

export function useInventoryItems (filter: {
  status?: StockStatusLiteral | null
  include_inactive?: boolean
  query?: string
} = {}) {
  return useQuery<InventoryItemWithStatusRecord[]>({
    queryKey: inventoryKeys.list(filter),
    enabled: isTauri(),
    queryFn: () =>
      invoke("inventory_list_items", {
        args: {
          status: filter.status ?? undefined,
          include_inactive: filter.include_inactive ?? false,
          query: filter.query?.trim() ? filter.query.trim() : null,
        },
      }),
    staleTime: 15_000,
  })
}

export function useInventoryItem (id: string | null | undefined) {
  return useQuery<InventoryItemDetailRecord>({
    queryKey: inventoryKeys.detail(id ?? ""),
    enabled: isTauri() && Boolean(id),
    queryFn: () => invoke("inventory_get_item", { args: { id: id! } }),
    staleTime: 5_000,
  })
}

export function useInventoryAdjustments (
  itemId: string | null | undefined,
  limit = 50
) {
  return useQuery<InventoryAdjustmentRecord[]>({
    queryKey: [...inventoryKeys.adjustments(itemId ?? ""), limit],
    enabled: isTauri() && Boolean(itemId),
    queryFn: () =>
      invoke("inventory_list_adjustments", {
        args: { item_id: itemId!, limit },
      }),
    staleTime: 5_000,
  })
}

export interface InventoryAdjustmentCreateInput {
  item_id: string
  reason: AdjustmentReasonLiteral
  delta: number
  note?: string | null
}

export function useInventoryAdjustmentCreate () {
  const qc = useQueryClient()
  return useMutation<
    InventoryAdjustmentRecord,
    Error,
    InventoryAdjustmentCreateInput
  >({
    mutationFn: (input) =>
      invoke("inventory_create_adjustment", {
        args: {
          item_id: input.item_id,
          reason: input.reason,
          delta: input.delta,
          note: input.note ?? null,
        },
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
      void qc.invalidateQueries({ queryKey: CATALOG_ROOT })
    },
  })
}

export function useInventoryRecompute () {
  const qc = useQueryClient()
  return useMutation<{ new_on_hand: number }, Error, { item_id: string }>({
    mutationFn: (input) =>
      invoke("inventory_recompute_on_hand", {
        args: { item_id: input.item_id },
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
      void qc.invalidateQueries({ queryKey: CATALOG_ROOT })
    },
  })
}
