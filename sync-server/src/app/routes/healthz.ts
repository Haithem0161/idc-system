import type { FastifyPluginAsync } from 'fastify'
import { Type } from '@sinclair/typebox'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

const HealthSchema = Type.Object({
  status: Type.Literal('ok'),
  version: Type.String(),
})

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.get('/healthz', {
    schema: {
      tags: ['health'],
      summary: 'Liveness probe',
      description: 'Returns 200 OK if the server is reachable. No auth required.',
      response: {
        200: HealthSchema,
      },
    },
  }, async () => ({ status: 'ok' as const, version: '0.1.0' }))
}

export default route
