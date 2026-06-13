import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { inventoryKeys } from "@/features/inventory/queries"
import { invoke, isTauri } from "@/lib/ipc"
import type {
  ChecksGridCardRecord,
  LockResultRecord,
  PatientRecord,
  QualifiedOperatorRecord,
  ReceiptArtifactsRecord,
  VisitRecord,
} from "@/lib/ipc"
import type {
  VisitCreateDraftInput,
  VisitLockInput,
  VisitUpdateDraftInput,
  VisitVoidInput,
} from "@/lib/schemas/visit"

export const visitKeys = {
  all: ["visits"] as const,
  checksGrid: ["visits", "checks-grid"] as const,
  todayByCheck: (checkTypeId: string) =>
    ["visits", "today-by-check", checkTypeId] as const,
  draftsByCheck: (checkTypeId: string) =>
    ["visits", "drafts-by-check", checkTypeId] as const,
  workspace: (checkTypeId: string) =>
    ["visits", "workspace", checkTypeId] as const,
  detail: (id: string) => ["visits", "detail", id] as const,
  qualifiedOperators: (checkTypeId: string) =>
    ["visits", "qualified-operators", checkTypeId] as const,
  receipts: (id: string) => ["visits", "receipts", id] as const,
} as const

export function useChecksGrid () {
  return useQuery<ChecksGridCardRecord[]>({
    queryKey: visitKeys.checksGrid,
    enabled: isTauri(),
    queryFn: () => invoke("visits_checks_grid"),
    staleTime: 30_000,
  })
}

export function useVisitsTodayByCheck (checkTypeId: string | null | undefined) {
  return useQuery<VisitRecord[]>({
    queryKey: visitKeys.todayByCheck(checkTypeId ?? ""),
    enabled: isTauri() && Boolean(checkTypeId),
    queryFn: () =>
      invoke("visits_list_today_by_check", {
        args: { check_type_id: checkTypeId! },
      }),
    staleTime: 15_000,
  })
}

export function useVisitsWorkspace (
  checkTypeId: string | null | undefined,
  filters: {
    statuses?: string[]
    doctor_ids?: string[]
    subtype_ids?: string[]
    limit?: number
  } = {}
) {
  return useQuery<VisitRecord[]>({
    queryKey: [...visitKeys.workspace(checkTypeId ?? ""), filters],
    enabled: isTauri() && Boolean(checkTypeId),
    queryFn: () =>
      invoke("visits_list_workspace", {
        args: { check_type_id: checkTypeId!, ...filters },
      }),
    staleTime: 15_000,
  })
}

export function useVisit (visitId: string | null | undefined) {
  return useQuery<VisitRecord>({
    queryKey: visitKeys.detail(visitId ?? ""),
    enabled: isTauri() && Boolean(visitId),
    queryFn: () => invoke("visits_get", { args: { visit_id: visitId! } }),
  })
}

export function useQualifiedOperators (
  checkTypeId: string | null | undefined
) {
  return useQuery<QualifiedOperatorRecord[]>({
    queryKey: visitKeys.qualifiedOperators(checkTypeId ?? ""),
    enabled: isTauri() && Boolean(checkTypeId),
    queryFn: () =>
      invoke("visits_qualified_operators", {
        args: { check_type_id: checkTypeId! },
      }),
    staleTime: 5_000,
  })
}

export function useVisitCreateDraft () {
  const qc = useQueryClient()
  return useMutation<VisitRecord, Error, VisitCreateDraftInput>({
    mutationFn: (input) =>
      invoke("visits_create_draft", {
        args: {
          patient_id: input.patient_id,
          check_type_id: input.check_type_id,
          check_subtype_id: input.check_subtype_id ?? null,
          doctor_id: input.doctor_id ?? null,
          dye: input.dye ?? false,
          report: input.report ?? false,
        },
      }),
    // Invalidate on settle (success AND error): a failed mutation is exactly
    // when local optimistic state may have diverged from the server, so the
    // cache must refetch regardless of outcome.
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: visitKeys.all })
    },
  })
}

export function useVisitUpdateDraft () {
  const qc = useQueryClient()
  return useMutation<VisitRecord, Error, VisitUpdateDraftInput>({
    mutationFn: (input) =>
      invoke("visits_update_draft", {
        args: {
          visit_id: input.visit_id,
          patient_id: input.patient_id,
          check_subtype_id: input.check_subtype_id,
          doctor_id: input.doctor_id,
          dye: input.dye,
          report: input.report,
        },
      }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: visitKeys.all })
    },
  })
}

export function useVisitDiscard () {
  const qc = useQueryClient()
  return useMutation<null, Error, { visit_id: string }>({
    mutationFn: (input) =>
      invoke("visits_discard", { args: { visit_id: input.visit_id } }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: visitKeys.all })
    },
  })
}

export function useVisitLock () {
  const qc = useQueryClient()
  return useMutation<LockResultRecord, Error, VisitLockInput>({
    mutationFn: (input) =>
      invoke("visits_lock", {
        args: { visit_id: input.visit_id, operator_id: input.operator_id },
      }),
    // Locking consumes inventory server-side (consume-on-lock), so the
    // inventory views must refetch too. Invalidate on settle so a failed
    // lock still reconciles any partially-applied local state.
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: visitKeys.all })
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
    },
  })
}

export function useVisitVoid () {
  const qc = useQueryClient()
  return useMutation<VisitRecord, Error, VisitVoidInput>({
    mutationFn: (input) =>
      invoke("visits_void", {
        args: { visit_id: input.visit_id, reason: input.reason },
      }),
    // Voiding writes offsetting inventory adjustments server-side
    // (offset-on-void), so the inventory views must refetch too. Invalidate
    // on settle so a failed void still reconciles local state.
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: visitKeys.all })
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
    },
  })
}

export function useReceiptReprint () {
  return useMutation<ReceiptArtifactsRecord, Error, { visit_id: string }>({
    mutationFn: (input) =>
      invoke("receipts_reprint", { args: { visit_id: input.visit_id } }),
  })
}

// ---- Patients ----

export const patientKeys = {
  search: (query: string) => ["patients", "search", query] as const,
}

export function usePatientSearch (query: string) {
  return useQuery<PatientRecord[]>({
    queryKey: patientKeys.search(query),
    enabled: isTauri(),
    queryFn: () =>
      invoke("patients_search", { args: { query, limit: 20 } }),
    staleTime: 15_000,
  })
}

export function usePatientCreate () {
  const qc = useQueryClient()
  return useMutation<PatientRecord, Error, { name: string }>({
    mutationFn: (input) =>
      invoke("patients_create", { args: { name: input.name } }),
    // Invalidate every patient search so a freshly-created patient shows up
    // immediately and a second commit of the same name resolves the existing
    // row instead of creating a duplicate. On settle (not just success) so a
    // failed create still reconciles any stale search cache.
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["patients", "search"] })
    },
  })
}
