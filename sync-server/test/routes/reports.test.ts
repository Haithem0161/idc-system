import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-r'
const USER_ID = '01900000-0000-7000-8000-000000000001'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string }>
}

function authToken (
  app: FastifyAppLike,
  role: 'superadmin' | 'receptionist' | 'accountant' = 'accountant'
): string {
  return app.jwt.sign({
    sub: USER_ID,
    email: 'acc@example.com',
    entityId: TENANT,
    role,
  })
}

function jsonOp (
  opId: string,
  entity: string,
  entityId: string,
  payload: Record<string, unknown>
) {
  return {
    op_id: opId,
    entity,
    entity_id: entityId,
    op: 'upsert' as const,
    payload_b64: Buffer.from(JSON.stringify(payload)).toString('base64'),
  }
}

function patientPayload (id: string) {
  const now = new Date().toISOString()
  return {
    id,
    name: 'Pat',
    entity_id: TENANT,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-r',
  }
}

function lockedVisitPayload (
  id: string,
  patientId: string,
  lockedAt: string,
  doctorId: string | null = null,
  doctorCut = 20000
) {
  const now = lockedAt
  return {
    id,
    patient_id: patientId,
    status: 'locked',
    receptionist_user_id: USER_ID,
    check_type_id: '01900000-0000-7000-8000-00000000A001',
    check_subtype_id: null,
    doctor_id: doctorId,
    operator_id: '01900000-0000-7000-8000-00000000B001',
    dye: false,
    report: false,
    locked_at: lockedAt,
    voided_at: null,
    voided_by_user_id: null,
    void_reason: null,
    price_snapshot_iqd: 50000,
    dye_cost_snapshot_iqd: 0,
    report_amount_snapshot_iqd: 0,
    doctor_cut_snapshot_iqd: doctorCut,
    operator_cut_snapshot_iqd: 5000,
    internal_pct_snapshot: doctorId == null ? 40 : null,
    total_amount_iqd_snapshot: 50000,
    patient_name_snapshot: 'Pat',
    doctor_name_snapshot: doctorId == null ? null : 'Dr A',
    operator_name_snapshot: 'Op One',
    check_type_name_ar_snapshot: 'فحص',
    check_type_name_en_snapshot: 'Test',
    check_subtype_name_ar_snapshot: null,
    check_subtype_name_en_snapshot: null,
    entity_id: TENANT,
    version: 2,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-r',
  }
}

async function seedTwoVisits (
  app: FastifyAppLike,
  token: string,
  isoDate: string
): Promise<void> {
  const patientId = '01900000-0000-7000-8000-000000000101'
  const visit1 = '01900000-0000-7000-8000-000000000201'
  const visit2 = '01900000-0000-7000-8000-000000000202'
  const doctor1 = '01900000-0000-7000-8000-00000000C001'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-r' },
    payload: {
      ops: [
        jsonOp('01HZRP00000000000000000001', 'patients', patientId, patientPayload(patientId)),
        jsonOp('01HZRP00000000000000000002', 'visits', visit1, lockedVisitPayload(visit1, patientId, isoDate, doctor1, 20000)),
        jsonOp('01HZRP00000000000000000003', 'visits', visit2, lockedVisitPayload(visit2, patientId, isoDate, null, 0)),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
}

test('GET /reports/visits returns rows + totals', async (t) => {
  const app = await build(t) as unknown as FastifyAppLike
  const token = authToken(app, 'superadmin')
  const isoDate = new Date().toISOString()
  await seedTwoVisits(app, token, isoDate)

  // Wide range so both visits fall in.
  const from = new Date(Date.now() - 24 * 3600 * 1000).toISOString()
  const to = new Date(Date.now() + 24 * 3600 * 1000).toISOString()
  const res = await app.inject({
    method: 'GET',
    url: `/reports/visits?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.mode, 'rows')
  assert.strictEqual(body.rows.length, 2)
  assert.strictEqual(body.totals.visits, 2)
  // Sum of price_snapshot_iqd = 50000 + 50000 = 100000.
  assert.strictEqual(body.totals.revenue_iqd, 100000)
  // Sum of doctor cuts: 20000 + 0 = 20000.
  assert.strictEqual(body.totals.doctor_cut_iqd, 20000)
})

test('GET /reports/visits groupBy=by_doctor returns grouped totals', async (t) => {
  const app = await build(t) as unknown as FastifyAppLike
  const seedToken = authToken(app, 'superadmin')
  const token = authToken(app, 'accountant')
  const isoDate = new Date().toISOString()
  await seedTwoVisits(app, seedToken, isoDate)
  const from = new Date(Date.now() - 24 * 3600 * 1000).toISOString()
  const to = new Date(Date.now() + 24 * 3600 * 1000).toISOString()
  const res = await app.inject({
    method: 'GET',
    url: `/reports/visits?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&groupBy=by_doctor`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.mode, 'groups')
  // Two groups: a named doctor + the house pseudo-row.
  assert.strictEqual(body.groups.length, 2)
})

test('GET /reports/visits rejects receptionist role with 403', async (t) => {
  const app = await build(t) as unknown as FastifyAppLike
  const token = authToken(app, 'receptionist')
  const from = new Date(Date.now() - 24 * 3600 * 1000).toISOString()
  const to = new Date(Date.now() + 24 * 3600 * 1000).toISOString()
  const res = await app.inject({
    method: 'GET',
    url: `/reports/visits?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 403, res.payload)
})

test('GET /reports/visits requires auth (401)', async (t) => {
  const app = await build(t) as unknown as FastifyAppLike
  const from = new Date(Date.now() - 24 * 3600 * 1000).toISOString()
  const to = new Date(Date.now() + 24 * 3600 * 1000).toISOString()
  const res = await app.inject({
    method: 'GET',
    url: `/reports/visits?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
  })
  assert.strictEqual(res.statusCode, 401, res.payload)
})

test('GET /reports/daily-close/:date returns breakdowns', async (t) => {
  const app = await build(t) as unknown as FastifyAppLike
  const seedToken = authToken(app, 'superadmin')
  const token = authToken(app, 'accountant')
  // Seed visits with a deterministic Baghdad-day timestamp: 12:00 local on
  // 2026-05-12 = 09:00 UTC.
  const localDate = '2026-05-12'
  const lockedAtUtc = '2026-05-12T09:00:00.000Z'
  const patientId = '01900000-0000-7000-8000-000000000101'
  const visit1 = '01900000-0000-7000-8000-000000000201'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${seedToken}`, 'x-device-id': 'dev-r' },
    payload: {
      ops: [
        jsonOp('01HZDC00000000000000000001', 'patients', patientId, patientPayload(patientId)),
        jsonOp('01HZDC00000000000000000002', 'visits', visit1, lockedVisitPayload(visit1, patientId, lockedAtUtc, null, 0)),
      ],
    },
  })

  const res = await app.inject({
    method: 'GET',
    url: `/reports/daily-close/${localDate}?tzOffsetMinutes=180`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.tenant_id, TENANT)
  assert.strictEqual(body.target_date, localDate)
  assert.strictEqual(body.tz_offset, '+03:00')
  assert.strictEqual(body.locked_count, 1)
  assert.strictEqual(body.total_revenue_iqd, 50000)
  // House pseudo-row in per_doctor.
  assert.strictEqual(body.per_doctor.length, 1)
  assert.strictEqual(body.per_doctor[0].doctor_id, null)
})

test('GET /reports/daily-close excludes visits outside the local-day window', async (t) => {
  const app = await build(t) as unknown as FastifyAppLike
  const seedToken = authToken(app, 'superadmin')
  const token = authToken(app, 'accountant')
  // Visit locked 5 minutes BEFORE 2026-05-12 +03:00 day-start (which is
  // 2026-05-11T21:00:00Z) -- so it belongs to 2026-05-11, not 2026-05-12.
  const lockedAtUtc = '2026-05-11T20:55:00.000Z'
  const patientId = '01900000-0000-7000-8000-000000000101'
  const visit1 = '01900000-0000-7000-8000-000000000201'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${seedToken}`, 'x-device-id': 'dev-r' },
    payload: {
      ops: [
        jsonOp('01HZDC00000000000000000003', 'patients', patientId, patientPayload(patientId)),
        jsonOp('01HZDC00000000000000000004', 'visits', visit1, lockedVisitPayload(visit1, patientId, lockedAtUtc, null, 0)),
      ],
    },
  })
  const res = await app.inject({
    method: 'GET',
    url: '/reports/daily-close/2026-05-12?tzOffsetMinutes=180',
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.locked_count, 0)
})
