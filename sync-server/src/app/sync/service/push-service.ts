import { DomainError } from '../../common/errors/domain'
import type { ParkedConflict, PushOp } from '../domain/types'
import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
} from '../domain/repositories'
import type {
  MemorySyncStore,
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
