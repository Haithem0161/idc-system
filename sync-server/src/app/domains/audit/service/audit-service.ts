import { DomainError } from '../../../common/errors/domain'
import type { AuditLogRepository } from '../../../sync/domain/repositories'
import type { AuditPayload } from '../../../sync/domain/types'

/**
 * Audit query service (phase-08 §3 Server, §7.6).
 *
 * Pages by `(at DESC, id DESC)` with a base64url-encoded cursor.
 * Hard limits: page size 100 (default 50), text length [2, 100],
 * entity_id_prefix length [4, 36].
 */
export interface AuditQueryParams {
  from: string
  to: string
  actor?: string
  action?: string
  entity?: string
  entityIdPrefix?: string
  text?: string
  cursor?: string
  limit?: number
}

export interface AuditQueryResult {
  rows: AuditPayload[]
  nextCursor: string | null
}

export class AuditQueryService {
  constructor (private readonly store: AuditLogRepository) {}

  async query (params: AuditQueryParams, tenantId: string): Promise<AuditQueryResult> {
    const from = new Date(params.from)
    const to = new Date(params.to)
    if (Number.isNaN(from.getTime()) || Number.isNaN(to.getTime())) {
      throw new DomainError('VALIDATION_ERROR', 'from/to must be valid RFC3339 timestamps', 422)
    }
    if (to <= from) {
      throw new DomainError('VALIDATION_ERROR', 'to must be after from', 422)
    }
    if (params.text && (params.text.length < 2 || params.text.length > 100)) {
      throw new DomainError('VALIDATION_ERROR', 'text must be 2..100 chars', 422)
    }
    if (params.entityIdPrefix &&
        (params.entityIdPrefix.length < 4 || params.entityIdPrefix.length > 36)) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'entity_id_prefix must be 4..36 chars',
        422
      )
    }
    const limit = clampLimit(params.limit)
    const result = await this.store.queryAudit({
      tenantId,
      from: from.toISOString(),
      to: to.toISOString(),
      actor: params.actor,
      action: params.action,
      entity: params.entity,
      entityIdPrefix: params.entityIdPrefix,
      text: params.text,
      cursor: params.cursor,
      limit,
    })
    return result
  }
}

function clampLimit (raw: number | undefined): number {
  const n = typeof raw === 'number' && Number.isFinite(raw) ? Math.floor(raw) : 50
  if (n < 1) return 1
  if (n > 100) return 100
  return n
}
