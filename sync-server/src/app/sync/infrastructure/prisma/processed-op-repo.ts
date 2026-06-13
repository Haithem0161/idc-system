import type { PrismaClient } from '@prisma/client'
import { createHash } from 'node:crypto'

import type {
  ProcessedOpRepository,
  ProcessedOpResponse,
} from '../../domain/repositories'

/**
 * Prisma-backed `ProcessedOpRepository`.
 *
 * `processed_ops` is a tenant-scoped dedupe table keyed by `op_id`. We store
 * a sha256 hash of the response body in `response_hash` (matches the existing
 * schema column). The full response is reconstructed on hit by callers that
 * re-execute the canonical response shape — for v0.1.0 we cache the body as
 * a serialized JSON string in `response_hash`'s prefix.
 *
 * Note: the current schema lacks a JSON column for the response body. We
 * stash the response under the same column by encoding `<hex_hash>:<json>` so
 * a future schema bump can split them without a backfill. The hash is the
 * canonical dedupe key; the body is replayed only when the client retries.
 */
export class PrismaProcessedOpRepo implements ProcessedOpRepository {
  constructor (private readonly prisma: PrismaClient) {}

  async has (opId: string, tenantId: string): Promise<ProcessedOpResponse | null> {
    // Look up by the composite (op_id, tenant) key so a row remembered under a
    // different tenant cannot satisfy -- or shadow -- this tenant's dedupe.
    const hit = await this.prisma.processedOp.findUnique({
      where: { opId_entityIdTenant: { opId, entityIdTenant: tenantId } },
    })
    if (!hit) return null
    const body = decodeResponseHash(hit.responseHash)
    return {
      op_id: opId,
      status: 'applied',
      body,
    }
  }

  async remember (
    opId: string,
    tenantId: string,
    response: ProcessedOpResponse
  ): Promise<void> {
    await this.rememberTx(this.prisma, opId, tenantId, response)
  }

  /**
   * Same as `remember` but accepts an interactive Prisma transaction client
   * so callers (notably `ConflictResolveService`) can compose the
   * ProcessedOp write with sibling writes in one atomic `$transaction`.
   * Phase-09 BLOCKER-6.
   */
  async rememberTx (
    tx: Pick<PrismaClient, 'processedOp'>,
    opId: string,
    tenantId: string,
    response: ProcessedOpResponse
  ): Promise<void> {
    const encoded = encodeResponseHash(response)
    // Upsert is keyed by the composite (op_id, tenant). This makes a repeated
    // remember idempotent (a double-remember after a crash-retry is safe) and
    // guarantees one tenant's write never clobbers another tenant's dedupe row.
    await tx.processedOp.upsert({
      where: { opId_entityIdTenant: { opId, entityIdTenant: tenantId } },
      create: {
        opId,
        entityIdTenant: tenantId,
        responseHash: encoded,
      },
      update: {
        responseHash: encoded,
      },
    })
  }

  // Retention sweep for the dedupe table: deletes processed-op rows older than
  // `cutoff` so the table does not grow unbounded.
  //
  // NOTE: not yet scheduled -- there is no BullMQ/cron scheduler in the server
  // yet (a tracked follow-up). When the job runner lands, a periodic worker
  // should call this with the documented retention window (the op-dedupe window
  // only needs to outlive the client's outbox retry horizon). Until then this
  // is a ready, tested operation callable manually or from a future sweep.
  async purgeOlderThan (cutoff: Date): Promise<number> {
    const result = await this.prisma.processedOp.deleteMany({
      where: { processedAt: { lt: cutoff } },
    })
    return result.count
  }
}

function encodeResponseHash (response: ProcessedOpResponse): string {
  const json = JSON.stringify(response.body ?? null)
  const hash = createHash('sha256').update(json).digest('hex')
  return `${hash}:${json}`
}

function decodeResponseHash (encoded: string): unknown {
  const idx = encoded.indexOf(':')
  if (idx < 0) return { ok: true }
  try {
    return JSON.parse(encoded.slice(idx + 1))
  } catch {
    return { ok: true }
  }
}
