import fp from 'fastify-plugin'
import swagger from '@fastify/swagger'
import swaggerUi from '@fastify/swagger-ui'

import { ErrorResponseSchema } from '../common/schemas/error'

/**
 * OpenAPI + Swagger UI plugin.
 *
 * Auto-generates `/documentation` from per-route schemas. The canonical
 * `ErrorResponse` shape is registered here so every route can `$ref` it.
 */
export default fp(async (fastify) => {
  fastify.addSchema(ErrorResponseSchema)

  await fastify.register(swagger, {
    openapi: {
      info: {
        title: 'IDC Sync Server',
        description:
          'Offline-first sync, conflict resolution, and backup endpoints for the IDC desktop app.',
        version: '0.1.0',
      },
      tags: [
        { name: 'health', description: 'Liveness and version' },
        { name: 'sync', description: 'Push, pull, and conflict resolution' },
      ],
      components: {
        securitySchemes: {
          bearerAuth: {
            type: 'http',
            scheme: 'bearer',
            bearerFormat: 'JWT',
          },
        },
      },
    },
  })

  await fastify.register(swaggerUi, {
    routePrefix: '/documentation',
    uiConfig: { docExpansion: 'list', deepLinking: true },
  })
})
