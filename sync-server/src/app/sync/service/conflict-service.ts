import { randomUUID } from 'node:crypto'

import type { PrismaClient } from '@prisma/client'

import { DomainError } from '../../common/errors/domain'
import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
  ProcessedOpResponse,
} from '../domain/repositories'
import type { AuditPayload } from '../domain/types'
import type { SyncEntityStore } from '../domain/sync-store'
import type {
  SettingSyncRecord,
  VisitSyncRecord,
} from '../infrastructure/memory/store'
import { PrismaAuditLogRepo } from '../infrastructure/prisma/audit-repo'
import { PrismaConflictParkedRepo } from '../infrastructure/prisma/conflict-parked-repo'
import { PrismaEntityStore } from '../infrastructure/prisma/entity-store'
import { PrismaProcessedOpRepo } from '../infrastructure/prisma/processed-op-repo'
import { validateSetting, validateVisit } from './validators'

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
    private readonly audit: AuditLogRepository,
    /**
     * The entity store used to APPLY the chosen payload. Without this a
     * 'local'/'merged' resolution would only stamp the audit row and discard
     * the user's choice, leaving the server on its losing version forever.
     */
    private readonly store: SyncEntityStore,
    /**
     * When set, the resolve-time writes (apply chosen payload, conflict
     * resolve, audit row, processed-op cache) run in a single Prisma
     * `$transaction` to satisfy the phase-09 BLOCKER-6 atomicity invariant.
     * When null (test/memory path), the writes run sequentially — memory
     * store is single-threaded so partial-failure rollback is not a hazard.
     */
    private readonly prisma: PrismaClient | null = null
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
    const response: ProcessedOpResponse | null = input.resolveOpId
      ? {
          op_id: input.resolveOpId,
          status: 'applied',
          body: { ok: true, choice: input.choice, opId },
        }
      : null

    if (this.prisma) {
      // Production path: all writes commit atomically.
      // Phase-09 BLOCKER-6 — a network drop between the resolve and the
      // audit-row insert MUST NOT leave the conflict marked resolved
      // without the audit trail row that documents how it was resolved.
      const prisma = this.prisma
      const conflictsTx = new PrismaConflictParkedRepo(prisma)
      const auditTx = new PrismaAuditLogRepo(prisma, null as unknown as never)
      const processedTx = new PrismaProcessedOpRepo(prisma)
      await prisma.$transaction(async (tx) => {
        // Apply the chosen payload to the entity store FIRST so the resolution
        // is meaningless-free: choosing 'local'/'merged' actually overwrites
        // the server's losing version. A txn-scoped store keeps it atomic.
        await this.applyChosen(parked, input, new PrismaEntityStore(tx as unknown as PrismaClient))
        await conflictsTx.resolveTx(tx, opId, tenantId, userId)
        await auditTx.insertManyTx(tx, [auditRow])
        if (input.resolveOpId && response) {
          await processedTx.rememberTx(tx, input.resolveOpId, tenantId, response)
        }
      })
    } else {
      // Memory / test path: sequential is sufficient (single-threaded).
      await this.applyChosen(parked, input, this.store)
      await this.conflicts.resolve(opId, tenantId, userId)
      await this.audit.insertMany([auditRow])
      if (input.resolveOpId && response) {
        await this.processed.remember(input.resolveOpId, tenantId, response)
      }
    }

    return { status: 'applied' }
  }

  /**
   * Apply the user's chosen payload to the entity store.
   * - 'server': the server row already wins; nothing to write.
   * - 'local': re-apply the parked client payload.
   * - 'merged': apply the supplied merged payload.
   * The chosen payload's version is bumped above BOTH the server and local
   * versions so the LWW upsert always accepts it and the resolution
   * propagates to every other device on its next pull.
   * Only `settings` and `visits` are ever parked (the manual-policy
   * entities), so only those are dispatched.
   */
  private async applyChosen (
    parked: { entity: string, serverPayload: unknown, localPayload: unknown },
    input: ResolveInput,
    store: SyncEntityStore
  ): Promise<void> {
    if (input.choice === 'server') return

    const source = input.choice === 'merged' ? input.merged : parked.localPayload
    if (!source || typeof source !== 'object') {
      throw new DomainError(
        'VALIDATION_ERROR',
        `cannot resolve ${parked.entity}: chosen payload is missing or not an object`,
        422
      )
    }

    const winning = { ...(source as Record<string, unknown>) }
    const serverVersion = versionOf(parked.serverPayload)
    const localVersion = versionOf(parked.localPayload)
    winning.version = Math.max(serverVersion, localVersion) + 1

    // Phase-10 T4: re-validate the chosen payload through the SAME domain
    // validators the push path uses, so a `merged` (or stale `local`) payload
    // cannot bypass the invariants on its way into the store. Without this a
    // malformed merge could persist an invalid setting or a locked visit with
    // broken financial snapshots.
    const validationOpId = `resolve:${parked.entity}`
    switch (parked.entity) {
      case 'settings':
        validateSetting(winning as unknown as SettingSyncRecord, validationOpId)
        await store.upsertSetting(winning as unknown as SettingSyncRecord)
        break
      case 'visits':
        validateVisit(winning as unknown as VisitSyncRecord, validationOpId)
        await store.upsertVisit(winning as unknown as VisitSyncRecord)
        break
      default:
        throw new DomainError(
          'VALIDATION_ERROR',
          `conflict resolution not supported for entity ${parked.entity}`,
          422
        )
    }
  }
}

function versionOf (payload: unknown): number {
  if (payload && typeof payload === 'object') {
    const v = (payload as Record<string, unknown>).version
    if (typeof v === 'number' && Number.isFinite(v)) return v
  }
  return 0
}
