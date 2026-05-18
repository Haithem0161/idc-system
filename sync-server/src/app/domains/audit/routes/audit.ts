import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

import { DomainError } from '../../../common/errors/domain'
import { AuditQueryService } from '../service/audit-service'

/**
 * GET /audit/query -- phase-08 §3 Server, §7.6.
 *
 * Superadmin-only. Returns the tenant's audit rows filtered by the supplied
 * query string. Sorts `(at DESC, id DESC)` for stability (§7.5); paginates
 * with a base64url cursor encoded as `{ at, id }`. Default page size 50,
 * hard cap 100. The 14-action union mirrors phase-01 §7.36 + phase-07 §7.18
 * `daily_close_run`.
 */

// Phase-09 §3.1 contract slice: exported so the Ajv-equivalent
// `Value.Check` harness can drift-test the audit query schemas.
export const ACTION_VALUES = [
  'create', 'update', 'soft_delete', 'lock', 'void', 'discard',
  'clock_in', 'clock_out', 'password_change', 'login', 'logout',
  'conflict_resolve', 'vacuum', 'daily_close_run',
] as const

export const ENTITY_VALUES = [
  'users', 'settings', 'check_types', 'check_subtypes', 'doctors',
  'doctor_check_pricing', 'operators', 'operator_specialties',
  'operator_shifts', 'patients', 'visits', 'inventory_items',
  'inventory_consumption_map', 'inventory_adjustments', 'audit_log',
] as const

export const AuditQuerySchema = Type.Object({
  from: Type.String({ format: 'date-time' }),
  to: Type.String({ format: 'date-time' }),
  actor: Type.Optional(Type.String({ format: 'uuid' })),
  action: Type.Optional(Type.Union(ACTION_VALUES.map((v) => Type.Literal(v)))),
  entity: Type.Optional(Type.Union(ENTITY_VALUES.map((v) => Type.Literal(v)))),
  entity_id_prefix: Type.Optional(Type.String({ minLength: 4, maxLength: 36 })),
  text: Type.Optional(Type.String({ minLength: 2, maxLength: 100 })),
  cursor: Type.Optional(Type.String()),
  limit: Type.Optional(Type.String({ pattern: '^\\d+$' })),
})

export const AuditRowSchema = Type.Object({
  id: Type.String(),
  actor_user_id: Type.String(),
  action: Type.String(),
  entity: Type.String(),
  entity_id: Type.String(),
  delta: Type.Unknown(),
  ip: Type.Union([Type.String(), Type.Null()]),
  device_id: Type.String(),
  at: Type.String(),
  version: Type.Integer(),
  entity_id_tenant: Type.String(),
})

export const AuditQueryResponseSchema = Type.Object({
  rows: Type.Array(AuditRowSchema),
  next_cursor: Type.Union([Type.String(), Type.Null()]),
})

const ErrorRef = Type.Ref('ErrorResponse')

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()
  const service = new AuditQueryService(fastify.auditQueryRepo)

  app.get('/audit/query', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['audit'],
      summary: 'Search audit rows server-side (admin only)',
      description: `Server-side audit search. Used by the Tauri client to
fan out across the 90-day local cliff (phase-08 §7.4). Superadmin-only.

Sort order: \`(at DESC, id DESC)\`. Cursor: base64url-encoded
\`{at, id}\`. Default page size 50; hard cap 100. \`text\` is a
substring match against \`delta\` JSON and \`entity_id\`; full-text
indexing is deferred to Horizon-1.`,
      security: [{ bearerAuth: [] }],
      querystring: AuditQuerySchema,
      response: {
        200: AuditQueryResponseSchema,
        401: ErrorRef,
        403: ErrorRef,
        422: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request) => {
      const actor = request.user as { role?: string } | undefined
      if (actor?.role !== 'superadmin') {
        throw new DomainError(
          'VALIDATION_ERROR',
          'audit query requires superadmin role',
          403
        )
      }
      const tenantId = request.tenantId
      const q = request.query
      const result = await service.query(
        {
          from: q.from,
          to: q.to,
          actor: q.actor,
          action: q.action,
          entity: q.entity,
          entityIdPrefix: q.entity_id_prefix,
          text: q.text,
          cursor: q.cursor,
          limit: q.limit ? Number.parseInt(q.limit, 10) : undefined,
        },
        tenantId
      )
      return {
        rows: result.rows.map((r) => ({
          id: r.id,
          actor_user_id: r.actor_user_id,
          action: r.action,
          entity: r.entity,
          entity_id: r.entity_id,
          delta: r.delta,
          ip: r.ip ?? null,
          device_id: r.device_id,
          at: r.at,
          version: r.version,
          entity_id_tenant: r.entity_id_tenant,
        })),
        next_cursor: result.nextCursor,
      }
    },
  })
}

export default route
