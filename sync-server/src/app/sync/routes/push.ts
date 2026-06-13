import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

import { PushBodySchema, PushResponseSchema } from '../presentation/schemas/push.js'

const ErrorRef = Type.Ref('ErrorResponse')

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.post('/sync/push', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['sync'],
      summary: 'Apply a batch of client ops',
      description: `Apply a batch of outbox operations from a client device.

- Idempotent on \`op_id\` (replays cached response on repeat).
- Phase-01 accepts \`entity = audit_log\` only; other entities return 422.
- \`op\` must be \`upsert\`; \`delete\` returns 422 \`UNSUPPORTED_OP\`.
- Audit rows with non-null \`deleted_at\` are rejected with 422 \`AUDIT_IMMUTABLE\`.`,
      security: [{ bearerAuth: [] }],
      body: PushBodySchema,
      response: {
        200: PushResponseSchema,
        401: ErrorRef,
        403: ErrorRef,
        422: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request) => {
      const tenantId = request.tenantId
      const deviceId = (request.headers['x-device-id'] as string | undefined) ?? 'unknown'
      const actor = request.user as
        | { sub?: string; role?: 'superadmin' | 'receptionist' | 'accountant'; entityId?: string }
        | undefined
      const actorClaims = actor?.sub && actor.role && actor.entityId
        ? { sub: actor.sub, role: actor.role, entityId: actor.entityId }
        : undefined
      const result = await fastify.pushService.apply(
        request.body.ops,
        tenantId,
        deviceId,
        actorClaims
      )
      return {
        accepted: result.accepted,
        conflicts: result.conflicts.map((c) => ({
          op_id: c.opId,
          entity: c.entity,
          entity_id: c.entityId,
          server_payload: c.serverPayload,
          local_payload: c.localPayload,
          reason: c.reason,
        })),
        rejected: result.rejected,
      }
    },
  })
}

export default route
