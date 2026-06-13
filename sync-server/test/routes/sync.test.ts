import { test } from 'node:test'
import * as assert from 'node:assert'
import { encode as encodeMsgpack } from '@msgpack/msgpack'

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
    role: 'admin',
  })
}

function makeAuditPayload (id: string, opts: Partial<Record<string, unknown>> = {}): string {
  const now = new Date().toISOString()
  const payload = {
    id,
    actor_user_id: USER_ID,
    action: 'create',
    entity: 'user',
    entity_id: 'u1',
    delta: { name: { from: null, to: 'Alice' } },
    ip: null,
    device_id: 'dev-1',
    at: now,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    version: 1,
    last_synced_at: null,
    origin_device_id: 'dev-1',
    entity_id_tenant: TENANT,
    ...opts,
  }
  return Buffer.from(encodeMsgpack(payload)).toString('base64')
}

function makeOp (opId: string, payload: string, op: string = 'upsert', entity: string = 'audit_log') {
  return { op_id: opId, entity, entity_id: 'audit-1', op, payload_b64: payload }
}

test('POST /sync/push without auth returns 401', async (t) => {
  const app = await build(t)
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    payload: { ops: [makeOp('01HZ0000000000000000000001', makeAuditPayload('audit-1'))] },
  })
  assert.strictEqual(res.statusCode, 401)
})

test('POST /sync/push accepts an audit_log batch', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000010'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, makeAuditPayload('audit-1'))] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.deepStrictEqual(body.conflicts, [])
  assert.strictEqual(body.accepted.length, 1)
  assert.strictEqual(body.accepted[0].status, 'applied')
})

test('POST /sync/push is idempotent on op_id', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000020'
  const payload = makeAuditPayload('audit-2')

  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, payload)] },
  })
  assert.strictEqual(first.statusCode, 200)
  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, payload)] },
  })
  assert.strictEqual(second.statusCode, 200)
  const body = JSON.parse(second.payload)
  assert.strictEqual(body.accepted[0].status, 'duplicate')
})

test('POST /sync/push rejects delete op (UNSUPPORTED_OP)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000030'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, makeAuditPayload('audit-3'), 'delete')] },
  })
  assert.strictEqual(res.statusCode, 422)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.code, 'VALIDATION_ERROR') // schema rejects literal first
})

test('POST /sync/push rejects audit_log with deleted_at (AUDIT_IMMUTABLE)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000040'
  const payload = makeAuditPayload('audit-4', { deleted_at: new Date().toISOString() })
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, payload)] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, opId)
  assert.strictEqual(body.rejected[0].code, 'AUDIT_IMMUTABLE')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('GET /sync/pull returns pushed rows in order', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000050'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, makeAuditPayload('audit-5'))] },
  })

  const res = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-2' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.changes.length, 1)
  assert.strictEqual(body.changes[0].entity, 'audit_log')
  assert.ok(body.next_cursor.length > 0)
})

test('POST /sync/lookup-op returns ids that exist', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000060'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeOp(opId, makeAuditPayload('audit-6'))] },
  })

  const res = await app.inject({
    method: 'POST',
    url: '/sync/lookup-op',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { op_ids: [opId, '01HZ0000000000000000000099'] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.deepStrictEqual(body.found, [opId])
})

test('POST /sync/conflicts/:opId/resolve returns 404 for unknown op', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const res = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/01HZ0000000000000000000099/resolve',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { choice: 'local' },
  })
  assert.strictEqual(res.statusCode, 404)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.code, 'NOT_FOUND')
})
