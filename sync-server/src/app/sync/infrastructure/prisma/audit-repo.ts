import type { Prisma, PrismaClient } from '@prisma/client'

import type { AuditLogRepository } from '../../domain/repositories'
import type { AuditPayload, ChangeRow } from '../../domain/types'
import { PrismaEntityStore } from './entity-store'

/**
 * Prisma-backed `AuditLogRepository` against the `audit_log` table.
 *
 * Three responsibilities:
 *   - `insertMany` — append-only ingestion from `/sync/push` (additive policy
 *     per phase-01 §7.16). Duplicates by `id` are ignored.
 *   - `changesSince` — the canonical pull port. Aggregates audit rows AND
 *     every syncable entity (delegated to `PrismaEntityStore.collectChanges`)
 *     so a single cursor sweeps the entire pullable surface.
 *   - `queryAudit` — phase-08 §3 Server `/audit/query`. Sorts `(at DESC,
 *     id DESC)` with a base64url `{at, id}` cursor.
 */
export class PrismaAuditLogRepo implements AuditLogRepository {
  constructor (
    private readonly prisma: PrismaClient,
    private readonly entityStore: PrismaEntityStore
  ) {}

  async insertMany (rows: AuditPayload[]): Promise<number> {
    return this.insertManyTx(this.prisma, rows)
  }

  /**
   * Same as `insertMany` but accepts an interactive Prisma transaction client
   * so callers (notably `ConflictResolveService`) can compose the audit
   * write with sibling writes in one atomic `$transaction`. Phase-09 BLOCKER-6.
   */
  async insertManyTx (
    tx: Pick<PrismaClient, 'auditLog'>,
    rows: AuditPayload[]
  ): Promise<number> {
    if (rows.length === 0) return 0
    const result = await tx.auditLog.createMany({
      data: rows.map((r) => toCreateRow(r)),
      skipDuplicates: true,
    })
    return result.count
  }

  async changesSince (
    tenantId: string,
    cursor: string | null,
    limit: number
  ): Promise<{ rows: ChangeRow[], nextCursor: string }> {
    const decoded = cursor ? decodeCursor(cursor) : null

    // Apply the keyset cursor IN THE QUERY, before `take`. Filtering only
    // in-memory after taking the oldest N rows meant that once a tenant had
    // more than `limit` audit rows, every row past the limit-th oldest was
    // never loaded -- so audit_log pull returned nothing forever.
    const cursorWhere: Prisma.AuditLogWhereInput | undefined = decoded
      ? {
          OR: [
            { updatedAt: { gt: new Date(decoded.at) } },
            { updatedAt: new Date(decoded.at), id: { gt: decoded.id } },
          ],
        }
      : undefined

    const auditRows = await this.prisma.auditLog.findMany({
      where: {
        entityIdTenant: tenantId,
        deletedAt: null,
        ...(cursorWhere ?? {}),
      },
      orderBy: [{ updatedAt: 'asc' }, { id: 'asc' }],
      take: Math.max(0, Math.min(limit, 500)),
    })

    const auditChanges: ChangeRow[] = auditRows.map((row) => ({
      entity: 'audit_log',
      entity_id: row.id,
      payload: toAuditPayload(row) as unknown as Record<string, unknown>,
      updated_at: row.updatedAt.toISOString(),
      version: row.version,
    }))

    const entityChanges = await this.entityStore.collectChanges(tenantId)

    const merged = [...auditChanges, ...entityChanges]
      .sort((a, b) => {
        const cmp = a.updated_at.localeCompare(b.updated_at)
        return cmp !== 0 ? cmp : a.entity_id.localeCompare(b.entity_id)
      })
      .filter((row) => {
        if (!decoded) return true
        const cmpAt = row.updated_at.localeCompare(decoded.at)
        if (cmpAt !== 0) return cmpAt > 0
        return row.entity_id.localeCompare(decoded.id) > 0
      })
      .slice(0, Math.max(0, Math.min(limit, 500)))

    const last = merged[merged.length - 1]
    const nextCursor = last
      ? encodeCursor(last.updated_at, last.entity_id)
      : cursor ?? ''
    return { rows: merged, nextCursor }
  }

  async markPulled (tenantId: string, ids: string[]): Promise<void> {
    if (ids.length === 0) return
    await this.prisma.auditLog.updateMany({
      where: { entityIdTenant: tenantId, id: { in: ids } },
      data: { pulledAt: new Date() },
    })
  }

  async queryAudit (params: {
    tenantId: string
    from: string
    to: string
    actor?: string
    action?: string
    entity?: string
    entityIdPrefix?: string
    text?: string
    cursor?: string
    limit: number
  }): Promise<{ rows: AuditPayload[], nextCursor: string | null }> {
    const where: Prisma.AuditLogWhereInput = {
      entityIdTenant: params.tenantId,
      at: { gte: new Date(params.from), lte: new Date(params.to) },
    }
    if (params.actor) where.actorUserId = params.actor
    if (params.action) where.action = params.action
    if (params.entity) where.entity = params.entity
    if (params.entityIdPrefix) where.entityId = { startsWith: params.entityIdPrefix }

    // Fetch a window slightly larger than `limit` so we can paginate the
    // text-filtered subset without missing pages. Cap at 500 to bound load.
    const fetchCap = Math.min(500, Math.max(params.limit * 4, params.limit + 1))
    const rows = await this.prisma.auditLog.findMany({
      where,
      orderBy: [{ at: 'desc' }, { id: 'desc' }],
      take: fetchCap,
    })

    const after = params.cursor ? decodeAuditCursor(params.cursor) : null
    const filtered = rows
      .filter((r) => {
        if (!after) return true
        const atIso = r.at.toISOString()
        if (atIso !== after.at) return atIso < after.at
        return r.id < after.id
      })
      .filter((r) => {
        if (!params.text) return true
        const delta = JSON.stringify(r.delta ?? {})
        return delta.includes(params.text) || r.entityId.includes(params.text)
      })

    const slice = filtered.slice(0, params.limit)
    const payloads = slice.map(toAuditPayload)
    const nextCursor = filtered.length > slice.length && slice.length > 0
      ? encodeAuditCursor(slice[slice.length - 1].at.toISOString(), slice[slice.length - 1].id)
      : null
    return { rows: payloads, nextCursor }
  }
}

function toCreateRow (r: AuditPayload): Prisma.AuditLogCreateManyInput {
  return {
    id: r.id,
    actorUserId: r.actor_user_id,
    action: r.action,
    entity: r.entity,
    entityId: r.entity_id,
    delta: r.delta as Prisma.InputJsonValue,
    ip: r.ip ?? null,
    deviceId: r.device_id,
    at: new Date(r.at),
    createdAt: new Date(r.created_at),
    updatedAt: new Date(r.updated_at),
    deletedAt: r.deleted_at ? new Date(r.deleted_at) : null,
    version: r.version,
    lastSyncedAt: r.last_synced_at ? new Date(r.last_synced_at) : null,
    originDeviceId: r.origin_device_id ?? null,
    entityIdTenant: r.entity_id_tenant,
  }
}

function toAuditPayload (r: {
  id: string
  actorUserId: string
  action: string
  entity: string
  entityId: string
  delta: Prisma.JsonValue
  ip: string | null
  deviceId: string
  at: Date
  createdAt: Date
  updatedAt: Date
  deletedAt: Date | null
  version: number
  lastSyncedAt: Date | null
  originDeviceId: string | null
  entityIdTenant: string
}): AuditPayload {
  return {
    id: r.id,
    actor_user_id: r.actorUserId,
    action: r.action,
    entity: r.entity,
    entity_id: r.entityId,
    delta: (r.delta ?? {}) as Record<string, unknown>,
    ip: r.ip,
    device_id: r.deviceId,
    at: r.at.toISOString(),
    created_at: r.createdAt.toISOString(),
    updated_at: r.updatedAt.toISOString(),
    deleted_at: r.deletedAt ? r.deletedAt.toISOString() : null,
    version: r.version,
    last_synced_at: r.lastSyncedAt ? r.lastSyncedAt.toISOString() : null,
    origin_device_id: r.originDeviceId,
    entity_id_tenant: r.entityIdTenant,
  }
}

function encodeCursor (at: string, id: string): string {
  return `${at}|${id}`
}

function decodeCursor (cursor: string): { at: string, id: string } | null {
  const idx = cursor.lastIndexOf('|')
  if (idx <= 0) return null
  return { at: cursor.slice(0, idx), id: cursor.slice(idx + 1) }
}

function encodeAuditCursor (at: string, id: string): string {
  return Buffer.from(JSON.stringify({ at, id }), 'utf-8').toString('base64url')
}

function decodeAuditCursor (cursor: string): { at: string, id: string } | null {
  try {
    const json = Buffer.from(cursor, 'base64url').toString('utf-8')
    return JSON.parse(json) as { at: string, id: string }
  } catch {
    return null
  }
}
