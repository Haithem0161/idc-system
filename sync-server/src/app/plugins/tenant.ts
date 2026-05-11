import fp from 'fastify-plugin'
import type { FastifyInstance, FastifyRequest, FastifyReply } from 'fastify'

import { DomainError } from '../common/errors/domain'

/**
 * Tenant plugin.
 *
 * Decorates `request.tenantId` from the JWT `entityId` claim. Any
 * tenant-scoped Prisma query MUST filter by this id; the convention is
 * `where: { entityIdTenant: request.tenantId, ... }`.
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  fastify.decorateRequest('tenantId', '')

  fastify.decorate(
    'requireEntityContext',
    async function (request: FastifyRequest, _reply: FastifyReply) {
      const claims = request.user as { entityId?: string } | undefined
      const id = claims?.entityId
      if (!id || typeof id !== 'string' || id.length === 0) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'entityId claim missing from JWT',
          403
        )
      }
      ;(request as FastifyRequest & { tenantId: string }).tenantId = id
    }
  )
}

export default fp(plugin, { name: 'tenant', dependencies: ['auth-jwt'] })

declare module 'fastify' {
  interface FastifyRequest {
    tenantId: string
  }
  interface FastifyInstance {
    requireEntityContext(request: FastifyRequest, reply: FastifyReply): Promise<void>
  }
}
