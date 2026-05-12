import { DomainError } from '../../common/errors/domain'
import type { ParkedConflict, PushOp } from '../domain/types'
import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
} from '../domain/repositories'
import type {
  CheckSubtypeSyncRecord,
  CheckTypeSyncRecord,
  ConsumptionSyncRecord,
  DoctorPricingSyncRecord,
  DoctorSyncRecord,
  InventoryItemSyncRecord,
  MemorySyncStore,
  OperatorSpecialtySyncRecord,
  OperatorSyncRecord,
  SettingSyncRecord,
  UserSyncRecord,
} from '../infrastructure/memory/store'
import { decodeAuditPayload, decodeJsonPayload } from './push-decoders'

const PROTECTED_SETTING_KEYS = new Set([
  'dye_cost_iqd',
  'report_cost_iqd',
  'internal_doctor_pct',
  'idle_lock_minutes',
  'arabic_numerals',
  'clinic_display_name_ar',
  'clinic_display_name_en',
  'currency_symbol',
  'thermal_width',
  'thermal_printer_name',
])

export interface PushAccepted {
  op_id: string
  status: 'applied' | 'duplicate'
}

export interface PushOutcome {
  accepted: PushAccepted[]
  conflicts: ParkedConflict[]
}

export interface ActorClaims {
  sub: string
  role: 'superadmin' | 'receptionist' | 'accountant'
  entityId: string
}

export class SyncPushService {
  constructor (
    private readonly audit: AuditLogRepository,
    private readonly conflicts: ConflictParkedRepository,
    private readonly processed: ProcessedOpRepository,
    private readonly store: MemorySyncStore
  ) {}

  async apply (
    batch: PushOp[],
    tenantId: string,
    deviceId: string,
    actor?: ActorClaims
  ): Promise<PushOutcome> {
    void deviceId
    const accepted: PushAccepted[] = []
    const conflicts: ParkedConflict[] = []

    for (const op of batch) {
      const cached = await this.processed.has(op.op_id, tenantId)
      if (cached) {
        accepted.push({ op_id: op.op_id, status: 'duplicate' })
        continue
      }
      if (op.op !== 'upsert') {
        throw new DomainError(
          'UNSUPPORTED_OP',
          `op kind ${String(op.op)} is not supported in v1`,
          422,
          { op_id: op.op_id }
        )
      }

      switch (op.entity) {
        case 'audit_log': {
          const payload = decodeAuditPayload(op.payload_b64)
          if (payload.entity_id_tenant !== tenantId) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'payload entity_id_tenant must match JWT entityId',
              403,
              { op_id: op.op_id }
            )
          }
          if (payload.deleted_at != null) {
            throw new DomainError(
              'AUDIT_IMMUTABLE',
              'audit rows are append-only; deleted_at must be null',
              422,
              { op_id: op.op_id }
            )
          }
          await this.audit.insertMany([payload])
          break
        }
        case 'users': {
          this.requireSuperadmin(actor, 'users push')
          const row = decodeJsonPayload<UserSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          await this.store.upsertUser(row)
          break
        }
        case 'settings': {
          this.requireSuperadmin(actor, 'settings push')
          const row = decodeJsonPayload<SettingSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.deleted_at && PROTECTED_SETTING_KEYS.has(row.key)) {
            throw new DomainError(
              'VALIDATION_ERROR',
              `${row.key} is a required setting and cannot be deleted`,
              422,
              { op_id: op.op_id }
            )
          }
          const conflict = this.store.detectSettingConflict(row)
          if (conflict) {
            const envelope: ParkedConflict = {
              opId: op.op_id,
              entity: 'settings',
              entityId: row.id,
              serverPayload: conflict,
              localPayload: row,
              reason: 'manual_policy_version_divergence',
            }
            await this.conflicts.park({ ...envelope, tenantId })
            conflicts.push(envelope)
            continue
          }
          await this.store.upsertSetting(row)
          break
        }
        case 'check_types': {
          this.requireSuperadmin(actor, 'check_types push')
          const row = decodeJsonPayload<CheckTypeSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateCheckType(row, op.op_id)
          await this.store.upsertCheckType(row)
          break
        }
        case 'check_subtypes': {
          this.requireSuperadmin(actor, 'check_subtypes push')
          const row = decodeJsonPayload<CheckSubtypeSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          this.requireSubtypedParent(row.check_type_id, op.op_id)
          await this.store.upsertCheckSubtype(row)
          break
        }
        case 'doctors': {
          this.requireSuperadmin(actor, 'doctors push')
          const row = decodeJsonPayload<DoctorSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'doctor name required',
              422,
              { op_id: op.op_id }
            )
          }
          await this.store.upsertDoctor(row)
          break
        }
        case 'doctor_check_pricing': {
          this.requireSuperadmin(actor, 'doctor_check_pricing push')
          const row = decodeJsonPayload<DoctorPricingSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          this.validateDoctorPricing(row, op.op_id)
          await this.store.upsertDoctorPricing(row)
          break
        }
        case 'operators': {
          this.requireSuperadmin(actor, 'operators push')
          const row = decodeJsonPayload<OperatorSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'operator name required',
              422,
              { op_id: op.op_id }
            )
          }
          if (row.base_cut_per_check_iqd < 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'base_cut_per_check_iqd must be non-negative',
              422,
              { op_id: op.op_id }
            )
          }
          await this.store.upsertOperator(row)
          break
        }
        case 'operator_specialties': {
          this.requireSuperadmin(actor, 'operator_specialties push')
          const row = decodeJsonPayload<OperatorSpecialtySyncRecord>(
            op.payload_b64
          )
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          await this.store.upsertOperatorSpecialty(row)
          break
        }
        case 'inventory_items': {
          this.requireSuperadmin(actor, 'inventory_items push')
          const row = decodeJsonPayload<InventoryItemSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name_ar.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'name_ar required',
              422,
              { op_id: op.op_id }
            )
          }
          if (row.unit.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'unit required',
              422,
              { op_id: op.op_id }
            )
          }
          await this.store.upsertInventoryItem(row)
          break
        }
        case 'inventory_consumption_map': {
          this.requireSuperadmin(actor, 'inventory_consumption_map push')
          const row = decodeJsonPayload<ConsumptionSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          this.validateConsumption(row, op.op_id)
          await this.store.upsertConsumption(row)
          break
        }
        default:
          throw new DomainError(
            'VALIDATION_ERROR',
            `entity ${op.entity} not handled in this phase`,
            422,
            { op_id: op.op_id }
          )
      }

      const response = { op_id: op.op_id, status: 'applied' as const, body: { ok: true } }
      await this.processed.remember(op.op_id, tenantId, response)
      accepted.push({ op_id: op.op_id, status: 'applied' })
    }

    return { accepted, conflicts }
  }

  private requireSuperadmin (actor: ActorClaims | undefined, what: string): void {
    if (!actor || actor.role !== 'superadmin') {
      throw new DomainError(
        'VALIDATION_ERROR',
        `${what} requires superadmin role`,
        403
      )
    }
  }

  private requireSubtypedParent (checkTypeId: string, opId: string): void {
    const parent = this.store.checkTypes.get(checkTypeId)
    if (!parent) {
      throw new DomainError(
        'VALIDATION_ERROR',
        `parent check_type ${checkTypeId} not found`,
        422,
        { op_id: opId }
      )
    }
    if (parent.deleted_at) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'parent check_type is deleted',
        422,
        { op_id: opId }
      )
    }
    if (!parent.has_subtypes) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'parent check_type does not allow subtypes (errors:catalog.parent_not_subtyped)',
        422,
        { op_id: opId }
      )
    }
  }

  private validateDoctorPricing (row: DoctorPricingSyncRecord, opId: string): void {
    const parent = this.store.checkTypes.get(row.check_type_id)
    if (!parent) {
      throw new DomainError(
        'VALIDATION_ERROR',
        `parent check_type ${row.check_type_id} not found`,
        422,
        { op_id: opId }
      )
    }
    if (parent.has_subtypes && row.check_subtype_id == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'check_subtype_id required when parent has subtypes',
        422,
        { op_id: opId }
      )
    }
    if (!parent.has_subtypes && row.check_subtype_id != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'check_subtype_id forbidden when parent has no subtypes',
        422,
        { op_id: opId }
      )
    }
    if (row.cut_value < 0) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'cut_value must be non-negative',
        422,
        { op_id: opId }
      )
    }
    if (row.cut_kind === 'pct' && row.cut_value > 100) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'cut_value must be <= 100 for pct',
        422,
        { op_id: opId }
      )
    }
    if (row.price_override_iqd != null && row.price_override_iqd < 0) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'price_override_iqd must be non-negative',
        422,
        { op_id: opId }
      )
    }
  }

  private validateConsumption (row: ConsumptionSyncRecord, opId: string): void {
    const parent = this.store.checkTypes.get(row.check_type_id)
    if (!parent) {
      throw new DomainError(
        'VALIDATION_ERROR',
        `parent check_type ${row.check_type_id} not found`,
        422,
        { op_id: opId }
      )
    }
    if (parent.has_subtypes && row.check_subtype_id == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'check_subtype_id required when parent has subtypes',
        422,
        { op_id: opId }
      )
    }
    if (!parent.has_subtypes && row.check_subtype_id != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'check_subtype_id forbidden when parent has no subtypes',
        422,
        { op_id: opId }
      )
    }
    if (row.quantity_per_check <= 0) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'quantity_per_check must be > 0',
        422,
        { op_id: opId }
      )
    }
    if (row.on_dye_only && !parent.dye_supported) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'parent check_type does not support dye',
        422,
        { op_id: opId }
      )
    }
  }
}

function validateCheckType (row: CheckTypeSyncRecord, opId: string): void {
  if (row.name_ar.trim().length === 0) {
    throw new DomainError('VALIDATION_ERROR', 'name_ar required', 422, { op_id: opId })
  }
  if (row.has_subtypes && row.base_price_iqd != null) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'base_price_iqd must be null when has_subtypes=true',
      422,
      { op_id: opId }
    )
  }
  if (!row.has_subtypes && (row.base_price_iqd == null || row.base_price_iqd < 0)) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'base_price_iqd must be non-negative when has_subtypes=false',
      422,
      { op_id: opId }
    )
  }
}

function assertTenantMatches (rowTenant: string, tenantId: string, opId: string): void {
  if (rowTenant !== tenantId) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'payload entity_id must match JWT entityId',
      403,
      { op_id: opId }
    )
  }
}
