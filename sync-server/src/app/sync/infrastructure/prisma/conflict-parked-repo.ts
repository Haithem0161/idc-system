import type { Prisma, PrismaClient } from '@prisma/client'

import type { ConflictParkedRepository } from '../../domain/repositories'
import type { ParkedConflict } from '../../domain/types'

/**
 * Prisma-backed `ConflictParkedRepository` against the `conflicts_parked`
 * table (schema.prisma: `ConflictParked`).
 *
 * Phase-08 §7.11 + phase-09 §3 (conflict-resolve audit): `resolve` and its
 * sibling `resolveTx` exist so the caller can append the `audit_log` row in
 * the same Postgres transaction as the `resolved_at` write. Public callers
 * (the route handler) use `resolve`; internal callers (conflict-service)
 * use `resolveTx`.
 */
export class PrismaConflictParkedRepo implements ConflictParkedRepository {
  constructor (private readonly prisma: PrismaClient) {}

  async park (record: ParkedConflict & { tenantId: string }): Promise<void> {
    await this.prisma.conflictParked.upsert({
      where: { opId: record.opId },
      create: {
        opId: record.opId,
        entityIdTenant: record.tenantId,
        entity: record.entity,
        entityId: record.entityId,
        localPayload: record.localPayload as Prisma.InputJsonValue,
        serverPayload: record.serverPayload as Prisma.InputJsonValue,
        reason: record.reason,
      },
      update: {
        entity: record.entity,
        entityId: record.entityId,
        localPayload: record.localPayload as Prisma.InputJsonValue,
        serverPayload: record.serverPayload as Prisma.InputJsonValue,
        reason: record.reason,
      },
    })
  }

  async load (opId: string, tenantId: string) {
    const row = await this.prisma.conflictParked.findUnique({ where: { opId } })
    if (!row) return null
    if (row.entityIdTenant !== tenantId) return null
    return toParkedConflict(row, tenantId)
  }

  async resolve (opId: string, tenantId: string, userId: string): Promise<void> {
    await this.resolveTx(this.prisma, opId, tenantId, userId)
  }

  async resolveTx (
    tx: Pick<PrismaClient, 'conflictParked'>,
    opId: string,
    tenantId: string,
    userId: string
  ): Promise<void> {
    await tx.conflictParked.updateMany({
      where: { opId, entityIdTenant: tenantId, resolvedAt: null },
      data: { resolvedAt: new Date(), resolvedByUserId: userId },
    })
  }

  async listOpenConflicts (tenantId: string) {
    const rows = await this.prisma.conflictParked.findMany({
      where: { entityIdTenant: tenantId, resolvedAt: null },
      orderBy: { createdAt: 'desc' },
      take: 100,
    })
    return rows.map((r) => toParkedConflict(r, tenantId))
  }
}

function toParkedConflict (
  row: {
    opId: string
    entity: string
    entityId: string
    localPayload: Prisma.JsonValue
    serverPayload: Prisma.JsonValue
    reason: string
    resolvedAt: Date | null
  },
  tenantId: string
): ParkedConflict & { tenantId: string, resolvedAt: string | null } {
  return {
    opId: row.opId,
    entity: row.entity,
    entityId: row.entityId,
    localPayload: row.localPayload as unknown,
    serverPayload: row.serverPayload as unknown,
    // Persisted park-time reason (e.g. manual_policy_version_divergence);
    // falls back to the column default 'persisted' for legacy rows.
    reason: row.reason,
    tenantId,
    resolvedAt: row.resolvedAt ? row.resolvedAt.toISOString() : null,
  }
}
