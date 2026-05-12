import { invoke as tauriInvoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

/**
 * Typed wrapper around Tauri's `invoke<>()`.
 *
 * Every IPC command goes through this helper so the TypeScript compiler can
 * enforce arg shape and return shape from the command map below.
 */
export type CommandMap = {
  sync_status: { args: void; result: SyncStatusSnapshot }
  sync_outbox_count: { args: void; result: number }
  sync_trigger_push: { args: void; result: null }
  sync_trigger_pull: { args: void; result: null }
  sync_list_conflicts: {
    args: { limit?: number; offset?: number }
    result: ConflictRecord[]
  }
  sync_resolve_conflict: {
    args: { args: { opId: string; choice: "local" | "server" | "merged"; merged?: unknown } }
    result: null
  }
  device_info: { args: void; result: DeviceInfo }
  config_set_sync_server_url: { args: { url: string }; result: null }
  config_get_sync_server_url: { args: void; result: string | null }
  // Auth
  auth_login: {
    args: { args: { email: string; password: string; entity_id_hint?: string } }
    result: AuthLoginResult
  }
  auth_logout: { args: void; result: null }
  auth_current_user: { args: void; result: AuthUserContext | null }
  auth_lock: { args: void; result: null }
  auth_unlock: { args: { args: { password: string } }; result: null }
  auth_is_locked: { args: void; result: boolean }
  // Users
  users_list: { args: { args: { include_inactive?: boolean } }; result: UserAdminRecord[] }
  users_get: { args: { args: { id: string } }; result: UserAdminRecord }
  users_create: {
    args: { args: { email: string; name: string; role: UserRoleLiteral; password: string } }
    result: UserAdminRecord
  }
  users_update: {
    args: { args: { id: string; email?: string; name?: string; role?: UserRoleLiteral } }
    result: UserAdminRecord
  }
  users_soft_delete: { args: { args: { id: string } }; result: null }
  users_reset_password: { args: { args: { id: string; new_password: string } }; result: null }
  users_create_first_admin: {
    args: { args: { email: string; name: string; password: string; entity_id?: string } }
    result: UserAdminRecord
  }
  // Settings
  settings_list: { args: void; result: SettingRecord[] }
  settings_get: { args: { args: { key: string } }; result: SettingRecord | null }
  settings_update: {
    args: { args: { key: string; value: SettingValueWire } }
    result: SettingRecord
  }

  // Catalog: check_types
  check_types_list: {
    args: { args: { include_inactive?: boolean; query?: string } }
    result: CheckTypeRecord[]
  }
  check_types_get: { args: { args: { id: string } }; result: CheckTypeRecord }
  check_types_create: {
    args: { args: CheckTypeCreateArgs }
    result: CheckTypeRecord
  }
  check_types_update: {
    args: { args: CheckTypeUpdateArgs }
    result: CheckTypeRecord
  }
  check_types_toggle_subtypes: {
    args: { args: { id: string; to_value: boolean; base_price_iqd?: number | null } }
    result: CheckTypeRecord
  }
  check_types_soft_delete: { args: { args: { id: string } }; result: null }
  // Catalog: check_subtypes
  check_subtypes_list_by_type: {
    args: { args: { check_type_id: string } }
    result: CheckSubtypeRecord[]
  }
  check_subtypes_create: {
    args: { args: CheckSubtypeCreateArgs }
    result: CheckSubtypeRecord
  }
  check_subtypes_update: {
    args: { args: CheckSubtypeUpdateArgs }
    result: CheckSubtypeRecord
  }
  check_subtypes_soft_delete: { args: { args: { id: string } }; result: null }
  // Catalog: doctors
  doctors_list: {
    args: { args: { include_inactive?: boolean; query?: string } }
    result: DoctorRecord[]
  }
  doctors_get: {
    args: { args: { id: string } }
    result: { doctor: DoctorRecord; pricings: DoctorPricingRecord[] }
  }
  doctors_create: { args: { args: DoctorCreateArgs }; result: DoctorRecord }
  doctors_update: { args: { args: DoctorUpdateArgs }; result: DoctorRecord }
  doctors_set_active: {
    args: { args: { id: string; is_active: boolean } }
    result: DoctorRecord
  }
  doctors_soft_delete: { args: { args: { id: string } }; result: null }
  // Catalog: doctor pricing
  doctor_pricing_upsert: {
    args: { args: DoctorPricingUpsertArgs }
    result: DoctorPricingRecord
  }
  doctor_pricing_soft_delete: { args: { args: { id: string } }; result: null }
  pricing_effective: {
    args: { args: { doctor_id?: string; check_type_id: string; check_subtype_id?: string } }
    result: number
  }
  // Catalog: operators
  operators_list: {
    args: { args: { include_inactive?: boolean; query?: string } }
    result: OperatorRecord[]
  }
  operators_get: {
    args: { args: { id: string } }
    result: { operator: OperatorRecord; specialties: OperatorSpecialtyRecord[] }
  }
  operators_create: { args: { args: OperatorCreateArgs }; result: OperatorRecord }
  operators_update: { args: { args: OperatorUpdateArgs }; result: OperatorRecord }
  operators_set_active: {
    args: { args: { id: string; is_active: boolean } }
    result: OperatorRecord
  }
  operators_soft_delete: { args: { args: { id: string } }; result: null }
  // Catalog: operator specialties
  operator_specialties_upsert: {
    args: { args: { operator_id: string; check_type_id: string } }
    result: OperatorSpecialtyRecord
  }
  operator_specialties_soft_delete: { args: { args: { id: string } }; result: null }
  // Catalog: inventory items
  inventory_catalog_list: {
    args: { args: { include_inactive?: boolean; query?: string } }
    result: InventoryItemRecord[]
  }
  inventory_catalog_get: {
    args: { args: { id: string } }
    result: { item: InventoryItemRecord; consumption: ConsumptionRecord[] }
  }
  inventory_catalog_create: {
    args: { args: InventoryItemCreateArgs }
    result: InventoryItemRecord
  }
  inventory_catalog_update: {
    args: { args: InventoryItemUpdateArgs }
    result: InventoryItemRecord
  }
  inventory_catalog_soft_delete: { args: { args: { id: string } }; result: null }
  // Catalog: consumption map
  inventory_consumption_create: {
    args: { args: ConsumptionCreateArgs }
    result: ConsumptionRecord
  }
  inventory_consumption_update: {
    args: { args: { id: string; quantity_per_check: number; on_dye_only: boolean } }
    result: ConsumptionRecord
  }
  inventory_consumption_soft_delete: { args: { args: { id: string } }; result: null }
  inventory_consumption_list_by_type: {
    args: { args: { check_type_id: string } }
    result: ConsumptionRecord[]
  }

  // Phase 4: shifts
  shifts_clock_in: {
    args: { args: { operator_id: string; note?: string | null } }
    result: ShiftRecord
  }
  shifts_clock_out: {
    args: { args: { shift_id: string } }
    result: ShiftRecord
  }
  shifts_list_open: {
    args: void
    result: ShiftWithMetaRecord[]
  }
  shifts_history_today: {
    args: void
    result: ShiftWithMetaRecord[]
  }
  shifts_edit: {
    args: {
      args: {
        shift_id: string
        check_in_at: string
        check_out_at?: string | null
        note?: { value: string | null } | null
      }
    }
    result: ShiftRecord
  }
  shifts_soft_delete: {
    args: { args: { shift_id: string; reason: string } }
    result: null
  }
  shifts_list_overlaps: {
    args: { args: { operator_id?: string } }
    result: ShiftOverlapPair[]
  }
}

// ---- Catalog wire shapes -------------------------------------------------

export type CutKindLiteral = "pct" | "fixed"

export interface CheckTypeRecord {
  id: string
  name_ar: string
  name_en: string | null
  has_subtypes: boolean
  base_price_iqd: number | null
  dye_supported: boolean
  report_supported: boolean
  sort_order: number
  is_active: boolean
  created_at: string
  updated_at: string
  deleted_at: string | null
  version: number
  entity_id: string
}

export interface CheckTypeCreateArgs {
  name_ar: string
  name_en?: string | null
  has_subtypes: boolean
  base_price_iqd?: number | null
  dye_supported?: boolean
  report_supported?: boolean
  sort_order?: number
}

export interface CheckTypeUpdateArgs {
  id: string
  name_ar?: string
  name_en?: string | null
  base_price_iqd?: number | null
  dye_supported?: boolean
  report_supported?: boolean
  sort_order?: number
  is_active?: boolean
}

export interface CheckSubtypeRecord {
  id: string
  check_type_id: string
  name_ar: string
  name_en: string | null
  price_iqd: number
  sort_order: number
  created_at: string
  updated_at: string
  version: number
  entity_id: string
}

export interface CheckSubtypeCreateArgs {
  check_type_id: string
  name_ar: string
  name_en?: string | null
  price_iqd: number
  sort_order?: number
}

export interface CheckSubtypeUpdateArgs {
  id: string
  name_ar?: string
  name_en?: string | null
  price_iqd?: number
  sort_order?: number
}

export interface DoctorRecord {
  id: string
  name: string
  specialty: string | null
  phone: string | null
  is_active: boolean
  notes: string | null
  created_at: string
  updated_at: string
  version: number
  entity_id: string
}

export interface DoctorCreateArgs {
  name: string
  specialty?: string | null
  phone?: string | null
  notes?: string | null
}

export interface DoctorUpdateArgs {
  id: string
  name?: string
  specialty?: string | null
  phone?: string | null
  notes?: string | null
}

export interface DoctorPricingRecord {
  id: string
  doctor_id: string
  check_type_id: string
  check_subtype_id: string | null
  price_override_iqd: number | null
  cut_kind: CutKindLiteral
  cut_value: number
  created_at: string
  updated_at: string
  version: number
}

export interface DoctorPricingUpsertArgs {
  doctor_id: string
  check_type_id: string
  check_subtype_id?: string | null
  price_override_iqd?: number | null
  cut_kind: CutKindLiteral
  cut_value: number
}

export interface OperatorRecord {
  id: string
  name: string
  phone: string | null
  base_cut_per_check_iqd: number
  is_active: boolean
  notes: string | null
  created_at: string
  updated_at: string
  version: number
}

export interface OperatorCreateArgs {
  name: string
  phone?: string | null
  base_cut_per_check_iqd: number
  notes?: string | null
}

export interface OperatorUpdateArgs {
  id: string
  name?: string
  phone?: string | null
  base_cut_per_check_iqd?: number
  notes?: string | null
}

export interface OperatorSpecialtyRecord {
  id: string
  operator_id: string
  check_type_id: string
  created_at: string
  updated_at: string
  version: number
}

export interface InventoryItemRecord {
  id: string
  name_ar: string
  name_en: string | null
  unit: string
  quantity_on_hand: number
  low_stock_threshold: number
  is_active: boolean
  created_at: string
  updated_at: string
  version: number
}

export interface InventoryItemCreateArgs {
  name_ar: string
  name_en?: string | null
  unit: string
  low_stock_threshold?: number
}

export interface InventoryItemUpdateArgs {
  id: string
  name_ar?: string
  name_en?: string | null
  unit?: string
  low_stock_threshold?: number
  is_active?: boolean
}

export interface ConsumptionRecord {
  id: string
  check_type_id: string
  check_subtype_id: string | null
  item_id: string
  quantity_per_check: number
  on_dye_only: boolean
  version: number
}

export interface ConsumptionCreateArgs {
  check_type_id: string
  check_subtype_id?: string | null
  item_id: string
  quantity_per_check: number
  on_dye_only?: boolean
}

// ---- Phase 4: shifts ----------------------------------------------------

export interface ShiftRecord {
  id: string
  operator_id: string
  check_in_at: string
  check_out_at: string | null
  check_in_by_user_id: string
  check_out_by_user_id: string | null
  note: string | null
  created_at: string
  updated_at: string
  deleted_at: string | null
  version: number
  entity_id: string
}

export interface ShiftWithMetaRecord extends ShiftRecord {
  operator_name: string
  operator_phone: string | null
}

export interface ShiftOverlapPair {
  left: ShiftRecord
  right: ShiftRecord
}

export type UserRoleLiteral = "superadmin" | "receptionist" | "accountant"

export interface AuthUserContext {
  user_id: string
  entity_id: string
  email: string
  name: string | null
  role: string
}

export interface AuthLoginResult {
  mode: "online" | "offline"
  user: UserAdminRecord
}

export interface UserAdminRecord {
  id: string
  email: string
  name: string
  role: UserRoleLiteral
  is_active: boolean
  last_login_at: string | null
  created_at: string
  updated_at: string
  entity_id: string
  version: number
}

export type SettingValueWire =
  | { valueType: "int"; value: number }
  | { valueType: "decimal"; value: string }
  | { valueType: "text"; value: string }
  | { valueType: "bool"; value: boolean }

export interface SettingRecord {
  id: string
  key: string
  value: SettingValueWire
  updated_at: string
  version: number
  entity_id: string
}

export type SyncStatus = "idle" | "pushing" | "pulling" | "offline" | "error"

export interface SyncStatusSnapshot {
  status: SyncStatus
  pendingOps: number
  pending_ops?: number
}

export interface DeviceInfo {
  deviceId: string
  appVersion: string
  device_id?: string
  app_version?: string
}

export interface ConflictRecord {
  opId: string
  entity: string
  entityId: string
  serverPayload: unknown
  localPayload: unknown
  reason: string
}

export async function invoke<K extends keyof CommandMap>(
  command: K,
  ...rest: CommandMap[K]["args"] extends void ? [] : [CommandMap[K]["args"]]
): Promise<CommandMap[K]["result"]> {
  if (!isTauri()) {
    throw new Error(`IPC unavailable: not running inside Tauri (command=${command})`)
  }
  const args = (rest[0] ?? undefined) as Record<string, unknown> | undefined
  return await tauriInvoke<CommandMap[K]["result"]>(command, args)
}

export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined"
  )
}

export async function listenEvent<T>(
  event: string,
  handler: (payload: T) => void
): Promise<UnlistenFn> {
  if (!isTauri()) {
    // No-op subscription when not in Tauri (e.g. Vite dev pageload).
    return async () => undefined
  }
  return await listen<T>(event, (e) => handler(e.payload))
}

export const SYNC_EVENTS = {
  STATUS: "sync:status",
  CONFLICT: "sync:conflict",
  PROGRESS: "sync:progress",
  AUTH_EXPIRED: "auth:session_expired",
} as const
