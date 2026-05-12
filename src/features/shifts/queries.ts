import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri } from "@/lib/ipc"
import type {
  ShiftOverlapPair,
  ShiftRecord,
  ShiftWithMetaRecord,
} from "@/lib/ipc"

export const shiftKeys = {
  all: ["shifts"] as const,
  open: ["shifts", "open"] as const,
  historyToday: ["shifts", "today"] as const,
  overlaps: (operatorId?: string) =>
    ["shifts", "overlaps", operatorId ?? "all"] as const,
} as const

export function useOpenShifts () {
  return useQuery({
    queryKey: shiftKeys.open,
    enabled: isTauri(),
    queryFn: () => invoke("shifts_list_open"),
    staleTime: 30_000,
  })
}

export function useShiftHistoryToday () {
  return useQuery({
    queryKey: shiftKeys.historyToday,
    enabled: isTauri(),
    queryFn: () => invoke("shifts_history_today"),
    staleTime: 60_000,
  })
}

export function useShiftOverlaps (operatorId?: string) {
  return useQuery({
    queryKey: shiftKeys.overlaps(operatorId),
    enabled: isTauri(),
    queryFn: () =>
      invoke("shifts_list_overlaps", { args: { operator_id: operatorId } }),
  })
}

export function useShiftClockIn () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { operator_id: string; note?: string | null }) =>
      invoke("shifts_clock_in", {
        args: { operator_id: input.operator_id, note: input.note ?? null },
      }),
    onSuccess: (_data: ShiftRecord) => {
      void qc.invalidateQueries({ queryKey: shiftKeys.all })
    },
  })
}

export function useShiftClockOut () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { shift_id: string }) =>
      invoke("shifts_clock_out", { args: { shift_id: input.shift_id } }),
    onSuccess: (_data: ShiftRecord) => {
      void qc.invalidateQueries({ queryKey: shiftKeys.all })
    },
  })
}

export function useShiftEdit () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: {
      shift_id: string
      check_in_at: string
      check_out_at?: string | null
      note?: { value: string | null } | null
    }) => invoke("shifts_edit", { args: input }),
    onSuccess: (_data: ShiftRecord) => {
      void qc.invalidateQueries({ queryKey: shiftKeys.all })
    },
  })
}

export function useShiftSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { shift_id: string; reason: string }) =>
      invoke("shifts_soft_delete", { args: input }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: shiftKeys.all })
    },
  })
}

export type ShiftListResult = ShiftWithMetaRecord[]
export type OverlapsResult = ShiftOverlapPair[]
