import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
  ProcessedOpResponse,
  SyncCursorRepository,
} from '../../domain/repositories'
import type { AuditPayload, ChangeRow, ParkedConflict } from '../../domain/types'

/**
 * In-memory store for Phase-1 development and tests.
 *
 * Production swap-in: a Prisma-backed implementation that lives alongside
 * this file. The store is intentionally tenant-aware so a single instance
 * can serve multiple test tenants in parallel.
 */
export class MemorySyncStore implements
  AuditLogRepository,
  ProcessedOpRepository,
  SyncCursorRepository,
  ConflictParkedRepository {

  private readonly audit = new Map<string, AuditPayload>()
  private readonly processed = new Map<string, { tenantId: string, response: ProcessedOpResponse, processedAt: Date }>()
  private readonly cursors = new Map<string, string>()
  private readonly conflicts = new Map<string, ParkedConflict & {
    tenantId: string
    resolvedAt: string | null
  }>()

  // ---- AuditLogRepository --------------------------------------------------

  async insertMany (rows: AuditPayload[]): Promise<number> {
    let inserted = 0
    for (const row of rows) {
      if (!this.audit.has(row.id)) {
        this.audit.set(row.id, row)
        inserted += 1
      }
    }
    return inserted
  }

  async changesSince (
    tenantId: string,
    cursor: string | null,
    limit: number
  ): Promise<{ rows: ChangeRow[], nextCursor: string }> {
    const after = cursor ? decodeCursor(cursor) : null
    const candidates = [...this.audit.values()]
      .filter((row) => row.entity_id_tenant === tenantId)
      .filter((row) => row.deleted_at == null)
      .sort((a, b) => {
        const cmp = a.updated_at.localeCompare(b.updated_at)
        return cmp !== 0 ? cmp : a.id.localeCompare(b.id)
      })
      .filter((row) => {
        if (!after) return true
        const cmpAt = row.updated_at.localeCompare(after.at)
        if (cmpAt !== 0) return cmpAt > 0
        return row.id.localeCompare(after.id) > 0
      })
      .slice(0, Math.max(0, Math.min(limit, 500)))

    const rows: ChangeRow[] = candidates.map((row) => ({
      entity: 'audit_log',
      entity_id: row.id,
      payload: row as unknown as Record<string, unknown>,
      updated_at: row.updated_at,
      version: row.version,
    }))

    const last = candidates[candidates.length - 1]
    const nextCursor = last ? encodeCursor(last.updated_at, last.id) : cursor ?? ''

    return { rows, nextCursor }
  }

  async markPulled (tenantId: string, ids: string[]): Promise<void> {
    const at = new Date().toISOString()
    for (const id of ids) {
      const row = this.audit.get(id)
      if (row && row.entity_id_tenant === tenantId) {
        row.last_synced_at = at
      }
    }
  }

  // ---- ProcessedOpRepository ----------------------------------------------

  async has (opId: string, tenantId: string): Promise<ProcessedOpResponse | null> {
    const hit = this.processed.get(opId)
    if (!hit) return null
    if (hit.tenantId !== tenantId) return null
    return hit.response
  }

  async remember (opId: string, tenantId: string, response: ProcessedOpResponse): Promise<void> {
    this.processed.set(opId, { tenantId, response, processedAt: new Date() })
  }

  async purgeOlderThan (cutoff: Date): Promise<number> {
    let removed = 0
    for (const [k, v] of this.processed.entries()) {
      if (v.processedAt < cutoff) {
        this.processed.delete(k)
        removed += 1
      }
    }
    return removed
  }

  // ---- SyncCursorRepository -----------------------------------------------

  async get (deviceId: string, tenantId: string): Promise<string | null> {
    return this.cursors.get(`${tenantId}:${deviceId}`) ?? null
  }

  async set (deviceId: string, tenantId: string, cursor: string): Promise<void> {
    this.cursors.set(`${tenantId}:${deviceId}`, cursor)
  }

  // ---- ConflictParkedRepository -------------------------------------------

  async park (record: ParkedConflict & { tenantId: string }): Promise<void> {
    this.conflicts.set(record.opId, { ...record, resolvedAt: null })
  }

  async load (opId: string, tenantId: string) {
    const hit = this.conflicts.get(opId)
    if (!hit) return null
    if (hit.tenantId !== tenantId) return null
    return hit
  }

  async resolve (opId: string, tenantId: string, userId: string): Promise<void> {
    const hit = this.conflicts.get(opId)
    if (!hit) return
    if (hit.tenantId !== tenantId) return
    hit.resolvedAt = new Date().toISOString()
    void userId
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
