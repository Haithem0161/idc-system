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
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZWP00000000000000000002')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
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
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZWP00000000000000000003')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 403)
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
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZWP00000000000000000005')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
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
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZWP00000000000000000006')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
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
  assert.strictEqual(second.statusCode, 200, second.payload)
  const body = JSON.parse(second.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZWP00000000000000000008')
  assert.strictEqual(body.rejected[0].code, 'ADDITIVE_VIOLATION')
  assert.strictEqual(body.rejected[0].status_code, 409)
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
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZWP00000000000000000009')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

// --- Phase-10 T11: cross-tenant inventory isolation -------------------------

test('T11: an adjustment from tenant B cannot read or overwrite tenant A inventory', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike & {
    entityStore: {
      inventoryItems: Map<string, { id: string, quantity_on_hand: number, version: number, entity_id: string }>
      upsertInventoryItem: (row: Record<string, unknown>) => Promise<unknown>
    }
  }

  const TENANT_A = 'tenant-A'
  const TENANT_B = 'tenant-B'
  const itemId = '01900000-0000-7000-8000-0000000004AA'
  const now = new Date().toISOString()

  // Seed tenant A's inventory item with a known on-hand quantity.
  await a.entityStore.upsertInventoryItem({
    id: itemId,
    name_ar: 'مادة',
    name_en: 'Reagent',
    unit: 'box',
    quantity_on_hand: 42,
    low_stock_threshold: 0,
    is_active: true,
    entity_id: TENANT_A,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-A',
  })
  const before = a.entityStore.inventoryItems.get(itemId)
  assert.ok(before, 'tenant A item must be seeded')
  assert.strictEqual(before.quantity_on_hand, 42)
  const beforeVersion = before.version

  // Tenant B pushes a superadmin adjustment that references tenant A's item_id.
  const bToken = app.jwt.sign({
    sub: USER_ID,
    email: 'superadmin@example.com',
    entityId: TENANT_B,
    role: 'superadmin',
  })
  const adjId = '01900000-0000-7000-8000-0000000002BB'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${bToken}`, 'x-device-id': 'dev-B' },
    payload: {
      ops: [jsonOp('01HZWP0000000000000000B001', 'inventory_adjustments', adjId,
        adjustment(adjId, itemId, { entity_id: TENANT_B, by_user_id: USER_ID, reason: 'receive', delta: 100 }))],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)

  // Tenant A's item must be UNTOUCHED: same quantity, same version. The
  // cross-tenant recompute is a no-op because the item belongs to tenant A.
  const after = a.entityStore.inventoryItems.get(itemId)
  assert.ok(after)
  assert.strictEqual(after.quantity_on_hand, 42, 'tenant A on-hand must NOT change')
  assert.strictEqual(after.version, beforeVersion, 'tenant A item version must NOT change')
  assert.strictEqual(after.entity_id, TENANT_A)
})
