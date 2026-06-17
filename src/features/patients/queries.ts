import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri } from "@/lib/ipc"
import type {
  DuplicateGroupRecord,
  PatientRecord,
  PatientSortLiteral,
  PatientStatsRecord,
  PatientVisitSummary,
} from "@/lib/ipc"

export interface PatientListFilter {
  query?: string
  includeDeleted?: boolean
  sort?: PatientSortLiteral
  limit?: number
  offset?: number
}

export const patientArchiveKeys = {
  all: ["patients", "archive"] as const,
  list: (filter: PatientListFilter) =>
    ["patients", "archive", "list", filter] as const,
  detail: (id: string) => ["patients", "archive", "detail", id] as const,
  visits: (id: string, page: { limit?: number; offset?: number }) =>
    ["patients", "archive", "visits", id, page] as const,
  stats: (id: string) => ["patients", "archive", "stats", id] as const,
  duplicates: ["patients", "archive", "duplicates"] as const,
}

// The new-visit combobox keys its results under ["patients","search"]; invalidate
// it alongside the archive so a rename/merge/restore there stays consistent.
const SEARCH_ROOT = ["patients", "search"] as const

export function usePatientsList (filter: PatientListFilter = {}) {
  return useQuery<PatientRecord[]>({
    queryKey: patientArchiveKeys.list(filter),
    enabled: isTauri(),
    queryFn: () =>
      invoke("patients_list", {
        args: {
          query: filter.query,
          include_deleted: filter.includeDeleted ?? false,
          sort: filter.sort,
          limit: filter.limit ?? 50,
          offset: filter.offset ?? 0,
        },
      }),
    staleTime: 30_000,
  })
}

export function usePatientDetail (id: string | null | undefined) {
  return useQuery<PatientRecord>({
    queryKey: patientArchiveKeys.detail(id ?? ""),
    enabled: isTauri() && Boolean(id),
    queryFn: () => invoke("patients_get", { args: { id: id! } }),
  })
}

export function usePatientVisits (
  id: string | null | undefined,
  page: { limit?: number; offset?: number } = {}
) {
  return useQuery<PatientVisitSummary[]>({
    queryKey: patientArchiveKeys.visits(id ?? "", page),
    enabled: isTauri() && Boolean(id),
    queryFn: () =>
      invoke("patients_list_visits", {
        args: { id: id!, limit: page.limit ?? 50, offset: page.offset ?? 0 },
      }),
    staleTime: 15_000,
  })
}

export function usePatientStats (id: string | null | undefined) {
  return useQuery<PatientStatsRecord>({
    queryKey: patientArchiveKeys.stats(id ?? ""),
    enabled: isTauri() && Boolean(id),
    queryFn: () => invoke("patients_stats", { args: { id: id! } }),
    staleTime: 15_000,
  })
}

export function usePatientDuplicates (enabled = true) {
  return useQuery<DuplicateGroupRecord[]>({
    queryKey: patientArchiveKeys.duplicates,
    enabled: isTauri() && enabled,
    queryFn: () => invoke("patients_find_duplicates"),
    staleTime: 30_000,
  })
}

export interface DemographicsInput {
  id: string
  phone?: string | null
  sex?: "M" | "F" | null
  birth_date?: string | null
  file_no?: string | null
  notes?: string | null
}

export function usePatientUpdateDemographics () {
  const qc = useQueryClient()
  return useMutation<PatientRecord, Error, DemographicsInput>({
    mutationFn: (input) =>
      invoke("patients_update_demographics", { args: input }),
    onSettled: (_data, _err, vars) => {
      void qc.invalidateQueries({ queryKey: patientArchiveKeys.all })
      void qc.invalidateQueries({ queryKey: patientArchiveKeys.detail(vars.id) })
      void qc.invalidateQueries({ queryKey: SEARCH_ROOT })
    },
  })
}

export function usePatientSoftDelete () {
  const qc = useQueryClient()
  return useMutation<null, Error, { id: string }>({
    mutationFn: (input) => invoke("patients_soft_delete", { args: input }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: patientArchiveKeys.all })
      void qc.invalidateQueries({ queryKey: SEARCH_ROOT })
    },
  })
}

export function usePatientRestore () {
  const qc = useQueryClient()
  return useMutation<PatientRecord, Error, { id: string }>({
    mutationFn: (input) => invoke("patients_restore", { args: input }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: patientArchiveKeys.all })
      void qc.invalidateQueries({ queryKey: SEARCH_ROOT })
    },
  })
}

export function usePatientMerge () {
  const qc = useQueryClient()
  return useMutation<null, Error, { survivor_id: string; merged_id: string }>({
    mutationFn: (input) => invoke("patients_merge", { args: input }),
    onSettled: () => {
      // A merge re-points visits and tombstones a patient: invalidate the
      // whole archive, the search index, and any visit caches.
      void qc.invalidateQueries({ queryKey: patientArchiveKeys.all })
      void qc.invalidateQueries({ queryKey: SEARCH_ROOT })
      void qc.invalidateQueries({ queryKey: ["visits"] })
    },
  })
}
