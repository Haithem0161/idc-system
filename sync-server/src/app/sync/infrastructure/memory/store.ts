import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
  ProcessedOpResponse,
  SyncCursorRepository,
} from '../../domain/repositories'
import type { SyncEntityStore } from '../../domain/sync-store'
import type { AuditPayload, ChangeRow, ParkedConflict } from '../../domain/types'

/**
 * In-memory store for Phase-1+2 development and tests.
 *
 * Holds `audit_log`, `users`, `settings`, `ProcessedOp`, `SyncCursor`, and
 * `ConflictParked`. Production swap-in: Prisma-backed implementation.
 */

export interface UserSyncRecord {
  id: string
  email: string
  name: string
  password_hash?: string
  role: 'superadmin' | 'receptionist' | 'accountant'
  is_active: boolean
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface SettingSyncRecord {
  id: string
  key: string
  value: string
  value_type: 'int' | 'decimal' | 'text' | 'bool'
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

// ---- Catalog records (phase-03) ------------------------------------------

export interface CheckTypeSyncRecord {
  id: string
  name_ar: string
  name_en: string | null
  has_subtypes: boolean
  base_price_iqd: number | null
  dye_price_iqd: number | null
  sort_order: number
  is_active: boolean
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface CheckSubtypeSyncRecord {
  id: string
  check_type_id: string
  name_ar: string
  name_en: string | null
  price_iqd: number
  dye_price_iqd: number | null
  sort_order: number
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface DoctorSyncRecord {
  id: string
  name: string
  specialty: string | null
  phone: string | null
  is_active: boolean
  notes: string | null
  // Doctor-level default cut (client migration 014). Nullable; absent on
  // payloads pushed by an older client, so consumers coerce undefined -> null.
  // Both set/cleared together; default_cut_kind is 'pct' | 'fixed' | null.
  default_cut_kind: string | null
  default_cut_value: number | null
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface DoctorPricingSyncRecord {
  id: string
  doctor_id: string
  check_type_id: string
  check_subtype_id: string | null
  price_override_iqd: number | null
  cut_kind: 'pct' | 'fixed'
  cut_value: number
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface OperatorSyncRecord {
  id: string
  name: string
  phone: string | null
  base_cut_per_check_iqd: number
  is_active: boolean
  notes: string | null
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

// مندوب (representative). Mirrors OperatorSyncRecord MINUS the stored cut: the
// per-visit cut (500 or 1000 IQD) is chosen on the visit, not on this row.
// LWW (client migration 020).
export interface MandoubSyncRecord {
  id: string
  name: string
  phone: string | null
  is_active: boolean
  notes: string | null
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface OperatorSpecialtySyncRecord {
  id: string
  operator_id: string
  check_type_id: string
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface InventoryItemSyncRecord {
  id: string
  name_ar: string
  name_en: string | null
  unit: string
  quantity_on_hand: number
  low_stock_threshold: number
  is_active: boolean
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface ConsumptionSyncRecord {
  id: string
  check_type_id: string
  check_subtype_id: string | null
  item_id: string
  quantity_per_check: number
  on_dye_only: boolean
  entity_id: string
  version: number
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface OperatorShiftSyncRecord {
  id: string
  operator_id: string
  check_in_at: string
  check_out_at: string | null
  check_in_by_user_id: string
  check_out_by_user_id: string | null
  note: string | null
  entity_id: string
  version: number
  created_at: string
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

// ---- Reception records (phase-05) ----------------------------------------

export interface PatientSyncRecord {
  id: string
  name: string
  // Demographics (client migration 012). Nullable; absent on payloads pushed
  // by an older client, so every consumer coerces `undefined` to `null`.
  phone: string | null
  sex: string | null
  birth_date: string | null
  file_no: string | null
  notes: string | null
  entity_id: string
  version: number
  created_at: string
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface VisitSyncRecord {
  id: string
  patient_id: string
  status: 'draft' | 'locked' | 'voided'
  receptionist_user_id: string
  check_type_id: string
  check_subtype_id: string | null
  doctor_id: string | null
  operator_id: string | null
  mandoub_id: string | null
  dye: boolean
  report: boolean
  dalal: boolean
  /** Discount mode: zeroes the referring doctor's cut for this visit. Draft-
   *  editable like `dalal`; valid only with a real referring doctor. */
  discount: boolean
  locked_at: string | null
  voided_at: string | null
  voided_by_user_id: string | null
  void_reason: string | null
  price_snapshot_iqd: number | null
  /** Editable per-visit price the receptionist set on the draft, before it is
   *  frozen into `price_snapshot_iqd` at lock. Decoupled from the check
   *  type's default price; kept for audit/edit-history, not used in cut math
   *  (the snapshot at lock is authoritative for cuts and the total invariant). */
  price_override_iqd: number | null
  dye_cost_snapshot_iqd: number | null
  report_amount_snapshot_iqd: number | null
  report_pct_snapshot: number | null
  reporting_doctor_name_snapshot: string | null
  doctor_cut_snapshot_iqd: number | null
  operator_cut_snapshot_iqd: number | null
  mandoub_cut_snapshot_iqd: number | null
  mandoub_name_snapshot: string | null
  internal_pct_snapshot: number | null
  total_amount_iqd_snapshot: number | null
  amount_paid_override_iqd: number | null
  patient_name_snapshot: string | null
  doctor_name_snapshot: string | null
  operator_name_snapshot: string | null
  check_type_name_ar_snapshot: string | null
  check_type_name_en_snapshot: string | null
  check_subtype_name_ar_snapshot: string | null
  check_subtype_name_en_snapshot: string | null
  entity_id: string
  version: number
  created_at: string
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export interface InventoryAdjustmentSyncRecord {
  id: string
  item_id: string
  delta: number
  reason: 'receive' | 'writeoff' | 'count_correction' | 'consume_visit'
  visit_id: string | null
  note: string | null
  by_user_id: string
  entity_id: string
  version: number
  created_at: string
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

// ---- Daily close (signed & frozen) ---------------------------------------

/**
 * Signed & frozen daily close (client migration 015). Field names mirror the
 * desktop `FrozenClosePushPayload` exactly. Conflict policy is LAST-WRITE-WINS,
 * version-gated: the freeze is version 1; a superadmin reopen is version 2 of
 * the same id (sets `reopened_at` + reopen metadata).
 */
export interface DailyCloseSyncRecord {
  id: string
  target_date: string
  tz_offset: string
  input_hash: string
  total_revenue_iqd: number
  total_collected_iqd: number
  total_discount_iqd: number
  total_doctor_cuts_iqd: number
  total_operator_cuts_iqd: number
  // Optional: absent from pre-migration-019 client payloads (coerced to 0 on
  // apply). Required on the persisted Prisma model (@default(0)).
  total_report_iqd?: number
  // Optional: absent from pre-migration-021 client payloads (coerced to 0 on
  // apply). Required on the persisted Prisma model (@default(0)). Net-side
  // carve-out subtracted from net_iqd AFTER the report.
  total_mandoub_cuts_iqd?: number
  total_inventory_consumption_value_iqd: number
  net_iqd: number
  locked_count: number
  voided_count: number
  voided_value_iqd: number
  signed_by_user_id: string
  signed_by_name: string
  signed_at: string
  reopened_at: string | null
  reopened_by_user_id: string | null
  reopen_reason: string | null
  entity_id: string
  version: number
  created_at: string
  updated_at: string
  deleted_at: string | null
  origin_device_id: string | null
}

export type CatalogSyncRecord =
  | CheckTypeSyncRecord
  | CheckSubtypeSyncRecord
  | DoctorSyncRecord
  | DoctorPricingSyncRecord
  | OperatorSyncRecord
  | OperatorSpecialtySyncRecord
  | InventoryItemSyncRecord
  | ConsumptionSyncRecord

export class MemorySyncStore implements
  AuditLogRepository,
  ProcessedOpRepository,
  SyncCursorRepository,
  ConflictParkedRepository,
  SyncEntityStore {

  private readonly audit = new Map<string, AuditPayload>()
  readonly users = new Map<string, UserSyncRecord>()
  readonly settings = new Map<string, SettingSyncRecord>()
  readonly checkTypes = new Map<string, CheckTypeSyncRecord>()
  readonly checkSubtypes = new Map<string, CheckSubtypeSyncRecord>()
  readonly doctors = new Map<string, DoctorSyncRecord>()
  readonly doctorPricings = new Map<string, DoctorPricingSyncRecord>()
  readonly operators = new Map<string, OperatorSyncRecord>()
  readonly mandoubs = new Map<string, MandoubSyncRecord>()
  readonly operatorSpecialties = new Map<string, OperatorSpecialtySyncRecord>()
  readonly inventoryItems = new Map<string, InventoryItemSyncRecord>()
  readonly consumptionMap = new Map<string, ConsumptionSyncRecord>()
  readonly operatorShifts = new Map<string, OperatorShiftSyncRecord>()
  readonly patients = new Map<string, PatientSyncRecord>()
  readonly visits = new Map<string, VisitSyncRecord>()
  readonly inventoryAdjustments = new Map<string, InventoryAdjustmentSyncRecord>()
  readonly dailyCloses = new Map<string, DailyCloseSyncRecord>()
  private readonly processed = new Map<string, { tenantId: string, response: ProcessedOpResponse, processedAt: Date }>()
  private readonly cursors = new Map<string, string>()
  private readonly conflicts = new Map<string, ParkedConflict & {
    tenantId: string
    resolvedAt: string | null
  }>()

  async insertMany (rows: AuditPayload[]): Promise<number> {
    let inserted = 0
    for (const row of rows) {
      if (!this.audit.has(row.id)) {
        this.audit.set(row.id, row)
        inserted += 1
      }
    }
    return inserted
  }

  async upsertUser (row: UserSyncRecord): Promise<{ applied: boolean }> {
    const existing = this.users.get(row.id)
    if (!existing) {
      this.users.set(row.id, row)
      return { applied: true }
    }
    if (row.version > existing.version ||
        (row.version === existing.version && row.updated_at > existing.updated_at)) {
      this.users.set(row.id, { ...existing, ...row })
      return { applied: true }
    }
    return { applied: false }
  }

  async upsertCheckType (row: CheckTypeSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.checkTypes, row)
  }

  async getCheckType (id: string): Promise<CheckTypeSyncRecord | null> {
    return this.checkTypes.get(id) ?? null
  }

  async listAllVisits (tenantId: string): Promise<VisitSyncRecord[]> {
    return [...this.visits.values()].filter(
      (v) => v.entity_id === tenantId && (v.deleted_at ?? null) == null
    )
  }

  async listAllInventoryAdjustments (
    tenantId: string
  ): Promise<InventoryAdjustmentSyncRecord[]> {
    return [...this.inventoryAdjustments.values()].filter(
      (a) => a.entity_id === tenantId && (a.deleted_at ?? null) == null
    )
  }

  async listAllOperatorShifts (
    tenantId: string
  ): Promise<OperatorShiftSyncRecord[]> {
    return [...this.operatorShifts.values()].filter(
      (s) => s.entity_id === tenantId && (s.deleted_at ?? null) == null
    )
  }

  async upsertCheckSubtype (row: CheckSubtypeSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.checkSubtypes, row)
  }

  async upsertDoctor (row: DoctorSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.doctors, row)
  }

  async upsertDoctorPricing (
    row: DoctorPricingSyncRecord
  ): Promise<{ applied: boolean }> {
    return upsertLWW(this.doctorPricings, row)
  }

  async upsertOperator (row: OperatorSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.operators, row)
  }

  async upsertMandoub (row: MandoubSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.mandoubs, row)
  }

  async upsertOperatorSpecialty (
    row: OperatorSpecialtySyncRecord
  ): Promise<{ applied: boolean }> {
    return upsertLWW(this.operatorSpecialties, row)
  }

  async upsertInventoryItem (
    row: InventoryItemSyncRecord
  ): Promise<{ applied: boolean }> {
    return upsertLWW(this.inventoryItems, row)
  }

  async upsertConsumption (
    row: ConsumptionSyncRecord
  ): Promise<{ applied: boolean }> {
    return upsertLWW(this.consumptionMap, row)
  }

  /**
   * `operator_shifts` follows an additive-only policy:
   * - Pure inserts (new `id`) survive unconditionally.
   * - Updates of an existing `id` (clock_out, retroactive edit, soft_delete)
   *   resolve LWW by `(version, updated_at, origin_device_id)`.
   * See phase-04 §4 Sync Semantics + §7.6 + §7.9.
   */
  async upsertOperatorShift (
    row: OperatorShiftSyncRecord
  ): Promise<{ applied: boolean }> {
    return upsertLWW(this.operatorShifts, row)
  }

  async upsertPatient (row: PatientSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.patients, row)
  }

  /**
   * `daily_close` follows LAST-WRITE-WINS (version-gated). The freeze is
   * version 1; a superadmin reopen is version 2 of the same id, so the reopen
   * (higher version) always wins over the freeze cleanly -- no conflict park.
   * The additive-on-id semantics (a second offline freeze of the same day is
   * ignored) are enforced by the in-force partial-unique index in Postgres;
   * the in-memory store mirrors the version gate only.
   */
  async upsertDailyClose (row: DailyCloseSyncRecord): Promise<{ applied: boolean }> {
    return upsertLWW(this.dailyCloses, row)
  }

  /**
   * `visits` follows a manual conflict policy. The server compares pushed
   * version vs existing.version; older or snapshot-divergent pushes are
   * parked for the resolver UI. See phase-05 §4 Sync Semantics + §7.40.
   */
  async upsertVisit (row: VisitSyncRecord): Promise<{ applied: boolean }> {
    const existing = this.visits.get(row.id)
    if (!existing) {
      this.visits.set(row.id, row)
      return { applied: true }
    }
    if (row.version > existing.version) {
      this.visits.set(row.id, { ...existing, ...row })
      return { applied: true }
    }
    if (row.version === existing.version) {
      const cmp = row.updated_at.localeCompare(existing.updated_at)
      if (cmp > 0) {
        this.visits.set(row.id, { ...existing, ...row })
        return { applied: true }
      }
    }
    return { applied: false }
  }

  /**
   * Detect a manual conflict on a visit push. Returns the existing row when
   * the incoming push would lose data: older version with diverging
   * snapshot/status, OR same version with diverging snapshot.
   */
  detectVisitConflict (incoming: VisitSyncRecord): VisitSyncRecord | null {
    const existing = this.visits.get(incoming.id)
    if (!existing) return null
    const snapshotKeys: (keyof VisitSyncRecord)[] = [
      'status',
      'dalal',
      'discount',
      'price_snapshot_iqd',
      'price_override_iqd',
      'dye_cost_snapshot_iqd',
      'report_amount_snapshot_iqd',
      'report_pct_snapshot',
      'reporting_doctor_name_snapshot',
      'doctor_cut_snapshot_iqd',
      'operator_cut_snapshot_iqd',
      'mandoub_cut_snapshot_iqd',
      'mandoub_name_snapshot',
      'internal_pct_snapshot',
      'total_amount_iqd_snapshot',
      'amount_paid_override_iqd',
    ]
    const snapshotDiffers = snapshotKeys.some((k) => existing[k] !== incoming[k])
    if (incoming.version < existing.version && snapshotDiffers) {
      return existing
    }
    if (incoming.version === existing.version && snapshotDiffers) {
      return existing
    }
    return null
  }

  /**
   * Append-only inventory_adjustments. Returns `{ applied: true }` only on
   * a brand-new id; an existing id is a duplicate (handled via
   * ProcessedOp). See phase-05 §7.36.
   *
   * Phase-06 §7.3: when the adjustment is applied successfully, recompute
   * `inventoryItems.quantity_on_hand` for the affected item by summing all
   * non-deleted adjustments. Mirrors the local SQLite invariant so server
   * reports do not require a separate recompute job.
   */
  async upsertInventoryAdjustment (
    row: InventoryAdjustmentSyncRecord
  ): Promise<{ applied: boolean, duplicate: boolean }> {
    const existing = this.inventoryAdjustments.get(row.id)
    if (!existing) {
      this.inventoryAdjustments.set(row.id, row)
      this.recomputeInventoryItemOnHand(row.item_id, row.entity_id)
      return { applied: true, duplicate: false }
    }
    return { applied: false, duplicate: true }
  }

  /**
   * Sum all non-deleted adjustments for `itemId` WITHIN `tenantId` and
   * overwrite the matching `inventoryItems.quantity_on_hand`. The version is
   * bumped on every recompute so a subsequent pull surfaces the new total to
   * clients that receive `inventory_items` rows in the same batch.
   *
   * Phase-10 T11: the tenant filter mirrors the Prisma store -- without it an
   * adjustment referencing another tenant's item_id would read and overwrite
   * that tenant's inventory item.
   */
  recomputeInventoryItemOnHand (itemId: string, tenantId: string): number {
    let sum = 0
    for (const adj of this.inventoryAdjustments.values()) {
      if (adj.item_id === itemId && adj.entity_id === tenantId && adj.deleted_at == null) {
        sum += adj.delta
      }
    }
    const item = this.inventoryItems.get(itemId)
    // Only recompute the item if it belongs to the same tenant.
    if (item && item.entity_id === tenantId) {
      this.inventoryItems.set(itemId, {
        ...item,
        quantity_on_hand: sum,
        version: item.version + 1,
        updated_at: new Date().toISOString(),
      })
    }
    return sum
  }

  async upsertSetting (row: SettingSyncRecord): Promise<{ applied: boolean }> {
    const existing = this.settings.get(row.id) ?? this.findSettingByKey(row.entity_id, row.key)
    if (!existing) {
      this.settings.set(row.id, row)
      return { applied: true }
    }
    this.settings.set(existing.id, { ...existing, ...row })
    return { applied: true }
  }

  detectSettingConflict (incoming: SettingSyncRecord): SettingSyncRecord | null {
    const existing = this.settings.get(incoming.id) ?? this.findSettingByKey(incoming.entity_id, incoming.key)
    if (!existing) return null
    if (existing.id === incoming.id && existing.version === incoming.version) return null
    if (
      existing.version >= incoming.version &&
      (existing.value !== incoming.value || existing.value_type !== incoming.value_type)
    ) {
      return existing
    }
    return null
  }

  private findSettingByKey (entityId: string, key: string): SettingSyncRecord | undefined {
    for (const s of this.settings.values()) {
      if (s.entity_id === entityId && s.key === key && s.deleted_at == null) {
        return s
      }
    }
    return undefined
  }

  async changesSince (
    tenantId: string,
    cursor: string | null,
    limit: number
  ): Promise<{ rows: ChangeRow[], nextCursor: string }> {
    const after = cursor ? decodeCursor(cursor) : null

    const auditChanges: ChangeRow[] = [...this.audit.values()]
      .filter((row) => row.entity_id_tenant === tenantId && row.deleted_at == null)
      .map((row) => ({
        entity: 'audit_log',
        entity_id: row.id,
        payload: row as unknown as Record<string, unknown>,
        updated_at: row.updated_at,
        version: row.version,
      }))

    const userChanges: ChangeRow[] = [...this.users.values()]
      .filter((row) => row.entity_id === tenantId)
      .map((row) => {
        // SECURITY: strip password_hash from the pull payload (see the Prisma
        // store's toUserSyncRecord). Never ship credential hashes to peers.
        const { password_hash: _omit, ...safe } = row
        return {
          entity: 'users' as const,
          entity_id: row.id,
          payload: safe as unknown as Record<string, unknown>,
          updated_at: row.updated_at,
          version: row.version,
        }
      })

    const settingChanges: ChangeRow[] = [...this.settings.values()]
      .filter((row) => row.entity_id === tenantId && row.deleted_at == null)
      .map((row) => ({
        entity: 'settings',
        entity_id: row.id,
        payload: row as unknown as Record<string, unknown>,
        updated_at: row.updated_at,
        version: row.version,
      }))

    const catalogChanges: ChangeRow[] = [
      ...mapCatalogChanges('check_types', this.checkTypes, tenantId),
      ...mapCatalogChanges('check_subtypes', this.checkSubtypes, tenantId),
      ...mapCatalogChanges('doctors', this.doctors, tenantId),
      ...mapCatalogChanges('doctor_check_pricing', this.doctorPricings, tenantId),
      ...mapCatalogChanges('operators', this.operators, tenantId),
      ...mapCatalogChanges('mandoubs', this.mandoubs, tenantId),
      ...mapCatalogChanges('operator_specialties', this.operatorSpecialties, tenantId),
      ...mapCatalogChanges('inventory_items', this.inventoryItems, tenantId),
      ...mapCatalogChanges(
        'inventory_consumption_map',
        this.consumptionMap,
        tenantId
      ),
    ]

    // Shifts use additive-only semantics: include soft-deleted rows so the
    // tombstone propagates. mapShiftChanges keeps deleted_at on the payload.
    const shiftChanges: ChangeRow[] = mapShiftChanges(this.operatorShifts, tenantId)

    // Reception entities. Patients = LWW (filter deleted). Visits = manual
    // policy (filter deleted). Inventory adjustments = additive (keep
    // tombstones for symmetry though we never actually soft-delete).
    const receptionChanges: ChangeRow[] = [
      ...mapReceptionChanges('patients', this.patients, tenantId, false),
      ...mapReceptionChanges('visits', this.visits, tenantId, false),
      ...mapReceptionChanges(
        'inventory_adjustments',
        this.inventoryAdjustments,
        tenantId,
        true
      ),
    ]

    // Daily close = LWW. A reopened close keeps reopened_at set but is never
    // tombstoned, so it must still propagate (peers learn the day is editable
    // again); only deleted_at hides a row, and that column is unused here.
    const dailyCloseChanges: ChangeRow[] = mapCatalogChanges(
      'daily_close',
      this.dailyCloses,
      tenantId
    )

    const merged = [...auditChanges, ...userChanges, ...settingChanges, ...catalogChanges, ...shiftChanges, ...receptionChanges, ...dailyCloseChanges]
      .sort((a, b) => {
        const cmp = a.updated_at.localeCompare(b.updated_at)
        return cmp !== 0 ? cmp : a.entity_id.localeCompare(b.entity_id)
      })
      .filter((row) => {
        if (!after) return true
        const cmpAt = row.updated_at.localeCompare(after.at)
        if (cmpAt !== 0) return cmpAt > 0
        return row.entity_id.localeCompare(after.id) > 0
      })
      .slice(0, Math.max(0, Math.min(limit, 500)))

    const last = merged[merged.length - 1]
    const nextCursor = last
      ? encodeCursor(last.updated_at, last.entity_id)
      : cursor ?? ''

    return { rows: merged, nextCursor }
  }

  async markPulled (tenantId: string, ids: string[]): Promise<void> {
    void tenantId
    void ids
  }

  async has (opId: string, tenantId: string): Promise<ProcessedOpResponse | null> {
    const hit = this.processed.get(processedKey(opId, tenantId))
    if (!hit) return null
    return hit.response
  }

  async remember (opId: string, tenantId: string, response: ProcessedOpResponse): Promise<void> {
    // Key on the composite (op_id, tenant) so the same op_id under two tenants
    // keeps two independent dedupe rows -- a tenant B remember must NOT clobber
    // tenant A's dedupe (which would let A's retry double-apply). Mirrors the
    // Prisma store's composite primary key.
    this.processed.set(processedKey(opId, tenantId), { tenantId, response, processedAt: new Date() })
  }

  async purgeOlderThan (cutoff: Date): Promise<number> {
    let removed = 0
    for (const [k, v] of this.processed.entries()) {
      if (v.processedAt < cutoff) {
        this.processed.delete(k)
        removed += 1
      }
    }
    return removed
  }

  async get (deviceId: string, tenantId: string): Promise<string | null> {
    return this.cursors.get(`${tenantId}:${deviceId}`) ?? null
  }

  async set (deviceId: string, tenantId: string, cursor: string): Promise<void> {
    this.cursors.set(`${tenantId}:${deviceId}`, cursor)
  }

  async park (record: ParkedConflict & { tenantId: string }): Promise<void> {
    this.conflicts.set(record.opId, { ...record, resolvedAt: null })
  }

  async load (opId: string, tenantId: string) {
    const hit = this.conflicts.get(opId)
    if (!hit) return null
    if (hit.tenantId !== tenantId) return null
    return hit
  }

  async resolve (opId: string, tenantId: string, userId: string): Promise<void> {
    const hit = this.conflicts.get(opId)
    if (!hit) return
    if (hit.tenantId !== tenantId) return
    hit.resolvedAt = new Date().toISOString()
    void userId
  }

  /**
   * Phase-08 §7.11: GET /sync/conflicts. Returns unresolved parked
   * conflicts for the tenant, newest first, capped at 100.
   */
  async listOpenConflicts (tenantId: string): Promise<Array<ParkedConflict & {
    tenantId: string
    resolvedAt: string | null
  }>> {
    return [...this.conflicts.values()]
      .filter((c) => c.tenantId === tenantId && c.resolvedAt == null)
      .slice(0, 100)
  }

  /**
   * Phase-08 §3 Server: GET /audit/query. Filters by actor/action/entity/
   * entity_id prefix/free-text/from/to. Sorts by `(at DESC, id DESC)`
   * (phase-08 §7.5) with a base64-encoded cursor.
   */
  async queryAudit (params: {
    tenantId: string
    from: string
    to: string
    actor?: string
    action?: string
    entity?: string
    entityIdPrefix?: string
    text?: string
    cursor?: string
    limit: number
  }): Promise<{ rows: AuditPayload[], nextCursor: string | null }> {
    const rows = [...this.audit.values()]
      .filter((r) => r.entity_id_tenant === params.tenantId)
      .filter((r) => r.at >= params.from && r.at <= params.to)
      .filter((r) => !params.actor || r.actor_user_id === params.actor)
      .filter((r) => !params.action || r.action === params.action)
      .filter((r) => !params.entity || r.entity === params.entity)
      .filter((r) => !params.entityIdPrefix || r.entity_id.startsWith(params.entityIdPrefix))
      .filter((r) => {
        if (!params.text) return true
        const delta = JSON.stringify(r.delta ?? {})
        return delta.includes(params.text) || r.entity_id.includes(params.text)
      })
      .sort((a, b) => {
        if (b.at !== a.at) return b.at < a.at ? -1 : 1
        return b.id < a.id ? -1 : 1
      })

    let start = 0
    if (params.cursor) {
      const decoded = (() => {
        try {
          const json = Buffer.from(params.cursor, 'base64url').toString('utf-8')
          return JSON.parse(json) as { at: string, id: string }
        } catch {
          return null
        }
      })()
      if (decoded) {
        start = rows.findIndex((r) => {
          if (r.at !== decoded.at) return r.at < decoded.at
          return r.id < decoded.id
        })
        if (start < 0) start = rows.length
      }
    }
    const slice = rows.slice(start, start + params.limit)
    let nextCursor: string | null = null
    if (start + params.limit < rows.length && slice.length > 0) {
      const last = slice[slice.length - 1]
      nextCursor = Buffer.from(
        JSON.stringify({ at: last.at, id: last.id }),
        'utf-8'
      ).toString('base64url')
    }
    return { rows: slice, nextCursor }
  }
}

interface SyncRow {
  id: string
  version: number
  updated_at: string
  entity_id?: string
  deleted_at?: string | null
  origin_device_id?: string | null
}

function mapCatalogChanges<T extends SyncRow> (
  entity: string,
  store: Map<string, T>,
  tenantId: string
): ChangeRow[] {
  return [...store.values()]
    .filter((row) => (row.entity_id ?? '') === tenantId && (row.deleted_at ?? null) == null)
    .map((row) => ({
      entity,
      entity_id: row.id,
      payload: row as unknown as Record<string, unknown>,
      updated_at: row.updated_at,
      version: row.version,
    }))
}

/**
 * Shifts are additive: even soft-deleted rows ship to other devices so the
 * tombstone propagates. Phase-04 §7.9.
 */
function mapShiftChanges (
  store: Map<string, OperatorShiftSyncRecord>,
  tenantId: string
): ChangeRow[] {
  return [...store.values()]
    .filter((row) => row.entity_id === tenantId)
    .map((row) => ({
      entity: 'operator_shifts',
      entity_id: row.id,
      payload: row as unknown as Record<string, unknown>,
      updated_at: row.updated_at,
      version: row.version,
    }))
}

/**
 * Reception entity changes (patients / visits / inventory_adjustments).
 * `includeDeleted` controls whether tombstone rows propagate (additive
 * entities ship them, LWW + manual entities hide them).
 */
function mapReceptionChanges<T extends SyncRow> (
  entity: string,
  store: Map<string, T>,
  tenantId: string,
  includeDeleted: boolean
): ChangeRow[] {
  return [...store.values()]
    .filter((row) => (row.entity_id ?? '') === tenantId)
    .filter((row) => includeDeleted || (row.deleted_at ?? null) == null)
    .map((row) => ({
      entity,
      entity_id: row.id,
      payload: row as unknown as Record<string, unknown>,
      updated_at: row.updated_at,
      version: row.version,
    }))
}

/**
 * Last-write-wins upsert with `(version, updated_at, origin_device_id)`
 * tiebreak (phase-03 §7.17).
 */
function upsertLWW<T extends SyncRow> (store: Map<string, T>, row: T): { applied: boolean } {
  const existing = store.get(row.id)
  if (!existing) {
    store.set(row.id, row)
    return { applied: true }
  }
  if (row.version > existing.version) {
    store.set(row.id, { ...existing, ...row })
    return { applied: true }
  }
  if (row.version < existing.version) {
    return { applied: false }
  }
  const cmp = row.updated_at.localeCompare(existing.updated_at)
  if (cmp > 0) {
    store.set(row.id, { ...existing, ...row })
    return { applied: true }
  }
  if (cmp < 0) {
    return { applied: false }
  }
  // Same version and same updated_at: lex-smaller origin_device_id wins.
  const incoming = row.origin_device_id ?? ''
  const present = existing.origin_device_id ?? ''
  if (incoming !== '' && (present === '' || incoming.localeCompare(present) < 0)) {
    store.set(row.id, { ...existing, ...row })
    return { applied: true }
  }
  return { applied: false }
}

function processedKey (opId: string, tenantId: string): string {
  return `${tenantId}:${opId}`
}

function encodeCursor (at: string, id: string): string {
  return `${at}|${id}`
}

function decodeCursor (cursor: string): { at: string, id: string } | null {
  const idx = cursor.lastIndexOf('|')
  if (idx <= 0) return null
  return { at: cursor.slice(0, idx), id: cursor.slice(idx + 1) }
}
