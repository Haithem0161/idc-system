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
  DailyCloseSyncRecord,
  DoctorPricingSyncRecord,
  DoctorSyncRecord,
  InventoryAdjustmentSyncRecord,
  InventoryItemSyncRecord,
  MandoubSyncRecord,
  OperatorShiftSyncRecord,
  OperatorSpecialtySyncRecord,
  OperatorSyncRecord,
  PatientSyncRecord,
  SettingSyncRecord,
  UserSyncRecord,
  VisitSyncRecord,
} from '../infrastructure/memory/store'
import { decodeAuditPayload, decodeJsonPayload } from './push-decoders'
import { validateSetting, validateVisit } from './validators'

export interface PushAccepted {
  op_id: string
  status: 'applied' | 'duplicate'
}

export interface PushRejected {
  op_id: string
  code: string
  message: string
  status_code: number
}

export interface PushOutcome {
  accepted: PushAccepted[]
  conflicts: ParkedConflict[]
  rejected: PushRejected[]
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
    const accepted: PushAccepted[] = []
    const conflicts: ParkedConflict[] = []
    const rejected: PushRejected[] = []

    for (const op of batch) {
      try {
        await this.applyOne(op, tenantId, deviceId, actor, accepted, conflicts)
      } catch (err) {
        // A single bad op (validation / authorization / unsupported) is
        // isolated as a rejection instead of aborting the whole batch, so the
        // remaining good ops still apply. Non-DomainError failures (DB outage,
        // bugs) still abort -- those are transient/systemic, not op-specific.
        if (err instanceof DomainError) {
          rejected.push({
            op_id: op.op_id,
            code: err.code,
            message: err.message,
            status_code: err.status,
          })
          continue
        }
        throw err
      }
    }

    return { accepted, conflicts, rejected }
  }

  private async applyOne (
    op: PushOp,
    tenantId: string,
    deviceId: string,
    actor: ActorClaims | undefined,
    accepted: PushAccepted[],
    conflicts: ParkedConflict[]
  ): Promise<void> {
    void deviceId
    {
      const cached = await this.processed.has(op.op_id, tenantId)
      if (cached) {
        accepted.push({ op_id: op.op_id, status: 'duplicate' })
        return
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
          const row = decodeJsonPayload<UserSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          await this.store.upsertUser(row)
          break
        }
        case 'settings': {
          this.requireSuperadmin(actor, 'settings push')
          const row = decodeJsonPayload<SettingSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateSetting(row, op.op_id)
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
            return
          }
          await this.store.upsertSetting(row)
          break
        }
        case 'check_types': {
          this.requireSuperadmin(actor, 'check_types push')
          const row = decodeJsonPayload<CheckTypeSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateCheckType(row, op.op_id)
          await this.store.upsertCheckType(row)
          break
        }
        case 'check_subtypes': {
          this.requireSuperadmin(actor, 'check_subtypes push')
          const row = decodeJsonPayload<CheckSubtypeSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          await this.requireSubtypedParent(row.check_type_id, op.op_id)
          await this.store.upsertCheckSubtype(row)
          break
        }
        case 'doctors': {
          this.requireSuperadmin(actor, 'doctors push')
          const row = decodeJsonPayload<DoctorSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'doctor name required',
              422,
              { op_id: op.op_id }
            )
          }
          // Normalize the default cut (client migration 014) and mirror the
          // Rust money-engine contract so a junk cut can never land in Postgres:
          // both halves required together; pct 0..=100; fixed >= 0.
          await this.store.upsertDoctor(normalizeDoctorRow(row, op.op_id))
          break
        }
        case 'doctor_check_pricing': {
          this.requireSuperadmin(actor, 'doctor_check_pricing push')
          const row = decodeJsonPayload<DoctorPricingSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          await this.validateDoctorPricing(row, op.op_id)
          await this.store.upsertDoctorPricing(row)
          break
        }
        case 'operators': {
          this.requireSuperadmin(actor, 'operators push')
          const row = decodeJsonPayload<OperatorSyncRecord>(op.payload_b64, op.op_id)
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
        case 'mandoubs': {
          this.requireSuperadmin(actor, 'mandoubs push')
          const row = decodeJsonPayload<MandoubSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'mandoub name required',
              422,
              { op_id: op.op_id }
            )
          }
          // مندوب is a name + phone + notes record with no stored cut (the
          // per-visit cut is chosen on the visit). LWW.
          await this.store.upsertMandoub(row)
          break
        }
        case 'operator_specialties': {
          this.requireSuperadmin(actor, 'operator_specialties push')
          const row = decodeJsonPayload<OperatorSpecialtySyncRecord>(
            op.payload_b64,
            op.op_id
          )
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          await this.store.upsertOperatorSpecialty(row)
          break
        }
        case 'inventory_items': {
          this.requireSuperadmin(actor, 'inventory_items push')
          const row = decodeJsonPayload<InventoryItemSyncRecord>(op.payload_b64, op.op_id)
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
          const row = decodeJsonPayload<ConsumptionSyncRecord>(op.payload_b64, op.op_id)
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
          const row = decodeJsonPayload<PatientSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          if (row.name.trim().length === 0) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'patient name required',
              422,
              { op_id: op.op_id }
            )
          }
          // Normalize demographics (client migration 012) and mirror the Rust
          // `clean_sex` contract so a junk `sex` can never land in Postgres:
          // empty/whitespace -> null; 'M'/'F' (any case) accepted; anything
          // else is a per-op rejection.
          await this.store.upsertPatient(normalizePatientRow(row, op.op_id))
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
          const row = decodeJsonPayload<VisitSyncRecord>(op.payload_b64, op.op_id)
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
            return
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
          const row = decodeJsonPayload<InventoryAdjustmentSyncRecord>(op.payload_b64, op.op_id)
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
          const row = decodeJsonPayload<OperatorShiftSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateOperatorShift(row, op.op_id)
          await this.store.upsertOperatorShift(row)
          break
        }
        case 'daily_close': {
          // A signed close is authored by an accountant or superadmin (local
          // ReportsService gates sign to [accountant, superadmin]); a reopen is
          // superadmin-only locally and arrives as version 2 of the same id.
          // The server accepts the payload from either of those roles and
          // trusts the local audit row (daily_close_sign / daily_close_reopen)
          // for the finer transition-level role gate. LWW (version-gated).
          if (!actor || (actor.role !== 'accountant' && actor.role !== 'superadmin')) {
            throw new DomainError(
              'VALIDATION_ERROR',
              'daily_close push requires accountant or superadmin role',
              403,
              { op_id: op.op_id }
            )
          }
          const row = decodeJsonPayload<DailyCloseSyncRecord>(op.payload_b64, op.op_id)
          assertTenantMatches(row.entity_id, tenantId, op.op_id)
          validateDailyClose(row, op.op_id)
          await this.store.upsertDailyClose(normalizeDailyCloseRow(row))
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

      // Dedupe is recorded AFTER the entity write, so the two are not a single
      // atomic unit (the SyncEntityStore port is store-agnostic and would need
      // a tx threaded through all 14 upserts plus both store impls to make it
      // so -- out of scope here). The residual non-atomicity is bounded and
      // safe: if a crash lands between the entity write and `remember`, the
      // retry re-applies the SAME op, and every entity write is idempotent
      // (LWW upserts no-op on an equal-or-stale version; additive entities key
      // on a stable id), while `remember` itself is an idempotent upsert keyed
      // on (op_id, tenant). A double-apply therefore converges to the same
      // state rather than corrupting it.
      const response = { op_id: op.op_id, status: 'applied' as const, body: { ok: true } }
      await this.processed.remember(op.op_id, tenantId, response)
      accepted.push({ op_id: op.op_id, status: 'applied' })
    }
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
    if (row.on_dye_only && parent.dye_price_iqd == null) {
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

/** Trim a nullable string field; empty/whitespace collapses to null. Mirrors
 *  the desktop's `clean_opt`. */
function cleanOpt (raw: string | null | undefined): string | null {
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length === 0 ? null : trimmed
}

/**
 * Normalize a pushed patient row's demographics (client migration 012),
 * mirroring the Rust domain contract:
 *  - text fields are trimmed; empty/whitespace -> null (`clean_opt`).
 *  - `sex` accepts 'M'/'F' (any case) -> uppercased; empty -> null; anything
 *    else is a per-op rejection (`clean_sex`).
 *
 * Returning a fresh record (rather than mutating in place) keeps the decoded
 * payload immutable for any downstream conflict bookkeeping.
 */
function normalizePatientRow (row: PatientSyncRecord, opId: string): PatientSyncRecord {
  const sexClean = cleanOpt(row.sex)
  let sex: string | null = null
  if (sexClean !== null) {
    const upper = sexClean.toUpperCase()
    if (upper !== 'M' && upper !== 'F') {
      throw new DomainError('VALIDATION_ERROR', "sex must be 'M' or 'F'", 422, { op_id: opId })
    }
    sex = upper
  }
  return {
    ...row,
    phone: cleanOpt(row.phone),
    sex,
    birth_date: cleanOpt(row.birth_date),
    file_no: cleanOpt(row.file_no),
    notes: cleanOpt(row.notes),
  }
}

/**
 * Normalize a pushed doctor row's default cut (client migration 014), mirroring
 * the Rust `clean_default_cut` contract:
 *  - both halves are required together; one without the other is a rejection.
 *  - `pct` values are 0..=100; `fixed` values are >= 0 (IQD).
 *  - kind is lowercased; an unknown kind is a per-op rejection.
 * Text fields (specialty / phone / notes) are trimmed to null when blank.
 */
function normalizeDoctorRow (row: DoctorSyncRecord, opId: string): DoctorSyncRecord {
  const kindRaw = cleanOpt(row.default_cut_kind)
  const value = row.default_cut_value ?? null
  let kind: string | null = null
  let cutValue: number | null = null
  if (kindRaw === null && value === null) {
    // no default cut -- fine.
  } else if (kindRaw === null || value === null) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'default cut requires both a kind and a value',
      422,
      { op_id: opId }
    )
  } else {
    const lower = kindRaw.toLowerCase()
    if (lower === 'pct') {
      if (value < 0 || value > 100) {
        throw new DomainError('VALIDATION_ERROR', 'default cut percentage must be 0..=100', 422, { op_id: opId })
      }
    } else if (lower === 'fixed') {
      if (value < 0) {
        throw new DomainError('VALIDATION_ERROR', 'default cut amount must be non-negative', 422, { op_id: opId })
      }
    } else {
      throw new DomainError('VALIDATION_ERROR', "default cut kind must be 'pct' or 'fixed'", 422, { op_id: opId })
    }
    kind = lower
    cutValue = value
  }
  return {
    ...row,
    specialty: cleanOpt(row.specialty),
    phone: cleanOpt(row.phone),
    notes: cleanOpt(row.notes),
    default_cut_kind: kind,
    default_cut_value: cutValue,
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

/**
 * Validate a signed daily-close push (client migration 015). The totals are
 * i64 amounts (plain numbers) so the only structural invariants are the
 * required identifiers, a non-empty input_hash, and a >= 1 version (the freeze
 * is version 1; a reopen is version 2). The reopen tombstone columns are
 * checked for coherence: if the day is reopened, who reopened it must be set.
 */
function validateDailyClose (row: DailyCloseSyncRecord, opId: string): void {
  if (row.target_date.trim().length === 0) {
    throw new DomainError('VALIDATION_ERROR', 'daily_close target_date required', 422, { op_id: opId })
  }
  if (row.input_hash.trim().length === 0) {
    throw new DomainError('VALIDATION_ERROR', 'daily_close input_hash required', 422, { op_id: opId })
  }
  if (!row.signed_by_user_id) {
    throw new DomainError('VALIDATION_ERROR', 'daily_close signed_by_user_id required', 422, { op_id: opId })
  }
  if (row.signed_by_name.trim().length === 0) {
    throw new DomainError('VALIDATION_ERROR', 'daily_close signed_by_name required', 422, { op_id: opId })
  }
  if (!row.signed_at) {
    throw new DomainError('VALIDATION_ERROR', 'daily_close signed_at required', 422, { op_id: opId })
  }
  if (row.version < 1) {
    throw new DomainError('VALIDATION_ERROR', 'daily_close version must be >= 1', 422, { op_id: opId })
  }
  // A reopened close (reopened_at set) must record who reopened it. The freeze
  // path leaves all three reopen columns null.
  if (row.reopened_at != null && !row.reopened_by_user_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'reopened daily_close requires reopened_by_user_id',
      422,
      { op_id: opId }
    )
  }
}

/**
 * Normalize a pushed daily-close row, mirroring the desktop's overwrite-on-save
 * semantics: the nullable reopen text columns are trimmed to null when blank so
 * a whitespace-only reopen_reason never lands in Postgres.
 */
function normalizeDailyCloseRow (row: DailyCloseSyncRecord): DailyCloseSyncRecord {
  return {
    ...row,
    reopened_by_user_id: cleanOpt(row.reopened_by_user_id),
    reopen_reason: cleanOpt(row.reopen_reason),
  }
}
