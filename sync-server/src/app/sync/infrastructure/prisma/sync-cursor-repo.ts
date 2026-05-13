import type { PrismaClient } from '@prisma/client'

import type { SyncCursorRepository } from '../../domain/repositories'

/**
 * Prisma-backed `SyncCursorRepository`.
 *
 * Phase-01 §7.19: composite PK `(deviceId, entityIdTenant)`. We store the
 * cursor as a string (`<rfc3339_at>|<entity_row_id>`) to match the
 * `changesSince` cursor encoding produced by the audit repo.
 */
export class PrismaSyncCursorRepo implements SyncCursorRepository {
  constructor (private readonly prisma: PrismaClient) {}

  async get (deviceId: string, tenantId: string): Promise<string | null> {
    const row = await this.prisma.syncCursor.findUnique({
      where: { deviceId_entityIdTenant: { deviceId, entityIdTenant: tenantId } },
    })
    return row?.cursor ?? null
  }

  async set (deviceId: string, tenantId: string, cursor: string): Promise<void> {
    await this.prisma.syncCursor.upsert({
      where: { deviceId_entityIdTenant: { deviceId, entityIdTenant: tenantId } },
      create: { deviceId, entityIdTenant: tenantId, cursor },
      update: { cursor },
    })
  }
}
