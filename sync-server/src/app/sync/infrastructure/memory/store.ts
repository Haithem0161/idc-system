import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
  ProcessedOpResponse,
  SyncCursorRepository,
} from '../../domain/repositories'
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
  dye_supported: boolean
  report_supported: boolean
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
  ConflictParkedRepository {

  private readonly audit = new Map<string, AuditPayload>()
  readonly users = new Map<string, UserSyncRecord>()
  readonly settings = new Map<string, SettingSyncRecord>()
  readonly checkTypes = new Map<string, CheckTypeSyncRecord>()
  readonly checkSubtypes = new Map<string, CheckSubtypeSyncRecord>()
  readonly doctors = new Map<string, DoctorSyncRecord>()
  readonly doctorPricings = new Map<string, DoctorPricingSyncRecord>()
  readonly operators = new Map<string, OperatorSyncRecord>()
  readonly operatorSpecialties = new Map<string, OperatorSpecialtySyncRecord>()
  readonly inventoryItems = new Map<string, InventoryItemSyncRecord>()
  readonly consumptionMap = new Map<string, ConsumptionSyncRecord>()
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
      .map((row) => ({
        entity: 'users',
        entity_id: row.id,
        payload: row as unknown as Record<string, unknown>,
        updated_at: row.updated_at,
        version: row.version,
      }))

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
      ...mapCatalogChanges('operator_specialties', this.operatorSpecialties, tenantId),
      ...mapCatalogChanges('inventory_items', this.inventoryItems, tenantId),
      ...mapCatalogChanges(
        'inventory_consumption_map',
        this.consumptionMap,
        tenantId
      ),
    ]

    const merged = [...auditChanges, ...userChanges, ...settingChanges, ...catalogChanges]
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
    const hit = this.processed.get(opId)
    if (!hit) return null
    if (hit.tenantId !== tenantId) return null
    return hit.response
  }

  async remember (opId: string, tenantId: string, response: ProcessedOpResponse): Promise<void> {
    this.processed.set(opId, { tenantId, response, processedAt: new Date() })
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

function encodeCursor (at: string, id: string): string {
  return `${at}|${id}`
}

function decodeCursor (cursor: string): { at: string, id: string } | null {
  const idx = cursor.lastIndexOf('|')
  if (idx <= 0) return null
  return { at: cursor.slice(0, idx), id: cursor.slice(idx + 1) }
}
