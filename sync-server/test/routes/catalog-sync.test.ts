import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-x'
const USER_ID = '00000000-0000-7000-8000-000000000001'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string }>
}

function authToken (app: FastifyAppLike): string {
  return app.jwt.sign({
    sub: USER_ID,
    email: 'dev@example.com',
    entityId: TENANT,
    role: 'superadmin',
  })
}

function jsonOp (opId: string, entity: string, entityId: string, payload: Record<string, unknown>) {
  return {
    op_id: opId,
    entity,
    entity_id: entityId,
    op: 'upsert' as const,
    payload_b64: Buffer.from(JSON.stringify(payload)).toString('base64'),
  }
}

function checkTypePayload (id: string, overrides: Record<string, unknown> = {}) {
  const now = new Date().toISOString()
  return {
    id,
    name_ar: 'Ultrasound',
    name_en: 'Ultrasound',
    has_subtypes: false,
    base_price_iqd: 30_000,
    dye_supported: false,
    report_supported: false,
    sort_order: 0,
    is_active: true,
    entity_id: TENANT,
    version: 1,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-1',
    ...overrides,
  }
}

test('POST /sync/push accepts a check_types payload (superadmin only)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZAA00000000000000000001'
  const ctId = '01000000-0000-7000-8000-000000000001'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [jsonOp(opId, 'check_types', ctId, checkTypePayload(ctId))] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 1)
  assert.strictEqual(body.accepted[0].status, 'applied')
})

test('POST /sync/push rejects check_types with XOR violation', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZAA00000000000000000002'
  const ctId = '01000000-0000-7000-8000-000000000002'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp(
          opId,
          'check_types',
          ctId,
          checkTypePayload(ctId, { has_subtypes: true, base_price_iqd: 1000 })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, opId)
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('POST /sync/push rejects catalog push when role is not superadmin', async (t) => {
  const app = await build(t)
  const token = (app as unknown as FastifyAppLike).jwt.sign({
    sub: USER_ID,
    email: 'dev@example.com',
    entityId: TENANT,
    role: 'receptionist',
  })
  const opId = '01HZAA00000000000000000003'
  const ctId = '01000000-0000-7000-8000-000000000003'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [jsonOp(opId, 'check_types', ctId, checkTypePayload(ctId))] },
  })
  // Role guard surfaces as a per-op rejection (403 status_code) inside the 200 envelope.
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, opId)
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 403)
})

test('Pull returns previously-pushed catalog rows for a new device', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZAA00000000000000000004'
  const ctId = '01000000-0000-7000-8000-000000000004'

  // Push a check_type.
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [jsonOp(opId, 'check_types', ctId, checkTypePayload(ctId, { name_ar: 'Brain MRI' }))] },
  })

  // Pull from a fresh device.
  const res = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-2' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  const checkTypes = body.changes.filter((c: { entity: string }) => c.entity === 'check_types')
  assert.strictEqual(checkTypes.length, 1)
  assert.strictEqual(checkTypes[0].entity_id, ctId)
})

test('POST /sync/push rejects doctor_check_pricing when parent has_subtypes mismatches', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)

  // Push a flat parent check_type first.
  const ctId = '01000000-0000-7000-8000-000000000010'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp('01HZBB00000000000000000001', 'check_types', ctId, checkTypePayload(ctId, {
          has_subtypes: false,
          base_price_iqd: 25_000,
        })),
      ],
    },
  })

  // Doctor pricing with a non-null subtype against a flat parent should be rejected.
  const docId = '01000000-0000-7000-8000-000000000011'
  const pricingId = '01000000-0000-7000-8000-000000000012'
  const subId = '01000000-0000-7000-8000-000000000013'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp('01HZBB00000000000000000002', 'doctors', docId, {
          id: docId,
          name: 'Dr. Test',
          specialty: null,
          phone: null,
          is_active: true,
          notes: null,
          entity_id: TENANT,
          version: 1,
          updated_at: new Date().toISOString(),
          deleted_at: null,
          origin_device_id: 'dev-1',
        }),
      ],
    },
  })

  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp('01HZBB00000000000000000003', 'doctor_check_pricing', pricingId, {
          id: pricingId,
          doctor_id: docId,
          check_type_id: ctId,
          check_subtype_id: subId,
          price_override_iqd: 20_000,
          cut_kind: 'pct',
          cut_value: 30,
          entity_id: TENANT,
          version: 1,
          updated_at: new Date().toISOString(),
          deleted_at: null,
          origin_device_id: 'dev-1',
        }),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZBB00000000000000000003')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})
