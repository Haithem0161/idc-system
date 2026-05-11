import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

const PushOpSchema = Type.Object({
  op_id: Type.String({ minLength: 1 }),
  entity: Type.String({ minLength: 1 }),
  entity_id: Type.String({ minLength: 1 }),
  op: Type.Literal('upsert'),
  payload_b64: Type.String({ minLength: 1 }),
})

const PushBodySchema = Type.Object({
  ops: Type.Array(PushOpSchema, { minItems: 1, maxItems: 200 }),
})

const PushResponseSchema = Type.Object({
  accepted: Type.Array(
    Type.Object({
      op_id: Type.String(),
      status: Type.Union([Type.Literal('applied'), Type.Literal('duplicate')]),
    })
  ),
  conflicts: Type.Array(
    Type.Object({
      op_id: Type.String(),
      entity: Type.String(),
      entity_id: Type.String(),
      server_payload: Type.Unknown(),
      local_payload: Type.Unknown(),
      reason: Type.String(),
    })
  ),
})

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
      const result = await fastify.pushService.apply(request.body.ops, tenantId, deviceId)
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
      }
    },
  })
}

export default route
