import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

// Phase-09 §3.1 contract slice: schemas exported so the Ajv-equivalent
// (`Value.Check`) harness can drift-test the wire shape without
// re-declaring it.
export const LookupBodySchema = Type.Object({
  op_ids: Type.Array(Type.String({ minLength: 1 }), { minItems: 1, maxItems: 200 }),
})

export const LookupResponseSchema = Type.Object({
  found: Type.Array(Type.String()),
})

const ErrorRef = Type.Ref('ErrorResponse')

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.post('/sync/lookup-op', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['sync'],
      summary: 'Existence check for client op_ids',
      description: `Used during SyncEngine boot to reconcile in-flight outbox rows
whose ack was lost. Pure read; no side effects. Returns the subset of \`op_ids\`
that the server has already processed.`,
      security: [{ bearerAuth: [] }],
      body: LookupBodySchema,
      response: {
        200: LookupResponseSchema,
        401: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request) => {
      const tenantId = request.tenantId
      const found: string[] = []
      for (const opId of request.body.op_ids) {
        const hit = await fastify.processedOpRepo.has(opId, tenantId)
        if (hit) found.push(opId)
      }
      return { found }
    },
  })
}

export default route
