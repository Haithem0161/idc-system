import { DomainError } from '../../common/errors/domain'
import type { ConflictParkedRepository } from '../domain/repositories'

export interface ResolveInput {
  choice: 'local' | 'server' | 'merged'
  merged?: Record<string, unknown>
}

export class ConflictResolveService {
  constructor (private readonly conflicts: ConflictParkedRepository) {}

  async resolve (
    opId: string,
    input: ResolveInput,
    userId: string,
    tenantId: string
  ): Promise<void> {
    const parked = await this.conflicts.load(opId, tenantId)
    if (!parked) {
      throw new DomainError('NOT_FOUND', `no parked conflict for op_id=${opId}`, 404)
    }
    if (parked.resolvedAt) {
      throw new DomainError(
        'ALREADY_RESOLVED',
        'this conflict has already been resolved',
        409,
        { resolvedAt: parked.resolvedAt }
      )
    }

    if (input.choice === 'merged' && (!input.merged || typeof input.merged !== 'object')) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'merged payload required when choice = "merged"',
        422
      )
    }

    await this.conflicts.resolve(opId, tenantId, userId)
  }
}
