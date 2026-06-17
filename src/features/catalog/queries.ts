import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { inventoryKeys } from "@/features/inventory/queries"
import { invoke, isTauri } from "@/lib/ipc"
import type {
  CheckSubtypeCreateArgs,
  CheckSubtypeRecord,
  CheckSubtypeUpdateArgs,
  CheckTypeCreateArgs,
  CheckTypeRecord,
  CheckTypeUpdateArgs,
  ConsumptionCreateArgs,
  ConsumptionRecord,
  DoctorCreateArgs,
  DoctorPricingRecord,
  DoctorPricingUpsertArgs,
  DoctorRecord,
  DoctorUpdateArgs,
  DuplicateDoctorGroupRecord,
  InventoryItemCreateArgs,
  InventoryItemRecord,
  InventoryItemUpdateArgs,
  OperatorCreateArgs,
  OperatorRecord,
  OperatorSpecialtyRecord,
  OperatorUpdateArgs,
} from "@/lib/ipc"

export const catalogKeys = {
  checkTypes: ["catalog", "checkTypes"] as const,
  checkType: (id: string) => ["catalog", "checkTypes", id] as const,
  checkSubtypes: (typeId: string) => ["catalog", "checkSubtypes", typeId] as const,
  doctors: ["catalog", "doctors"] as const,
  doctor: (id: string) => ["catalog", "doctors", id] as const,
  doctorDuplicates: ["catalog", "doctors", "duplicates"] as const,
  operators: ["catalog", "operators"] as const,
  operator: (id: string) => ["catalog", "operators", id] as const,
  inventoryItems: ["catalog", "inventory"] as const,
  inventoryItem: (id: string) => ["catalog", "inventory", id] as const,
  consumptionByType: (id: string) => ["catalog", "consumption", "type", id] as const,
} as const

// ---- check_types --------------------------------------------------------

export function useCheckTypes (params: { include_inactive?: boolean; query?: string } = {}) {
  return useQuery({
    queryKey: [...catalogKeys.checkTypes, params] as const,
    enabled: isTauri(),
    queryFn: () => invoke("check_types_list", { args: params }),
  })
}

export function useCheckType (id: string | null) {
  return useQuery({
    queryKey: id ? catalogKeys.checkType(id) : ["catalog", "checkTypes", "none"],
    enabled: !!id && isTauri(),
    queryFn: async (): Promise<CheckTypeRecord> => invoke("check_types_get", { args: { id: id! } }),
  })
}

export function useCheckSubtypes (typeId: string | null) {
  return useQuery({
    queryKey: typeId ? catalogKeys.checkSubtypes(typeId) : ["catalog", "checkSubtypes", "none"],
    enabled: !!typeId && isTauri(),
    queryFn: async (): Promise<CheckSubtypeRecord[]> =>
      invoke("check_subtypes_list_by_type", { args: { check_type_id: typeId! } }),
  })
}

export function useCheckTypeCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: CheckTypeCreateArgs) => invoke("check_types_create", { args: input }),
    onSuccess: () => qc.invalidateQueries({ queryKey: catalogKeys.checkTypes }),
  })
}

export function useCheckTypeUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: CheckTypeUpdateArgs) => invoke("check_types_update", { args: input }),
    onSuccess: (data: CheckTypeRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.checkTypes })
      void qc.invalidateQueries({ queryKey: catalogKeys.checkType(data.id) })
    },
  })
}

export function useCheckTypeToggleSubtypes () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; to_value: boolean; base_price_iqd?: number | null }) =>
      invoke("check_types_toggle_subtypes", { args: input }),
    onSuccess: (data: CheckTypeRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.checkTypes })
      void qc.invalidateQueries({ queryKey: catalogKeys.checkType(data.id) })
    },
  })
}

export function useCheckTypeSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => invoke("check_types_soft_delete", { args: { id } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: catalogKeys.checkTypes }),
  })
}

// ---- check_subtypes -----------------------------------------------------

export function useCheckSubtypeCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: CheckSubtypeCreateArgs) => invoke("check_subtypes_create", { args: input }),
    onSuccess: (data: CheckSubtypeRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.checkSubtypes(data.check_type_id) })
    },
  })
}

export function useCheckSubtypeUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: CheckSubtypeUpdateArgs) => invoke("check_subtypes_update", { args: input }),
    onSuccess: (data: CheckSubtypeRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.checkSubtypes(data.check_type_id) })
    },
  })
}

export function useCheckSubtypeSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; check_type_id: string }) =>
      invoke("check_subtypes_soft_delete", { args: { id: input.id } }),
    onSuccess: (_, vars) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.checkSubtypes(vars.check_type_id) })
    },
  })
}

// ---- doctors ------------------------------------------------------------

export function useDoctors (params: { include_inactive?: boolean; query?: string } = {}) {
  return useQuery({
    queryKey: [...catalogKeys.doctors, params] as const,
    enabled: isTauri(),
    queryFn: () => invoke("doctors_list", { args: params }),
  })
}

export function useDoctor (id: string | null) {
  return useQuery({
    queryKey: id ? catalogKeys.doctor(id) : ["catalog", "doctors", "none"],
    enabled: !!id && isTauri(),
    queryFn: async (): Promise<{ doctor: DoctorRecord; pricings: DoctorPricingRecord[] }> =>
      invoke("doctors_get", { args: { id: id! } }),
  })
}

export function useDoctorCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: DoctorCreateArgs) => invoke("doctors_create", { args: input }),
    onSuccess: () => qc.invalidateQueries({ queryKey: catalogKeys.doctors }),
  })
}

export function useDoctorUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: DoctorUpdateArgs) => invoke("doctors_update", { args: input }),
    onSuccess: (data: DoctorRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.doctors })
      void qc.invalidateQueries({ queryKey: catalogKeys.doctor(data.id) })
    },
  })
}

export function useDoctorSetActive () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; is_active: boolean }) =>
      invoke("doctors_set_active", { args: input }),
    onSuccess: (data: DoctorRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.doctors })
      void qc.invalidateQueries({ queryKey: catalogKeys.doctor(data.id) })
    },
  })
}

export function useDoctorSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => invoke("doctors_soft_delete", { args: { id } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: catalogKeys.doctors }),
  })
}

export function useDoctorDuplicates (enabled = true) {
  return useQuery({
    queryKey: catalogKeys.doctorDuplicates,
    enabled: enabled && isTauri(),
    queryFn: (): Promise<DuplicateDoctorGroupRecord[]> => invoke("doctors_find_duplicates"),
  })
}

/**
 * Look up live doctors whose digit-only phone matches `phone`, excluding one id
 * (the doctor being edited). Used to warn before saving a duplicate phone. The
 * query is disabled for an empty phone so we never round-trip for nothing.
 */
export function useDoctorsWithPhone (phone: string, excludeId?: string | null) {
  const trimmed = phone.trim()
  return useQuery({
    queryKey: [...catalogKeys.doctors, "byPhone", trimmed, excludeId ?? null] as const,
    enabled: trimmed.length > 0 && isTauri(),
    queryFn: (): Promise<DoctorRecord[]> =>
      invoke("doctors_with_phone", { args: { phone: trimmed, exclude_id: excludeId ?? null } }),
  })
}

export function useDoctorPricingUpsert () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: DoctorPricingUpsertArgs) =>
      invoke("doctor_pricing_upsert", { args: input }),
    onSuccess: (data: DoctorPricingRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.doctor(data.doctor_id) })
    },
  })
}

export function useDoctorPricingSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; doctor_id: string }) =>
      invoke("doctor_pricing_soft_delete", { args: { id: input.id } }),
    onSuccess: (_, vars) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.doctor(vars.doctor_id) })
    },
  })
}

// ---- operators ----------------------------------------------------------

export function useOperators (params: { include_inactive?: boolean; query?: string } = {}) {
  return useQuery({
    queryKey: [...catalogKeys.operators, params] as const,
    enabled: isTauri(),
    queryFn: () => invoke("operators_list", { args: params }),
  })
}

export function useOperator (id: string | null) {
  return useQuery({
    queryKey: id ? catalogKeys.operator(id) : ["catalog", "operators", "none"],
    enabled: !!id && isTauri(),
    queryFn: async (): Promise<{ operator: OperatorRecord; specialties: OperatorSpecialtyRecord[] }> =>
      invoke("operators_get", { args: { id: id! } }),
  })
}

export function useOperatorCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: OperatorCreateArgs) => invoke("operators_create", { args: input }),
    onSuccess: () => qc.invalidateQueries({ queryKey: catalogKeys.operators }),
  })
}

export function useOperatorUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: OperatorUpdateArgs) => invoke("operators_update", { args: input }),
    onSuccess: (data: OperatorRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.operators })
      void qc.invalidateQueries({ queryKey: catalogKeys.operator(data.id) })
    },
  })
}

export function useOperatorSetActive () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; is_active: boolean }) =>
      invoke("operators_set_active", { args: input }),
    onSuccess: (data: OperatorRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.operators })
      void qc.invalidateQueries({ queryKey: catalogKeys.operator(data.id) })
    },
  })
}

export function useOperatorSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => invoke("operators_soft_delete", { args: { id } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: catalogKeys.operators }),
  })
}

export function useOperatorSpecialtyUpsert () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { operator_id: string; check_type_id: string }) =>
      invoke("operator_specialties_upsert", { args: input }),
    onSuccess: (data: OperatorSpecialtyRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.operator(data.operator_id) })
    },
  })
}

export function useOperatorSpecialtySoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; operator_id: string }) =>
      invoke("operator_specialties_soft_delete", { args: { id: input.id } }),
    onSuccess: (_, vars) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.operator(vars.operator_id) })
    },
  })
}

// ---- inventory items + consumption -------------------------------------

export function useInventoryItems (params: { include_inactive?: boolean; query?: string } = {}) {
  return useQuery({
    queryKey: [...catalogKeys.inventoryItems, params] as const,
    enabled: isTauri(),
    queryFn: () => invoke("inventory_catalog_list", { args: params }),
  })
}

export function useInventoryItem (id: string | null) {
  return useQuery({
    queryKey: id ? catalogKeys.inventoryItem(id) : ["catalog", "inventory", "none"],
    enabled: !!id && isTauri(),
    queryFn: async (): Promise<{ item: InventoryItemRecord; consumption: ConsumptionRecord[] }> =>
      invoke("inventory_catalog_get", { args: { id: id! } }),
  })
}

// Inventory items live in TWO parallel cache namespaces: the catalog-scoped
// hooks here (`["catalog", "inventory", ...]`) and the operations-scoped hooks
// in features/inventory (`["inventory", ...]`). Mutating via either path must
// invalidate BOTH roots so the namespaces stay coherent.
export function useInventoryItemCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: InventoryItemCreateArgs) =>
      invoke("inventory_catalog_create", { args: input }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: catalogKeys.inventoryItems })
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
    },
  })
}

export function useInventoryItemUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: InventoryItemUpdateArgs) =>
      invoke("inventory_catalog_update", { args: input }),
    onSuccess: (data: InventoryItemRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.inventoryItems })
      void qc.invalidateQueries({ queryKey: catalogKeys.inventoryItem(data.id) })
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
    },
  })
}

export function useInventoryItemSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => invoke("inventory_catalog_soft_delete", { args: { id } }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: catalogKeys.inventoryItems })
      void qc.invalidateQueries({ queryKey: inventoryKeys.all })
    },
  })
}

export function useConsumptionCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: ConsumptionCreateArgs) =>
      invoke("inventory_consumption_create", { args: input }),
    onSuccess: (data: ConsumptionRecord) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.inventoryItem(data.item_id) })
      void qc.invalidateQueries({ queryKey: catalogKeys.consumptionByType(data.check_type_id) })
    },
  })
}

export function useConsumptionSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { id: string; item_id: string; check_type_id: string }) =>
      invoke("inventory_consumption_soft_delete", { args: { id: input.id } }),
    onSuccess: (_, vars) => {
      void qc.invalidateQueries({ queryKey: catalogKeys.inventoryItem(vars.item_id) })
      void qc.invalidateQueries({ queryKey: catalogKeys.consumptionByType(vars.check_type_id) })
    },
  })
}
