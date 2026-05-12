import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-inv'
const USER_ID = '01900000-0000-7000-8000-000000000010'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string }>
}

function authToken (
  app: FastifyAppLike,
  role: 'superadmin' | 'receptionist' | 'accountant' = 'receptionist'
): string {
  return app.jwt.sign({
    sub: USER_ID,
    email: `${role}@example.com`,
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

function adjustment (
  id: string,
  itemId: string,
  overrides: Record<string, unknown> = {}
) {
  const now = new Date().toISOString()
  return {
    id,
    item_id: itemId,
    delta: 5,
    reason: 'receive' as const,
    visit_id: null,
    note: 'box of supplies',
    by_user_id: USER_ID,
    entity_id: TENANT,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-i1',
    ...overrides,
  }
}

test('inventory: receive adjustment from receptionist is applied', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike, 'receptionist')
  const id = '01900000-0000-7000-8000-000000000201'
  const itemId = '01900000-0000-7000-8000-000000000401'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [jsonOp('01HZWP00000000000000000001', 'inventory_adjustments', id, adjustment(id, itemId))],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted[0].status, 'applied')
})

test('inventory: count_correction with delta=0 is rejected (422)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike, 'superadmin')
  const id = '01900000-0000-7000-8000-000000000202'
  const itemId = '01900000-0000-7000-8000-000000000402'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000002',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { reason: 'count_correction', delta: 0 })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 422, res.payload)
})

test('inventory: count_correction from receptionist is forbidden (403)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike, 'receptionist')
  const id = '01900000-0000-7000-8000-000000000203'
  const itemId = '01900000-0000-7000-8000-000000000403'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000003',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { reason: 'count_correction', delta: 3 })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 403, res.payload)
})

test('inventory: count_correction superadmin negative signed delta is applied', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike, 'superadmin')
  const id = '01900000-0000-7000-8000-000000000204'
  const itemId = '01900000-0000-7000-8000-000000000404'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000004',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { reason: 'count_correction', delta: -2 })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted[0].status, 'applied')
})

test('inventory: receive with non-positive delta is rejected (422)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const id = '01900000-0000-7000-8000-000000000205'
  const itemId = '01900000-0000-7000-8000-000000000405'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000005',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { delta: -1 })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 422, res.payload)
})

test('inventory: writeoff with positive delta is rejected (422)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const id = '01900000-0000-7000-8000-000000000206'
  const itemId = '01900000-0000-7000-8000-000000000406'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000006',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { reason: 'writeoff', delta: 5 })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 422, res.payload)
})

test('inventory: adjustment replay returns 409 ADDITIVE_VIOLATION on different op_id', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const id = '01900000-0000-7000-8000-000000000207'
  const itemId = '01900000-0000-7000-8000-000000000407'
  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp('01HZWP00000000000000000007', 'inventory_adjustments', id, adjustment(id, itemId)),
      ],
    },
  })
  assert.strictEqual(first.statusCode, 200, first.payload)

  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000008',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { delta: 99 })
        ),
      ],
    },
  })
  assert.strictEqual(second.statusCode, 409, second.payload)
})

test('inventory: note longer than 500 chars is rejected', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const id = '01900000-0000-7000-8000-000000000208'
  const itemId = '01900000-0000-7000-8000-000000000408'
  const longNote = 'x'.repeat(501)
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-i1' },
    payload: {
      ops: [
        jsonOp(
          '01HZWP00000000000000000009',
          'inventory_adjustments',
          id,
          adjustment(id, itemId, { note: longNote })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 422, res.payload)
})
