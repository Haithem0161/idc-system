import { decode as decodeMsgpack } from '@msgpack/msgpack'

import { DomainError } from '../../common/errors/domain'
import type { AuditPayload, ParkedConflict, PushOp } from '../domain/types'
import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
} from '../domain/repositories'

export interface PushAccepted {
  op_id: string
  status: 'applied' | 'duplicate'
}

export interface PushOutcome {
  accepted: PushAccepted[]
  conflicts: ParkedConflict[]
}

export class SyncPushService {
  constructor (
    private readonly audit: AuditLogRepository,
    // Reserved for entities that use the manual policy; phase-01 has none.
    private readonly _conflicts: ConflictParkedRepository,
    private readonly processed: ProcessedOpRepository
  ) {
    void this._conflicts
  }

  async apply (batch: PushOp[], tenantId: string, deviceId: string): Promise<PushOutcome> {
    void deviceId
    const accepted: PushAccepted[] = []
    const conflicts: ParkedConflict[] = []

    for (const op of batch) {
      // Idempotency: replay the cached response.
      const cached = await this.processed.has(op.op_id, tenantId)
      if (cached) {
        accepted.push({ op_id: op.op_id, status: 'duplicate' })
        continue
      }

      if (op.op !== 'upsert') {
        // §7.15: server rejects all non-upsert ops in v1.
        throw new DomainError(
          'UNSUPPORTED_OP',
          `op kind ${String(op.op)} is not supported in v1`,
          422,
          { op_id: op.op_id }
        )
      }

      if (op.entity !== 'audit_log') {
        // Phase-1 accepts only audit_log; subsequent phases add entities.
        throw new DomainError(
          'VALIDATION_ERROR',
          `entity ${op.entity} not handled in phase-01`,
          422,
          { op_id: op.op_id }
        )
      }

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
        // §7.21: server REJECTS pushes that try to delete an audit row.
        throw new DomainError(
          'AUDIT_IMMUTABLE',
          'audit rows are append-only; deleted_at must be null',
          422,
          { op_id: op.op_id }
        )
      }

      await this.audit.insertMany([payload])
      const response = { op_id: op.op_id, status: 'applied' as const, body: { ok: true } }
      await this.processed.remember(op.op_id, tenantId, response)
      accepted.push({ op_id: op.op_id, status: 'applied' })
    }

    return { accepted, conflicts }
  }
}

function decodeAuditPayload (b64: string): AuditPayload {
  const bytes = Buffer.from(b64, 'base64')
  let raw: unknown
  try {
    raw = decodeMsgpack(bytes)
  } catch (err) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'payload is not valid MessagePack',
      422,
      { reason: (err as Error).message }
    )
  }
  if (!raw || typeof raw !== 'object') {
    throw new DomainError('VALIDATION_ERROR', 'payload root must be an object', 422)
  }
  const obj = raw as Record<string, unknown>
  const required = ['id', 'actor_user_id', 'action', 'entity', 'entity_id', 'device_id', 'entity_id_tenant'] as const
  for (const key of required) {
    if (typeof obj[key] !== 'string') {
      throw new DomainError('VALIDATION_ERROR', `audit payload missing field: ${key}`, 422)
    }
  }
  return {
    id: String(obj.id),
    actor_user_id: String(obj.actor_user_id),
    action: String(obj.action),
    entity: String(obj.entity),
    entity_id: String(obj.entity_id),
    delta: (obj.delta as Record<string, unknown>) ?? {},
    ip: typeof obj.ip === 'string' ? obj.ip : null,
    device_id: String(obj.device_id),
    at: typeof obj.at === 'string' ? obj.at : new Date().toISOString(),
    created_at: typeof obj.created_at === 'string' ? obj.created_at : new Date().toISOString(),
    updated_at: typeof obj.updated_at === 'string' ? obj.updated_at : new Date().toISOString(),
    deleted_at: typeof obj.deleted_at === 'string' ? obj.deleted_at : null,
    version: typeof obj.version === 'number' ? obj.version : 1,
    last_synced_at: null,
    origin_device_id: typeof obj.origin_device_id === 'string' ? obj.origin_device_id : null,
    entity_id_tenant: String(obj.entity_id_tenant),
  }
}
