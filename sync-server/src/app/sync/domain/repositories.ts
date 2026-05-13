import type { AuditPayload, ChangeRow, ParkedConflict } from './types'

/**
 * Ports (interfaces) consumed by the sync services.
 *
 * Two implementations live in this repo:
 *   - `infrastructure/memory/` — test bootstrap, no DB required.
 *   - `infrastructure/prisma/` — production wiring against Postgres.
 *
 * The plugin layer (`plugins/sync-services.ts`) picks one per process by
 * inspecting `fastify.prisma` presence (decorated by the `prisma` plugin
 * when `DATABASE_URL` is set).
 */

export interface ProcessedOpResponse {
  op_id: string
  status: 'applied' | 'duplicate'
  body: unknown
}

export interface ProcessedOpRepository {
  has (opId: string, tenantId: string): Promise<ProcessedOpResponse | null>
  remember (opId: string, tenantId: string, response: ProcessedOpResponse): Promise<void>
  purgeOlderThan (cutoff: Date): Promise<number>
}

export interface AuditLogRepository {
  insertMany (rows: AuditPayload[]): Promise<number>
  /**
   * Fetch changes after the given cursor, ordered by `(at, id)` ascending,
   * limited to `limit` rows. The cursor format is `<rfc3339_at>|<id_uuid>`.
   *
   * Implementations also surface non-audit syncable entities (users,
   * settings, catalog, reception, ...) per the pull contract — the
   * interface name is historic; the method is the canonical pull port.
   */
  changesSince (tenantId: string, cursor: string | null, limit: number): Promise<{
    rows: ChangeRow[]
    nextCursor: string
  }>
  markPulled (tenantId: string, ids: string[]): Promise<void>
  /**
   * Phase-08 §3 Server: `GET /audit/query`. Filters by actor/action/entity/
   * entity_id prefix/free-text/from/to. Sorts `(at DESC, id DESC)`
   * (phase-08 §7.5) with a base64url-encoded `{at, id}` cursor.
   */
  queryAudit (params: {
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
  }): Promise<{ rows: AuditPayload[], nextCursor: string | null }>
}

export interface SyncCursorRepository {
  get (deviceId: string, tenantId: string): Promise<string | null>
  set (deviceId: string, tenantId: string, cursor: string): Promise<void>
}

export interface ConflictParkedRepository {
  park (record: ParkedConflict & { tenantId: string }): Promise<void>
  load (opId: string, tenantId: string): Promise<(ParkedConflict & {
    tenantId: string
    resolvedAt: string | null
  }) | null>
  resolve (opId: string, tenantId: string, userId: string): Promise<void>
  /**
   * Phase-08 §7.11: list unresolved parked conflicts for the tenant,
   * newest-first, capped at 100.
   */
  listOpenConflicts (tenantId: string): Promise<Array<ParkedConflict & {
    tenantId: string
    resolvedAt: string | null
  }>>
}
