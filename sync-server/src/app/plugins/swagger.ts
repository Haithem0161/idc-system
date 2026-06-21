import fp from 'fastify-plugin'
import swagger from '@fastify/swagger'
import swaggerUi from '@fastify/swagger-ui'

import { ErrorResponseSchema } from '../common/schemas/error'
import { SERVER_VERSION } from '../common/version.js'

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
        version: SERVER_VERSION,
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

  // Serve the interactive Swagger UI only OUTSIDE production. In production it
  // would publicly enumerate the entire API surface (auth, sync, reports,
  // audit, every schema) to any unauthenticated visitor -- information
  // disclosure that eases targeted attacks. The OpenAPI document is still
  // generated (so `/documentation/json` machinery and types stay intact); only
  // the public UI is withheld on a real clinic deployment.
  if (!fastify.appEnv.isProduction) {
    await fastify.register(swaggerUi, {
      routePrefix: '/documentation',
      uiConfig: { docExpansion: 'list', deepLinking: true },
    })
  }
}, { name: 'swagger', dependencies: ['env'] })
