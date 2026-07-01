import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

import { ReportsService } from '../service/reports-service'

/**
 * Reports routes (phase-07 §3 Server).
 *
 * Both routes are tenant-scoped and require the accountant or superadmin
 * role. The desktop client routes to the server only for cross-90-day
 * windows (§7.16) or when the local DB is missing the target day.
 */

const ErrorRef = Type.Ref('ErrorResponse')

// Phase-09 §3.1 contract slice: schemas exported so the Ajv-equivalent
// (`Value.Check`) harness can drift-test the wire shape without
// re-declaring it.
export const VisitsQuerySchema = Type.Object({
  groupBy: Type.Optional(Type.Union([
    Type.Literal('none'),
    Type.Literal('by_date'),
    Type.Literal('by_doctor'),
    Type.Literal('by_operator'),
    Type.Literal('by_check_type'),
    Type.Literal('by_subtype'),
    Type.Literal('by_status'),
  ])),
  from: Type.String({ format: 'date-time' }),
  to: Type.String({ format: 'date-time' }),
  tz: Type.Optional(Type.String({ default: 'Asia/Baghdad' })),
  statuses: Type.Optional(Type.Array(Type.Union([
    Type.Literal('draft'),
    Type.Literal('locked'),
    Type.Literal('voided'),
  ]))),
  checkTypeIds: Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
  subtypeIds: Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
  doctorIds: Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
  operatorIds: Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
  dye: Type.Optional(Type.Union([
    Type.Literal('y'),
    Type.Literal('n'),
    Type.Literal('all'),
  ])),
  report: Type.Optional(Type.Union([
    Type.Literal('y'),
    Type.Literal('n'),
    Type.Literal('all'),
  ])),
  includeHouse: Type.Optional(Type.Boolean()),
  includeVoided: Type.Optional(Type.Boolean()),
  limit: Type.Optional(Type.String({ pattern: '^\\d+$' })),
})

export const TotalsSchema = Type.Object({
  visits: Type.Integer(),
  revenue_iqd: Type.Integer(),
  doctor_cut_iqd: Type.Integer(),
  operator_cut_iqd: Type.Integer(),
  report_iqd: Type.Integer(),
  mandoub_cut_iqd: Type.Integer(),
  net_iqd: Type.Integer(),
})

export const RowSchema = Type.Object({
  visit_id: Type.String(),
  locked_at: Type.Union([Type.String(), Type.Null()]),
  status: Type.String(),
  patient_name: Type.String(),
  doctor_name: Type.Union([Type.String(), Type.Null()]),
  operator_name: Type.String(),
  check_type_name_ar: Type.String(),
  check_type_name_en: Type.Union([Type.String(), Type.Null()]),
  check_subtype_name_ar: Type.Union([Type.String(), Type.Null()]),
  check_subtype_name_en: Type.Union([Type.String(), Type.Null()]),
  dye: Type.Boolean(),
  report: Type.Boolean(),
  price_iqd: Type.Integer(),
  doctor_cut_iqd: Type.Integer(),
  operator_cut_iqd: Type.Integer(),
  report_iqd: Type.Integer(),
  mandoub_cut_iqd: Type.Integer(),
  net_iqd: Type.Integer(),
})

export const GroupSchema = Type.Object({
  key: Type.String(),
  label: Type.String(),
  visits: Type.Integer(),
  revenue_iqd: Type.Integer(),
  doctor_cut_iqd: Type.Integer(),
  operator_cut_iqd: Type.Integer(),
  report_iqd: Type.Integer(),
  mandoub_cut_iqd: Type.Integer(),
  net_iqd: Type.Integer(),
})

export const VisitsResponseSchema = Type.Union([
  Type.Object({
    mode: Type.Literal('rows'),
    rows: Type.Array(RowSchema),
    totals: TotalsSchema,
  }),
  Type.Object({
    mode: Type.Literal('groups'),
    groups: Type.Array(GroupSchema),
    totals: TotalsSchema,
  }),
])

export const DailyCloseParamsSchema = Type.Object({
  date: Type.String({ pattern: '^\\d{4}-\\d{2}-\\d{2}$' }),
})

export const DailyCloseQuerySchema = Type.Object({
  // Fastify's TypeBox compiler does not coerce query-string integers by
  // default; accept as string and parse server-side. Default = 180 minutes
  // (Asia/Baghdad UTC+03:00).
  tzOffsetMinutes: Type.Optional(Type.String({ pattern: '^-?\\d+$' })),
})

export const DailyCloseResponseSchema = Type.Object({
  tenant_id: Type.String(),
  target_date: Type.String(),
  tz_offset: Type.String(),
  total_revenue_iqd: Type.Integer(),
  total_doctor_cuts_iqd: Type.Integer(),
  total_operator_cuts_iqd: Type.Integer(),
  total_report_iqd: Type.Integer(),
  total_mandoub_cuts_iqd: Type.Integer(),
  total_inventory_consumption_value_iqd: Type.Integer(),
  net_iqd: Type.Integer(),
  locked_count: Type.Integer(),
  voided_count: Type.Integer(),
  voided_value_iqd: Type.Integer(),
  per_doctor: Type.Array(Type.Object({
    doctor_id: Type.Union([Type.String(), Type.Null()]),
    name: Type.String(),
    visits: Type.Integer(),
    revenue_iqd: Type.Integer(),
    doctor_cut_iqd: Type.Integer(),
  })),
  per_operator: Type.Array(Type.Object({
    operator_id: Type.String(),
    name: Type.String(),
    visits: Type.Integer(),
    dye_visits: Type.Integer(),
    operator_cut_iqd: Type.Integer(),
    hours_on_shift_milli: Type.Integer(),
  })),
  per_mandoub: Type.Array(Type.Object({
    mandoub_id: Type.String(),
    name: Type.String(),
    visits: Type.Integer(),
    mandoub_cut_iqd: Type.Integer(),
  })),
  per_check_type: Type.Array(Type.Object({
    check_type_id: Type.String(),
    name_ar: Type.String(),
    name_en: Type.Union([Type.String(), Type.Null()]),
    visits: Type.Integer(),
    revenue_iqd: Type.Integer(),
    doctor_cut_iqd: Type.Integer(),
    operator_cut_iqd: Type.Integer(),
  })),
  generated_at: Type.String(),
})

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()
  const service = new ReportsService(fastify.entityStore)

  app.get('/reports/visits', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['reports'],
      summary: 'Visits report aggregate / rows',
      description: `Server-side rollup of the Visits Report (PRD §7.2.2).

- Tenant-scoped: filters on JWT \`entityId\`.
- Role: accountant or superadmin only.
- \`groupBy\` switches between row-per-visit and group aggregates.
- Status defaults to \`locked\` only; pass \`includeVoided=true\` or
  explicit \`statuses\` to include voided rows.
- The desktop client routes here for ranges longer than 90 days (§7.16).`,
      security: [{ bearerAuth: [] }],
      querystring: VisitsQuerySchema,
      response: {
        200: VisitsResponseSchema,
        401: ErrorRef,
        403: ErrorRef,
        422: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request, reply) => {
      const role = (request.user as { role?: string }).role
      if (role !== 'accountant' && role !== 'superadmin') {
        return reply.forbidden('reports require accountant or superadmin role')
      }
      const q = request.query
      return service.visits({
        tenantId: request.tenantId,
        from: q.from,
        to: q.to,
        includeVoided: q.includeVoided ?? false,
        statuses: q.statuses,
        checkTypeIds: q.checkTypeIds,
        subtypeIds: q.subtypeIds,
        doctorIds: q.doctorIds,
        operatorIds: q.operatorIds,
        includeHouse: q.includeHouse,
        dye: q.dye,
        report: q.report,
        groupBy: q.groupBy,
        limit: q.limit != null ? parseInt(q.limit, 10) : undefined,
      })
    },
  })

  app.get('/reports/daily-close/:date', {
    onRequest: [fastify.authenticate, fastify.requireEntityContext],
    schema: {
      tags: ['reports'],
      summary: 'Authoritative daily close for one tz-local calendar day',
      description: `Daily close artifact (PRD §7.2.5).

- Tenant-scoped; role: accountant or superadmin.
- \`:date\` is a YYYY-MM-DD calendar day in the operator's local tz.
- \`tzOffsetMinutes\` overrides the default Baghdad +03:00 = 180.
- Read-only -- never mutates server state.`,
      security: [{ bearerAuth: [] }],
      params: DailyCloseParamsSchema,
      querystring: DailyCloseQuerySchema,
      response: {
        200: DailyCloseResponseSchema,
        401: ErrorRef,
        403: ErrorRef,
        422: ErrorRef,
        500: ErrorRef,
      },
    },
    handler: async (request, reply) => {
      const role = (request.user as { role?: string }).role
      if (role !== 'accountant' && role !== 'superadmin') {
        return reply.forbidden('reports require accountant or superadmin role')
      }
      const raw = request.query.tzOffsetMinutes
      const tzOffset = raw != null ? parseInt(raw, 10) : 180
      return service.dailyClose(request.tenantId, request.params.date, tzOffset)
    },
  })
}

export default route
