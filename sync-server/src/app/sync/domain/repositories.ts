import type { AuditPayload, ChangeRow, ParkedConflict } from './types'

/**
 * Ports (interfaces) consumed by the sync services.
 *
 * Phase-1 ships an in-memory implementation under `infrastructure/memory/`;
 * the Prisma implementation lives next to it and is selected via the
 * `SYNC_STORE` env var.
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
   */
  changesSince (tenantId: string, cursor: string | null, limit: number): Promise<{
    rows: ChangeRow[]
    nextCursor: string
  }>
  markPulled (tenantId: string, ids: string[]): Promise<void>
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
}
