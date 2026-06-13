import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-x'
const USER_ID = '00000000-0000-7000-8000-000000000001'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string }>
}

function authToken (app: FastifyAppLike, role: 'superadmin' | 'receptionist' = 'receptionist'): string {
  return app.jwt.sign({
    sub: USER_ID,
    email: 'reception@example.com',
    entityId: TENANT,
    role,
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

function shiftPayload (id: string, overrides: Record<string, unknown> = {}) {
  const now = new Date().toISOString()
  return {
    id,
    operator_id: '01000000-0000-7000-8000-000000000ABC',
    check_in_at: now,
    check_out_at: null,
    check_in_by_user_id: USER_ID,
    check_out_by_user_id: null,
    note: null,
    entity_id: TENANT,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-1',
    ...overrides,
  }
}

test('POST /sync/push accepts an operator_shifts row (receptionist)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike, 'receptionist')
  const opId = '01HZSH00000000000000000001'
  const shiftId = '01100000-0000-7000-8000-000000000001'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [jsonOp(opId, 'operator_shifts', shiftId, shiftPayload(shiftId))] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 1)
  assert.strictEqual(body.accepted[0].status, 'applied')
})

test('POST /sync/push rejects operator_shifts with check_out_at < check_in_at', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const shiftId = '01100000-0000-7000-8000-000000000002'
  const checkIn = new Date()
  const checkOut = new Date(checkIn.getTime() - 60 * 60 * 1000)
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp(
          '01HZSH00000000000000000002',
          'operator_shifts',
          shiftId,
          shiftPayload(shiftId, {
            check_in_at: checkIn.toISOString(),
            check_out_at: checkOut.toISOString(),
            check_out_by_user_id: USER_ID,
          })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZSH00000000000000000002')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('POST /sync/push rejects operator_shifts from accountant role', async (t) => {
  const app = await build(t)
  const token = (app as unknown as FastifyAppLike).jwt.sign({
    sub: USER_ID,
    email: 'acct@example.com',
    entityId: TENANT,
    role: 'accountant',
  })
  const shiftId = '01100000-0000-7000-8000-000000000003'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [jsonOp('01HZSH00000000000000000003', 'operator_shifts', shiftId, shiftPayload(shiftId))],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZSH00000000000000000003')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 403)
})

test('POST /sync/push is idempotent on op_id replay', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const shiftId = '01100000-0000-7000-8000-000000000004'
  const opId = '01HZSH00000000000000000004'
  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [jsonOp(opId, 'operator_shifts', shiftId, shiftPayload(shiftId))] },
  })
  assert.strictEqual(first.statusCode, 200)
  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [jsonOp(opId, 'operator_shifts', shiftId, shiftPayload(shiftId))] },
  })
  assert.strictEqual(second.statusCode, 200)
  const body = JSON.parse(second.payload)
  assert.strictEqual(body.accepted[0].status, 'duplicate')
})

test('GET /sync/pull surfaces shifts (including soft-deleted tombstones)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const shiftId = '01100000-0000-7000-8000-000000000005'

  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp(
          '01HZSH00000000000000000005',
          'operator_shifts',
          shiftId,
          shiftPayload(shiftId, { note: 'opening shift' })
        ),
      ],
    },
  })

  // Soft-delete via a second push with a bumped version + deleted_at.
  const now = new Date().toISOString()
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp(
          '01HZSH00000000000000000006',
          'operator_shifts',
          shiftId,
          shiftPayload(shiftId, {
            version: 2,
            updated_at: now,
            deleted_at: now,
          })
        ),
      ],
    },
  })

  const res = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-2' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  const shifts = body.changes.filter((c: { entity: string }) => c.entity === 'operator_shifts')
  assert.strictEqual(shifts.length, 1)
  assert.strictEqual(shifts[0].entity_id, shiftId)
  // Tombstone propagates (additive policy + §7.9 soft-delete rule).
  assert.notStrictEqual((shifts[0].payload as { deleted_at: string | null }).deleted_at, null)
})

test('LWW resolves to the higher version on update', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const shiftId = '01100000-0000-7000-8000-000000000006'

  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp(
          '01HZSH00000000000000000007',
          'operator_shifts',
          shiftId,
          shiftPayload(shiftId, { note: 'first', version: 1 })
        ),
      ],
    },
  })
  assert.strictEqual(first.statusCode, 200)

  // Bump version + updated_at; second wins.
  const tomorrow = new Date(Date.now() + 60_000).toISOString()
  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        jsonOp(
          '01HZSH00000000000000000008',
          'operator_shifts',
          shiftId,
          shiftPayload(shiftId, {
            note: 'second',
            version: 2,
            updated_at: tomorrow,
          })
        ),
      ],
    },
  })
  assert.strictEqual(second.statusCode, 200)

  const pull = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-9' },
  })
  const body = JSON.parse(pull.payload)
  const shifts = body.changes.filter(
    (c: { entity: string, entity_id: string }) =>
      c.entity === 'operator_shifts' && c.entity_id === shiftId
  )
  assert.strictEqual(shifts.length, 1)
  assert.strictEqual((shifts[0].payload as { note: string | null }).note, 'second')
})
