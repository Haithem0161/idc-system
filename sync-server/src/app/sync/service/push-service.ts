import { DomainError } from '../../common/errors/domain'
import type { ParkedConflict, PushOp } from '../domain/types'
import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
} from '../domain/repositories'
import type { SyncEntityStore } from '../domain/sync-store'
import type {
  CheckSubtypeSyncRecord,
  CheckTypeSyncRecord,
  ConsumptionSyncRecord,
  DoctorPricingSyncRecord,
  DoctorSyncRecord,
  InventoryAdjustmentSyncRecord,
  InventoryItemSyncRecord,
  OperatorShiftSyncRecord,
  OperatorSpecialtySyncRecord,
  OperatorSyncRecord,
  PatientSyncRecord,
  SettingSyncRecord,
  UserSyncRecord,
  VisitSyncRecord,
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
    private readonly store: SyncEntityStore
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
          const conflict = await this.store.detectSettingConflict(row)
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
          await this.requireSubtypedParent(row.check_type_id, op.op_id)
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
          await this.validateDoctorPricing(row, op.op_id)
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
          await this.validateConsumption(row, op.op_id)
          await this.store.upsertConsumption(row)
          break
        }
        case 'patients': {
          // Receptionist + superadmin can push patient rows. LWW.
          if (!actor || (actor.role !== 'receptionist' && actor.role !== 'superadmin')) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'patients push requires receptionist or superadmin role',
              403,
              { op_id: op.op_id }
            )
          }
          const row = decodeJsonPayload<PatientSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'patient name required',
              422,
              { op_id: op.op_id }
            )
          }
          await this.store.upsertPatient(row)
          break
        }
        case 'visits': {
          if (!actor || (actor.role !== 'receptionist' && actor.role !== 'superadmin')) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'visits push requires receptionist or superadmin role',
              403,
              { op_id: op.op_id }
            )
          }
          const row = decodeJsonPayload<VisitSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateVisit(row, op.op_id)
          const conflict = await this.store.detectVisitConflict(row)
          if (conflict) {
            const envelope: ParkedConflict = {
              opId: op.op_id,
              entity: 'visits',
              entityId: row.id,
              serverPayload: conflict,
              localPayload: row,
              reason: 'manual_policy_visit_divergence',
            }
            await this.conflicts.park({ ...envelope, tenantId })
            conflicts.push(envelope)
            continue
          }
          await this.store.upsertVisit(row)
          break
        }
        case 'inventory_adjustments': {
          // Receptionist + superadmin push receive/writeoff/consume_visit.
          // Accountant CAN forward (for reports drilldown ops) but cannot
          // author count_correction rows. Additive-only: identical id +
          // ProcessedOp hit returns the cached response; identical id
          // without a ProcessedOp hit means a peer tried to mutate an
          // immutable row -- reject (§7.36).
          if (!actor || (actor.role !== 'receptionist' && actor.role !== 'superadmin' && actor.role !== 'accountant')) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'inventory_adjustments push requires authenticated role',
              403,
              { op_id: op.op_id }
            )
          }
          const row = decodeJsonPayload<InventoryAdjustmentSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateAdjustment(row, op.op_id)
          // Phase-06 §7.6: count_correction is superadmin-only.
          if (row.reason === 'count_correction' && actor.role !== 'superadmin') {
            throw new DomainError(
              'VALIDATION_ERROR',
              'count_correction adjustments require superadmin role',
              403,
              { op_id: op.op_id }
            )
          }
          const result = await this.store.upsertInventoryAdjustment(row)
          if (!result.applied && result.duplicate) {
            throw new DomainError(
              'ADDITIVE_VIOLATION',
              'inventory_adjustments are append-only',
              409,
              { op_id: op.op_id }
            )
          }
          break
        }
        case 'operator_shifts': {
          // Receptionists and superadmins both push shift rows: the local
          // ShiftService gates clock_in/clock_out to those roles. Retroactive
          // edits and soft-deletes are superadmin-only locally; the server
          // accepts the payload as additive but trusts the local audit row
          // for the role check.
          if (!actor || (actor.role !== 'receptionist' && actor.role !== 'superadmin')) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'operator_shifts push requires receptionist or superadmin role',
              403,
              { op_id: op.op_id }
            )
          }
          const row = decodeJsonPayload<OperatorShiftSyncRecord>(op.payload_b64)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateOperatorShift(row, op.op_id)
          await this.store.upsertOperatorShift(row)
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

  private async requireSubtypedParent (checkTypeId: string, opId: string): Promise<void> {
    const parent = await this.store.getCheckType(checkTypeId)
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

  private async validateDoctorPricing (row: DoctorPricingSyncRecord, opId: string): Promise<void> {
    const parent = await this.store.getCheckType(row.check_type_id)
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

  private async validateConsumption (row: ConsumptionSyncRecord, opId: string): Promise<void> {
    const parent = await this.store.getCheckType(row.check_type_id)
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

function validateOperatorShift (row: OperatorShiftSyncRecord, opId: string): void {
  if (!row.operator_id) {
    throw new DomainError('VALIDATION_ERROR', 'operator_id required', 422, {
      op_id: opId,
    })
  }
  if (!row.check_in_at) {
    throw new DomainError('VALIDATION_ERROR', 'check_in_at required', 422, {
      op_id: opId,
    })
  }
  if (!row.check_in_by_user_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'check_in_by_user_id required',
      422,
      { op_id: opId }
    )
  }
  if (row.check_out_at != null && row.check_out_at < row.check_in_at) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'check_out_at must be >= check_in_at',
      422,
      { op_id: opId }
    )
  }
  if (row.check_out_at != null && row.check_out_by_user_id == null) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'check_out_by_user_id required when check_out_at is set',
      422,
      { op_id: opId }
    )
  }
  if (row.version < 0) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'version must be non-negative',
      422,
      { op_id: opId }
    )
  }
  if (row.note != null && row.note.length > 1024) {
    throw new DomainError('VALIDATION_ERROR', 'note too long', 422, {
      op_id: opId,
    })
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

function validateVisit (row: VisitSyncRecord, opId: string): void {
  if (!row.patient_id || !row.receptionist_user_id || !row.check_type_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'visit missing required fields',
      422,
      { op_id: opId }
    )
  }
  if (!['draft', 'locked', 'voided'].includes(row.status)) {
    throw new DomainError(
      'VALIDATION_ERROR',
      `visit status invalid: ${row.status}`,
      422,
      { op_id: opId }
    )
  }
  if (row.status === 'locked') {
    if (row.operator_id == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'locked visit must have operator_id',
        422,
        { op_id: opId }
      )
    }
    const snapKeys: Array<keyof VisitSyncRecord> = [
      'price_snapshot_iqd',
      'dye_cost_snapshot_iqd',
      'report_cost_snapshot_iqd',
      'doctor_cut_snapshot_iqd',
      'operator_cut_snapshot_iqd',
      'total_amount_iqd_snapshot',
    ]
    for (const k of snapKeys) {
      if (row[k] == null) {
        throw new DomainError(
          'VALIDATION_ERROR',
          `locked visit missing snapshot field: ${String(k)}`,
          422,
          { op_id: opId }
        )
      }
    }
    const total =
      (row.price_snapshot_iqd ?? 0) +
      (row.dye_cost_snapshot_iqd ?? 0) +
      (row.report_cost_snapshot_iqd ?? 0)
    if (total !== row.total_amount_iqd_snapshot) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'total_amount_iqd_snapshot must equal price + dye + report',
        422,
        { op_id: opId }
      )
    }
    if (row.doctor_id == null && row.internal_pct_snapshot == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'internal_pct_snapshot required when doctor_id is null',
        422,
        { op_id: opId }
      )
    }
    if (row.doctor_id != null && row.internal_pct_snapshot != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'internal_pct_snapshot must be null when doctor_id is set',
        422,
        { op_id: opId }
      )
    }
    if (row.patient_name_snapshot == null || row.operator_name_snapshot == null || row.check_type_name_ar_snapshot == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'locked visit missing name snapshot fields',
        422,
        { op_id: opId }
      )
    }
  }
  if (row.status === 'voided') {
    if (!row.voided_by_user_id || row.void_reason == null || row.void_reason.trim().length < 5) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'voided visit requires voided_by_user_id and a >= 5-char void_reason',
        422,
        { op_id: opId }
      )
    }
  }
  if (row.dye && !row.check_type_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'visit dye requires check_type_id',
      422,
      { op_id: opId }
    )
  }
}

function validateAdjustment (
  row: InventoryAdjustmentSyncRecord,
  opId: string
): void {
  if (!row.item_id || !row.by_user_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'inventory_adjustment missing required fields',
      422,
      { op_id: opId }
    )
  }
  if (!['receive', 'writeoff', 'count_correction', 'consume_visit'].includes(row.reason)) {
    throw new DomainError(
      'VALIDATION_ERROR',
      `inventory_adjustment reason invalid: ${row.reason}`,
      422,
      { op_id: opId }
    )
  }
  if (row.reason === 'consume_visit' && row.visit_id == null) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'consume_visit adjustments require visit_id',
      422,
      { op_id: opId }
    )
  }
  if (row.reason === 'receive' && row.delta <= 0) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'receive adjustments must have positive delta',
      422,
      { op_id: opId }
    )
  }
  if (row.reason === 'writeoff' && row.delta >= 0) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'writeoff adjustments must have negative delta',
      422,
      { op_id: opId }
    )
  }
  // Phase-06 §7.1 / §7.7: count_correction must be non-zero (CHECK
  // backstopped by the local SQLite trigger from migrations/006).
  if (row.reason === 'count_correction' && row.delta === 0) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'count_correction adjustments must have non-zero delta',
      422,
      { op_id: opId }
    )
  }
  if (row.note != null && row.note.length > 500) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'adjustment note must be 500 characters or fewer',
      422,
      { op_id: opId }
    )
  }
}
