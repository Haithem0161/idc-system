import { test } from 'node:test'
import * as assert from 'node:assert'
import { encode as encodeMsgpack } from '@msgpack/msgpack'

import { build } from '../helper'

const TENANT = 'tenant-phase08'
const SUPERADMIN_ID = '00000000-0000-7000-8000-000000000088'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  conflictsRepo: {
    park: (record: Record<string, unknown>) => Promise<void>
  }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string, headers: Record<string, string> }>
}

function authToken (app: FastifyAppLike, role: 'superadmin' | 'receptionist' | 'accountant'): string {
  return app.jwt.sign({
    sub: SUPERADMIN_ID,
    email: 'admin@example.com',
    entityId: TENANT,
    role,
  })
}

function makeAuditPayload (id: string, opts: Partial<Record<string, unknown>> = {}): string {
  const now = new Date().toISOString()
  const payload = {
    id,
    actor_user_id: SUPERADMIN_ID,
    action: 'create',
    entity: 'visits',
    entity_id: '01HZ-visit-' + id.slice(-4),
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

async function pushAuditRow (
  app: FastifyAppLike,
  token: string,
  payloadB64: string,
  opId: string
): Promise<void> {
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [{ op_id: opId, entity: 'audit_log', entity_id: 'a-' + opId.slice(-4), op: 'upsert', payload_b64: payloadB64 }] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
}

test('GET /audit/query without auth returns 401', async (t) => {
  const app = await build(t)
  const res = await app.inject({
    method: 'GET',
    url: '/audit/query?from=2026-01-01T00:00:00Z&to=2026-12-31T23:59:59Z',
  })
  assert.strictEqual(res.statusCode, 401)
})

test('GET /audit/query rejects non-superadmin', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike, 'receptionist')
  const res = await app.inject({
    method: 'GET',
    url: '/audit/query?from=2026-01-01T00:00:00Z&to=2026-12-31T23:59:59Z',
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 403, res.payload)
  const body = JSON.parse(res.payload)
  assert.match(body.message, /superadmin/i)
})

test('GET /audit/query returns rows filtered by action + entity, sorted at DESC', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = authToken(a, 'superadmin')

  const ids = [
    '01HZ0000000000000000ADT001',
    '01HZ0000000000000000ADT002',
    '01HZ0000000000000000ADT003',
  ]
  const now = new Date()
  for (let i = 0; i < ids.length; i++) {
    const at = new Date(now.getTime() - i * 1000).toISOString()
    await pushAuditRow(
      a,
      token,
      makeAuditPayload(ids[i], { at, created_at: at, updated_at: at, action: i === 0 ? 'lock' : 'create' }),
      ids[i]
    )
  }

  const res = await app.inject({
    method: 'GET',
    url: `/audit/query?from=${new Date(now.getTime() - 60000).toISOString()}&to=${new Date(now.getTime() + 60000).toISOString()}&action=create&entity=visits`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.rows.length, 2)
  assert.ok(body.rows[0].at >= body.rows[1].at, 'expected at DESC')
  assert.strictEqual(body.rows[0].action, 'create')
})

test('GET /audit/query supports cursor pagination', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = authToken(a, 'superadmin')

  const now = new Date()
  for (let i = 0; i < 5; i++) {
    const id = '01HZ0000000000000000PAG00' + i
    const at = new Date(now.getTime() - i * 1000).toISOString()
    await pushAuditRow(
      a,
      token,
      makeAuditPayload(id, { at, created_at: at, updated_at: at }),
      id
    )
  }

  const first = await app.inject({
    method: 'GET',
    url: `/audit/query?from=${new Date(now.getTime() - 60000).toISOString()}&to=${new Date(now.getTime() + 60000).toISOString()}&limit=2`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(first.statusCode, 200)
  const fb = JSON.parse(first.payload)
  assert.strictEqual(fb.rows.length, 2)
  assert.ok(fb.next_cursor != null)

  const second = await app.inject({
    method: 'GET',
    url: `/audit/query?from=${new Date(now.getTime() - 60000).toISOString()}&to=${new Date(now.getTime() + 60000).toISOString()}&limit=2&cursor=${encodeURIComponent(fb.next_cursor)}`,
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(second.statusCode, 200)
  const sb = JSON.parse(second.payload)
  assert.strictEqual(sb.rows.length, 2)
  assert.ok(sb.rows[0].at < fb.rows[1].at, 'next page strictly older')
})

test('GET /sync/conflicts lists open envelopes for the tenant', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = authToken(a, 'superadmin')

  await a.conflictsRepo.park({
    opId: 'op-cf-1',
    entity: 'settings',
    entityId: 'setting-key-1',
    serverPayload: { value: 'srv' },
    localPayload: { value: 'lcl' },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })
  await a.conflictsRepo.park({
    opId: 'op-cf-2',
    entity: 'visits',
    entityId: 'visit-1',
    serverPayload: { id: 'visit-1' },
    localPayload: { id: 'visit-1' },
    reason: 'manual_policy_visit_divergence',
    tenantId: 'other-tenant',
  })

  const res = await app.inject({
    method: 'GET',
    url: '/sync/conflicts',
    headers: { authorization: `Bearer ${token}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.conflicts.length, 1)
  assert.strictEqual(body.conflicts[0].op_id, 'op-cf-1')
  assert.strictEqual(body.conflicts[0].resolved_at, null)
})

test('POST /sync/conflicts/:opId/resolve is idempotent on resolve_op_id', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = authToken(a, 'superadmin')

  await a.conflictsRepo.park({
    opId: 'op-cf-resolve-1',
    entity: 'settings',
    entityId: 'setting-key-1',
    serverPayload: { value: 'srv' },
    localPayload: { value: 'lcl' },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })

  const first = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-cf-resolve-1/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    payload: { choice: 'server', resolve_op_id: 'stable-resolve-hash-001' },
  })
  assert.strictEqual(first.statusCode, 200, first.payload)
  assert.strictEqual(JSON.parse(first.payload).status, 'applied')

  // Same resolve_op_id -> duplicate (no double-apply, no error).
  const retry = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-cf-resolve-1/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    payload: { choice: 'server', resolve_op_id: 'stable-resolve-hash-001' },
  })
  assert.strictEqual(retry.statusCode, 200, retry.payload)
  assert.strictEqual(JSON.parse(retry.payload).status, 'duplicate')
})

test('POST /sync/conflicts/:opId/resolve returns 409 ALREADY_RESOLVED on conflicting retry', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = authToken(a, 'superadmin')

  await a.conflictsRepo.park({
    opId: 'op-cf-already-1',
    entity: 'settings',
    entityId: 'setting-key-1',
    serverPayload: { value: 'srv' },
    localPayload: { value: 'lcl' },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })

  const first = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-cf-already-1/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    payload: { choice: 'server', resolve_op_id: 'first-attempt' },
  })
  assert.strictEqual(first.statusCode, 200, first.payload)

  const conflicting = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-cf-already-1/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    payload: { choice: 'local', resolve_op_id: 'second-attempt-different-choice' },
  })
  assert.strictEqual(conflicting.statusCode, 409, conflicting.payload)
  assert.strictEqual(JSON.parse(conflicting.payload).code, 'ALREADY_RESOLVED')
})

test('GET /healthz exposes the enriched dependency status', async (t) => {
  const app = await build(t)
  const res = await app.inject({ method: 'GET', url: '/healthz' })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.status, 'ok')
  assert.strictEqual(body.db, 'ok')
  assert.strictEqual(body.redis, 'ok')
  assert.strictEqual(body.migrationsApplied, true)
  assert.ok(typeof body.version === 'string')
})

test('GET /metrics is 404 when METRICS_TOKEN unset', async (t) => {
  const prev = process.env.METRICS_TOKEN
  delete process.env.METRICS_TOKEN
  const app = await build(t)
  const res = await app.inject({ method: 'GET', url: '/metrics' })
  assert.strictEqual(res.statusCode, 404)
  if (prev !== undefined) process.env.METRICS_TOKEN = prev
})

test('GET /metrics returns Prometheus payload when token matches', async (t) => {
  process.env.METRICS_TOKEN = 'test-token-phase08'
  const app = await build(t)
  const res = await app.inject({
    method: 'GET',
    url: '/metrics',
    headers: { 'x-internal-token': 'test-token-phase08' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  assert.match(res.payload, /sync_push_duration_seconds_count/)
  assert.match(res.payload, /sync_conflict_total/)
  delete process.env.METRICS_TOKEN
})
