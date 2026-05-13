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
  resolve_op_id: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
})

const ResolveParamsSchema = Type.Object({
  opId: Type.String({ minLength: 1 }),
})

const ResolveResponseSchema = Type.Object({
  ok: Type.Literal(true),
  status: Type.Union([Type.Literal('applied'), Type.Literal('duplicate')]),
})

const ConflictRowSchema = Type.Object({
  op_id: Type.String(),
  entity: Type.String(),
  entity_id: Type.String(),
  server_payload: Type.Unknown(),
  local_payload: Type.Unknown(),
  reason: Type.String(),
  resolved_at: Type.Union([Type.String(), Type.Null()]),
})

const ConflictsListResponseSchema = Type.Object({
  conflicts: Type.Array(ConflictRowSchema),
})

const ErrorRef = Type.Ref('ErrorResponse')

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.get('/sync/conflicts', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['sync'],
      summary: 'List parked conflicts',
      description: `Returns ONLY unresolved conflicts for the tenant. Newest
parked-at first. Page size hard cap: 100. A future
\`GET /sync/conflicts/history?from=&to=\` will expose resolved rows; v1
the resolver UI only needs the open queue (phase-08 §7.11).`,
      security: [{ bearerAuth: [] }],
      response: {
        200: ConflictsListResponseSchema,
        401: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request) => {
      const tenantId = request.tenantId
      const rows = await fastify.conflictsRepo.listOpenConflicts(tenantId)
      return {
        conflicts: rows.map((c) => ({
          op_id: c.opId,
          entity: c.entity,
          entity_id: c.entityId,
          server_payload: c.serverPayload,
          local_payload: c.localPayload,
          reason: c.reason,
          resolved_at: c.resolvedAt,
        })),
      }
    },
  })

  app.post('/sync/conflicts/:opId/resolve', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['sync'],
      summary: 'Resolve a parked conflict',
      description: `Manual conflict resolution. Picks one of:
- \`local\`: re-apply the client's local payload.
- \`server\`: discard the client op, keep the server row.
- \`merged\`: apply the supplied merged payload (must validate against the entity schema).

The client SHOULD include a \`resolve_op_id\` derived as
\`sha256(op_id|choice|canonical_merged_json)\`. The server caches the
response keyed by that id so a retry after a mid-flight network failure
returns \`duplicate\` instead of double-applying (phase-08 §7.22).

Returns \`409 ALREADY_RESOLVED\` if a DIFFERENT resolution arrives after
the first one committed.`,
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
      const deviceId = (request.headers['x-device-id'] as string | undefined) ?? 'unknown'
      const outcome = await fastify.conflictService.resolve(
        request.params.opId,
        {
          choice: request.body.choice,
          merged: request.body.merged,
          resolveOpId: request.body.resolve_op_id,
        },
        userId,
        tenantId,
        deviceId
      )
      return { ok: true as const, status: outcome.status }
    },
  })
}

export default route
