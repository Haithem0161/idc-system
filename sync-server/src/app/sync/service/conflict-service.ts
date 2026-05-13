import { randomUUID } from 'node:crypto'

import { DomainError } from '../../common/errors/domain'
import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
  ProcessedOpResponse,
} from '../domain/repositories'
import type { AuditPayload } from '../domain/types'

export interface ResolveInput {
  choice: 'local' | 'server' | 'merged'
  merged?: Record<string, unknown>
  /**
   * Stable hash of `(op_id, choice, canonical_merged)` minted by the client
   * so retries of the same resolution collide on `ProcessedOp` (phase-08 §7.22).
   * Required: the route layer accepts the legacy body shape without it
   * (callers from older client builds) but the new client always supplies it.
   */
  resolveOpId?: string
}

export interface ResolveOutcome {
  status: 'applied' | 'duplicate'
}

export class ConflictResolveService {
  constructor (
    private readonly conflicts: ConflictParkedRepository,
    private readonly processed: ProcessedOpRepository,
    private readonly audit: AuditLogRepository
  ) {}

  async resolve (
    opId: string,
    input: ResolveInput,
    userId: string,
    tenantId: string,
    deviceId: string
  ): Promise<ResolveOutcome> {
    // Phase-08 §7.22: idempotent retry. The resolve_op_id collides on
    // identical (op_id, choice, merged) so a mid-flight network drop does
    // not double-apply on the second click.
    if (input.resolveOpId) {
      const cached = await this.processed.has(input.resolveOpId, tenantId)
      if (cached) {
        return { status: 'duplicate' }
      }
    }

    const parked = await this.conflicts.load(opId, tenantId)
    if (!parked) {
      throw new DomainError('NOT_FOUND', `no parked conflict for op_id=${opId}`, 404)
    }
    if (parked.resolvedAt) {
      throw new DomainError(
        'ALREADY_RESOLVED',
        'this conflict has already been resolved on another device',
        409,
        { resolvedAt: parked.resolvedAt }
      )
    }

    if (input.choice === 'merged' && (!input.merged || typeof input.merged !== 'object')) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'merged payload required when choice = "merged"',
        422
      )
    }

    await this.conflicts.resolve(opId, tenantId, userId)

    // Phase-09 §3 (conflict-resolve audit): emit the audit row that the
    // phase-08 §1 enum advertised but no writer was emitting. Server-
    // canonical: the row lives only on the server until the next
    // /sync/pull brings it down to the resolver's device.
    const now = new Date().toISOString()
    const auditRow: AuditPayload = {
      id: randomUUID(),
      actor_user_id: userId,
      action: 'conflict_resolve',
      entity: parked.entity,
      entity_id: parked.entityId,
      delta: {
        choice: input.choice,
        opId,
        resolveOpId: input.resolveOpId ?? null,
      },
      ip: null,
      device_id: deviceId,
      at: now,
      created_at: now,
      updated_at: now,
      deleted_at: null,
      version: 1,
      last_synced_at: null,
      origin_device_id: deviceId,
      entity_id_tenant: tenantId,
    }
    await this.audit.insertMany([auditRow])

    if (input.resolveOpId) {
      const response: ProcessedOpResponse = {
        op_id: input.resolveOpId,
        status: 'applied',
        body: { ok: true, choice: input.choice, opId },
      }
      await this.processed.remember(input.resolveOpId, tenantId, response)
    }

    return { status: 'applied' }
  }
}
