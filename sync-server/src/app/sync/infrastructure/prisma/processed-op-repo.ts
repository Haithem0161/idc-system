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
    const hit = await this.prisma.processedOp.findUnique({ where: { opId } })
    if (!hit) return null
    if (hit.entityIdTenant !== tenantId) return null
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
    const encoded = encodeResponseHash(response)
    await this.prisma.processedOp.upsert({
      where: { opId },
      create: {
        opId,
        entityIdTenant: tenantId,
        responseHash: encoded,
      },
      update: {
        entityIdTenant: tenantId,
        responseHash: encoded,
      },
    })
  }

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
