import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

const ResolveBodySchema = Type.Object({
  choice: Type.Union([
    Type.Literal('local'),
    Type.Literal('server'),
    Type.Literal('merged'),
  ]),
  merged: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
})

const ResolveParamsSchema = Type.Object({
  opId: Type.String({ minLength: 1 }),
})

const ResolveResponseSchema = Type.Object({
  ok: Type.Literal(true),
})

const ErrorRef = Type.Ref('ErrorResponse')

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.post('/sync/conflicts/:opId/resolve', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['sync'],
      summary: 'Resolve a parked conflict',
      description: `Manual conflict resolution. Picks one of:
- \`local\`: re-apply the client's local payload.
- \`server\`: discard the client op, keep the server row.
- \`merged\`: apply the supplied merged payload (must validate against the entity schema).`,
      security: [{ bearerAuth: [] }],
      params: ResolveParamsSchema,
      body: ResolveBodySchema,
      response: {
        200: ResolveResponseSchema,
        401: ErrorRef,
        404: ErrorRef,
        409: ErrorRef,
        422: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request) => {
      const tenantId = request.tenantId
      const userId = (request.user as { sub?: string } | undefined)?.sub ?? 'unknown'
      await fastify.conflictService.resolve(
        request.params.opId,
        request.body,
        userId,
        tenantId
      )
      return { ok: true as const }
    },
  })
}

export default route
