import type {
  AuditLogRepository,
  SyncCursorRepository,
} from '../domain/repositories'
import type { ChangeRow } from '../domain/types'

export interface PullOutcome {
  changes: ChangeRow[]
  next_cursor: string
}

export class SyncPullService {
  constructor (
    private readonly audit: AuditLogRepository,
    private readonly cursors: SyncCursorRepository
  ) {}

  async changes (
    tenantId: string,
    deviceId: string,
    since: string | null,
    limit = 500
  ): Promise<PullOutcome> {
    const cursor = since ?? (await this.cursors.get(deviceId, tenantId))
    const { rows, nextCursor } = await this.audit.changesSince(tenantId, cursor, limit)
    if (rows.length > 0) {
      await this.audit.markPulled(tenantId, rows.map((r) => r.entity_id))
      await this.cursors.set(deviceId, tenantId, nextCursor)
    }
    return { changes: rows, next_cursor: nextCursor }
  }
}
