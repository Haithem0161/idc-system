import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

import { PullQuerySchema, PullResponseSchema } from '../presentation/schemas/pull.js'

const ErrorRef = Type.Ref('ErrorResponse')

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.get('/sync/pull', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['sync'],
      summary: 'Stream changes for the tenant since a cursor',
      description: `Returns up to \`limit\` changes (default 500) ordered by \`(updated_at, id)\` ascending.

- Cursor format: \`<rfc3339_updated_at>|<id_uuid>\`.
- The cursor is the watermark of the LAST row returned; the client persists it and passes it as \`since\` on the next call.
- Tenant scoping is enforced via the JWT \`entityId\` claim.`,
      security: [{ bearerAuth: [] }],
      querystring: PullQuerySchema,
      response: {
        200: PullResponseSchema,
        401: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request) => {
      const tenantId = request.tenantId
      const deviceId = (request.headers['x-device-id'] as string | undefined) ?? 'unknown'
      const since = request.query.since ?? null
      const limit = request.query.limit ?? 500
      return await fastify.pullService.changes(tenantId, deviceId, since, limit)
    },
  })
}

export default route
