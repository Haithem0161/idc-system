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
  sync_list_stuck: { args: void; result: StuckOpRecord[] }
  // Tauri v2 maps the Rust snake_case param `op_id` to camelCase `opId` on
  // the JS side (top-level command args only -- inner structs do NOT convert).
  sync_requeue_op: { args: { opId: string }; result: null }
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
  auth_has_any_user: { args: void; result: boolean }
  // DEF-007 G01: client-side token rotation. Reads the cached refresh
  // token from AppState, calls `/auth/refresh`, writes the new pair, and
  // fires the `auth:refreshed` Tauri event.
  auth_refresh: { args: void; result: { refreshed_at: string } }
  // DEF-007 G31: online-required password change.
  auth_change_password: {
    args: { args: { current_password: string; new_password: string } }
    result: null
  }
  // DEF-007 G08 / G21: pin the JWT public key in stronghold-equivalent
  // OS-secure storage. Returns the bootstrap outcome + pinned bytes
  // SHA-256 (the PEM never crosses the IPC boundary).
  auth_bootstrap_jwt_key: {
    args: { args: { server_url?: string } }
    result: { outcome: { status: "bootstrapped" | "already_pinned" | "pin_mismatch" }; pinned_sha256: string }
  }
  auth_jwt_pinned_sha256: { args: void; result: string | null }
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
  settings_set_locale: {
    args: { args: { locale: string } }
    result: SettingRecord
  }
  // DEF-007 G23: atomic multi-key save. Validates every (key, value)
  // pair up front; either all writes commit or none do.
  settings_update_batch: {
    args: { args: { entries: Array<{ key: string; value: SettingValueWire }> } }
    result: SettingRecord[]
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
  shifts_lines_run_today: {
    args: { args: { operator_id: string } }
    result: number
  }
  // ---- Phase 5: patients ----
  patients_search: {
    args: { args: { query?: string; limit?: number } }
    result: PatientRecord[]
  }
  patients_create: {
    args: { args: { name: string } }
    result: PatientRecord
  }
  patients_get: {
    args: { args: { id: string } }
    result: PatientRecord
  }
  patients_update: {
    args: { args: { id: string; name: string } }
    result: PatientRecord
  }
  // ---- Phase 5: visits ----
  visits_checks_grid: { args: void; result: ChecksGridCardRecord[] }
  visits_list_today_by_check: {
    args: { args: { check_type_id: string } }
    result: VisitRecord[]
  }
  visits_list_drafts_by_check: {
    args: { args: { check_type_id: string } }
    result: VisitRecord[]
  }
  visits_list_workspace: {
    args: {
      args: {
        check_type_id: string
        statuses?: string[]
        doctor_ids?: string[]
        subtype_ids?: string[]
        limit?: number
      }
    }
    result: VisitRecord[]
  }
  visits_get: {
    args: { args: { visit_id: string } }
    result: VisitRecord
  }
  visits_create_draft: {
    args: {
      args: {
        patient_id: string
        check_type_id: string
        check_subtype_id?: string | null
        doctor_id?: string | null
        dye?: boolean
        report?: boolean
      }
    }
    result: VisitRecord
  }
  visits_update_draft: {
    args: {
      args: {
        visit_id: string
        patient_id?: string
        check_subtype_id?: string | null
        doctor_id?: string | null
        dye?: boolean
        report?: boolean
      }
    }
    result: VisitRecord
  }
  visits_discard: {
    args: { args: { visit_id: string } }
    result: null
  }
  visits_qualified_operators: {
    args: { args: { check_type_id: string } }
    result: QualifiedOperatorRecord[]
  }
  visits_lock: {
    args: { args: { visit_id: string; operator_id: string } }
    result: LockResultRecord
  }
  visits_void: {
    args: { args: { visit_id: string; reason: string } }
    result: VisitRecord
  }
  visits_pricing_resolve: {
    args: { args: { visit_id: string } }
    result: ResolvedSnapshotsRecord
  }
  receipts_reprint: {
    args: { args: { visit_id: string } }
    result: ReceiptArtifactsRecord
  }
  receipts_read: {
    args: { args: { visit_id: string } }
    result: ReceiptContentRecord
  }
  // ---- Phase 6: inventory operations ----
  inventory_list_items: {
    args: {
      args: {
        status?: StockStatusLiteral
        include_inactive?: boolean
        query?: string | null
      }
    }
    result: InventoryItemWithStatusRecord[]
  }
  inventory_get_item: {
    args: { args: { id: string } }
    result: InventoryItemDetailRecord
  }
  inventory_list_adjustments: {
    args: { args: { item_id: string; limit?: number } }
    result: InventoryAdjustmentRecord[]
  }
  inventory_create_adjustment: {
    args: {
      args: {
        item_id: string
        reason: AdjustmentReasonLiteral
        delta: number
        note?: string | null
      }
    }
    result: InventoryAdjustmentRecord
  }
  inventory_recompute_on_hand: {
    args: { args: { item_id: string } }
    result: { new_on_hand: number }
  }
  // ---- Phase 7: reports ----
  reports_dashboard_kpis: {
    args: { args: ReportsRangeArgs }
    result: DashboardKpisRecord
  }
  reports_dashboard_tops: {
    args: { args: ReportsRangeArgs }
    result: DashboardTopsRecord
  }
  reports_visits: {
    args: { args: ReportsVisitsArgs }
    result: VisitsReportRecord
  }
  reports_doctor_earnings: {
    args: { args: ReportsRangeArgs }
    result: DoctorEarningsRecord[]
  }
  reports_doctor_drilldown: {
    args: { args: ReportsDoctorDrilldownArgs }
    result: DoctorDrilldownRecord
  }
  reports_operator_earnings: {
    args: { args: ReportsRangeArgs }
    result: OperatorEarningsRecord[]
  }
  reports_operator_drilldown: {
    args: { args: ReportsOperatorDrilldownArgs }
    result: OperatorDrilldownRecord
  }
  reports_daily_close: {
    args: { args: { date: string } }
    result: DailyCloseRecord
  }
  reports_export_visits_csv: {
    args: { args: { filters: ReportsVisitsArgs; path: string } }
    result: { path: string }
  }
  reports_export_doctors_csv: {
    args: { args: { from_utc: string; to_utc: string; include_voided?: boolean; path: string } }
    result: { path: string }
  }
  reports_export_operators_csv: {
    args: { args: { from_utc: string; to_utc: string; include_voided?: boolean; path: string } }
    result: { path: string }
  }
  reports_export_daily_close_pdf: {
    args: { args: { date: string; path: string } }
    result: { path: string }
  }

  // ---- Phase 8: audit + diagnostics ----
  audit_query: {
    args: { args: AuditQueryArgs }
    result: AuditPageRecord
  }
  audit_vacuum_now: {
    args: void
    result: VacuumResultRecord
  }
  diagnostics_summary: {
    args: void
    result: DiagnosticsSummaryRecord
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

// ---- Phase 6: inventory operations -------------------------------------

export type StockStatusLiteral = "ok" | "low" | "neg"

export type AdjustmentReasonLiteral =
  | "receive"
  | "writeoff"
  | "count_correction"
  | "consume_visit"

/**
 * Item row enriched with the computed stock-status pill and the local
 * `dirty` flag (consumed by the Pending-sync column in
 * `<InventoryItemsTable>` per phase-06 §7.12).
 */
export interface InventoryItemWithStatusRecord {
  id: string
  name_ar: string
  name_en: string | null
  unit: string
  quantity_on_hand: number
  low_stock_threshold: number
  is_active: boolean
  status: StockStatusLiteral
  updated_at: string
  created_at: string
  version: number
  dirty: boolean
  last_synced_at: string | null
  entity_id: string
}

export interface InventoryConsumptionMapRecord {
  id: string
  check_type_id: string
  check_subtype_id: string | null
  item_id: string
  quantity_per_check: number
  on_dye_only: boolean
}

export interface InventoryAdjustmentRecord {
  id: string
  item_id: string
  delta: number
  reason: AdjustmentReasonLiteral
  visit_id: string | null
  note: string | null
  by_user_id: string
  created_at: string
  updated_at: string
  version: number
  entity_id: string
  /**
   * True when the row reverses a voided visit's consume entry (positive
   * delta on a `consume_visit` row). Used by `<ItemAdjustmentsList>` to
   * render the reversal badge per phase-06 §7.15.
   */
  is_reversal: boolean
}

export interface InventoryItemDetailRecord {
  item: InventoryItemWithStatusRecord
  consumption_map: InventoryConsumptionMapRecord[]
  recent_adjustments: InventoryAdjustmentRecord[]
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

// ---- Phase 5 wire shapes ----------------------------------------------

export interface PatientRecord {
  id: string
  name: string
  created_at: string
  updated_at: string
  deleted_at: string | null
  version: number
  dirty: boolean
  entity_id: string
}

export interface ChecksGridCardRecord {
  check_type_id: string
  name_ar: string
  name_en: string | null
  has_subtypes: boolean
  dye_supported: boolean
  report_supported: boolean
  todays_visits: number
}

export interface VisitSnapshotRecord {
  price_iqd: number
  dye_cost_iqd: number
  report_cost_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  internal_pct: number | null
  total_amount_iqd: number
  patient_name: string
  doctor_name: string | null
  operator_name: string
  check_type_name_ar: string
  check_type_name_en: string | null
  check_subtype_name_ar: string | null
  check_subtype_name_en: string | null
}

export interface VisitRecord {
  id: string
  patient_id: string
  status: "draft" | "locked" | "voided"
  receptionist_user_id: string
  check_type_id: string
  check_subtype_id: string | null
  doctor_id: string | null
  operator_id: string | null
  dye: boolean
  report: boolean
  locked_at: string | null
  voided_at: string | null
  voided_by_user_id: string | null
  void_reason: string | null
  snapshots: VisitSnapshotRecord | null
  created_at: string
  updated_at: string
  deleted_at: string | null
  version: number
  dirty: boolean
  entity_id: string
}

export interface QualifiedOperatorRecord {
  id: string
  name: string
  is_active: boolean
}

export interface ReceiptArtifactsRecord {
  a5_path: string
  thermal_path: string
}

export interface ReceiptContentRecord {
  a5: string
  thermal: string
}

export interface LockResultRecord {
  visit: VisitRecord
  artifacts: ReceiptArtifactsRecord
}

export interface ResolvedSnapshotsRecord {
  snapshots: VisitSnapshotRecord
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

// The Rust structs serialize snake_case (default serde). The types reflect
// the actual wire shape; the normalizeSnapshot helper in features/sync maps
// them to the camelCase the UI uses.
export interface SyncStatusSnapshot {
  status: SyncStatus
  pending_ops: number
  /** Ops stranded after a server rejection or attempts cap. */
  stuck_ops: number
}

export interface StuckOpRecord {
  op_id: string
  entity: string
  entity_id: string
  attempts: number
  parked: boolean
  last_error: string | null
  created_at: string
}

export interface DeviceInfo {
  device_id: string
  app_version: string
}

export interface ConflictRecord {
  opId: string
  entity: string
  entityId: string
  serverPayload: unknown
  localPayload: unknown
  reason: string
}

// ---- Phase 7 wire shapes -------------------------------------------------

export interface ReportsRangeArgs {
  from_utc: string
  to_utc: string
  include_voided?: boolean
}

export type VisitsReportGroupByLiteral =
  | "none"
  | "by_date"
  | "by_doctor"
  | "by_operator"
  | "by_check_type"
  | "by_subtype"
  | "by_status"

export interface ReportsVisitsArgs {
  from_utc: string
  to_utc: string
  include_voided?: boolean
  statuses?: string[]
  check_type_ids?: string[]
  subtype_ids?: string[]
  doctor_ids?: string[]
  operator_ids?: string[]
  include_house?: boolean
  dye?: boolean | null
  report?: boolean | null
  group_by?: VisitsReportGroupByLiteral
  limit?: number
}

export interface ReportsDoctorDrilldownArgs {
  doctor_id?: string | null
  from_utc: string
  to_utc: string
  include_voided?: boolean
}

export interface ReportsOperatorDrilldownArgs {
  operator_id: string
  from_utc: string
  to_utc: string
  include_voided?: boolean
}

export interface TrendCellRecord {
  current_iqd: number
  prior_iqd: number
  delta_iqd: number
  delta_permille: number
}

export interface TrendMatrixRecord {
  revenue: TrendCellRecord
  doctor_cuts: TrendCellRecord
  operator_cuts: TrendCellRecord
  inventory_value: TrendCellRecord
  net: TrendCellRecord
}

export interface DashboardKpisRecord {
  range_from: string
  range_to: string
  revenue_iqd: number
  doctor_cuts_iqd: number
  operator_cuts_iqd: number
  inventory_consumption_value_iqd: number
  net_iqd: number
  trend_today_vs_yesterday: TrendMatrixRecord
  trend_week_vs_last_week: TrendMatrixRecord
  trend_month_vs_last_month: TrendMatrixRecord
}

export interface DashboardTopsRecord {
  top_doctors: DoctorEarningsRecord[]
  top_operators: OperatorEarningsRecord[]
  top_check_types: CheckTypeDailyRecord[]
}

export interface VisitReportRowRecord {
  visit_id: string
  locked_at: string | null
  status: string
  patient_name: string
  check_type_name_ar: string
  check_type_name_en: string | null
  check_subtype_name_ar: string | null
  check_subtype_name_en: string | null
  doctor_name: string | null
  operator_name: string
  dye: boolean
  report: boolean
  price_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  net_iqd: number
}

export interface VisitsReportTotalsRecord {
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  net_iqd: number
}

export interface VisitsReportGroupRecord {
  key: string
  label: string
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  net_iqd: number
}

export type VisitsReportRecord =
  | { mode: "rows"; rows: VisitReportRowRecord[]; totals: VisitsReportTotalsRecord }
  | { mode: "groups"; groups: VisitsReportGroupRecord[]; totals: VisitsReportTotalsRecord }

export interface DoctorEarningsRecord {
  doctor_id: string | null
  name: string
  specialty: string | null
  visits: number
  revenue_iqd: number
  doctor_cut_total_iqd: number
  avg_cut_per_visit_iqd: number
}

export interface DoctorPerCheckRowRecord {
  check_type_id: string
  check_type_name_ar: string
  check_type_name_en: string | null
  check_subtype_id: string | null
  check_subtype_name_ar: string | null
  check_subtype_name_en: string | null
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
  avg_cut_iqd: number
}

export interface DoctorDrilldownRecord {
  doctor_id: string | null
  name: string
  specialty: string | null
  per_check: DoctorPerCheckRowRecord[]
  source_visits: VisitReportRowRecord[]
  totals: VisitsReportTotalsRecord
}

export interface OperatorEarningsRecord {
  operator_id: string
  name: string
  visits: number
  visits_with_dye: number
  operator_cut_total_iqd: number
  hours_on_shift_milli: number
  avg_cut_per_hour_iqd: number
}

export interface OperatorShiftRowRecord {
  shift_id: string
  check_in_at: string
  check_out_at: string | null
  duration_milli: number | null
  lines_run: number
  cut_earned_iqd: number
}

export interface OperatorDrilldownRecord {
  operator_id: string
  name: string
  shifts: OperatorShiftRowRecord[]
  attributed_visits: VisitReportRowRecord[]
  totals: VisitsReportTotalsRecord
  total_hours_milli: number
}

export interface DoctorDailyRowRecord {
  doctor_id: string | null
  name: string
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
}

export interface OperatorDailyRowRecord {
  operator_id: string
  name: string
  visits: number
  dye_visits: number
  operator_cut_iqd: number
  hours_on_shift_milli: number
}

export interface CheckTypeDailyRecord {
  check_type_id: string
  name_ar: string
  name_en: string | null
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
}

export interface DailyCloseRecord {
  tenant_id: string
  target_date: string
  tz_offset: string
  total_revenue_iqd: number
  total_doctor_cuts_iqd: number
  total_operator_cuts_iqd: number
  total_inventory_consumption_value_iqd: number
  net_iqd: number
  locked_count: number
  voided_count: number
  voided_value_iqd: number
  per_doctor: DoctorDailyRowRecord[]
  per_operator: OperatorDailyRowRecord[]
  per_check_type: CheckTypeDailyRecord[]
  pending_sync: number
  provisional: boolean
  input_hash: string
  generated_at: string
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

// ---- Phase 8: audit + diagnostics ---------------------------------------

export type AuditActionLiteral =
  | "create"
  | "update"
  | "soft_delete"
  | "lock"
  | "void"
  | "discard"
  | "clock_in"
  | "clock_out"
  | "password_change"
  | "login"
  | "logout"
  | "conflict_resolve"
  | "vacuum"
  | "daily_close_run"

export type AuditEntityLiteral =
  | "users"
  | "settings"
  | "check_types"
  | "check_subtypes"
  | "doctors"
  | "doctor_check_pricing"
  | "operators"
  | "operator_specialties"
  | "operator_shifts"
  | "patients"
  | "visits"
  | "inventory_items"
  | "inventory_consumption_map"
  | "inventory_adjustments"
  | "audit_log"

export interface AuditQueryArgs {
  actor_user_id?: string
  action?: AuditActionLiteral
  entity?: AuditEntityLiteral
  entity_id_prefix?: string
  from_utc?: string
  to_utc?: string
  text?: string
  limit?: number
  offset?: number
}

export interface AuditRowRecord {
  id: string
  at: string
  actor_user_id: string
  action: string
  entity: string
  entity_id: string
  delta: unknown
  device_id: string
  version: number
  dirty: boolean
  source: "local" | "server"
}

export interface AuditPageRecord {
  rows: AuditRowRecord[]
  mode: "local" | "server" | "merged"
  next_offset: number | null
}

export interface VacuumResultRecord {
  audit_purged: number
  metrics_purged: number
}

export interface DiagnosticsSummaryRecord {
  lock_latency_p95_ms: number | null
  outbox_depth: number
  last_sync_at: string | null
  conflict_count_7d: number
  receipt_print_success_rate_30d: number | null
}

export const SYNC_EVENTS = {
  STATUS: "sync:status",
  CONFLICT: "sync:conflict",
  PROGRESS: "sync:progress",
  AUTH_EXPIRED: "auth:session_expired",
  /** Emitted after a pull applies rows; payload `{ entities: string[] }`. */
  APPLIED: "sync:applied",
  /** Emitted when the server rejects this app version with 426. */
  UPGRADE_REQUIRED: "app:upgrade_required",
} as const
