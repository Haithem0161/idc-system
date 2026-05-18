// Phase-09 BLOCKER-6: every successful manual conflict resolve MUST emit a
// `conflict_resolve` audit_log row attributed to the resolver, in the same
// transaction as the resolve commit. The new client always supplies a
// `resolve_op_id` so retries dedupe through the ProcessedOp cache.

import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-blocker6'
const SUPERADMIN_ID = '00000000-0000-7000-8000-000000000099'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  conflictsRepo: {
    park: (record: Record<string, unknown>) => Promise<void>
  }
  auditQueryRepo: {
    queryAudit: (filter: {
      tenantId: string
      from: string
      to: string
      action?: string
      entity?: string
      limit: number
    }) => Promise<{ rows: Array<Record<string, unknown>>; nextCursor: string | null }>
  }
  inject: (opts: object) => Promise<{ statusCode: number; payload: string }>
}

function tokenFor (app: FastifyAppLike, role = 'superadmin'): string {
  return app.jwt.sign({
    sub: SUPERADMIN_ID,
    email: 'mariam@example.com',
    entityId: TENANT,
    role,
  })
}

test('POST /sync/conflicts/:opId/resolve emits a conflict_resolve audit_log row', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = tokenFor(a)

  await a.conflictsRepo.park({
    opId: 'op-blk6-001',
    entity: 'settings',
    entityId: 'setting-key-x',
    serverPayload: { value: 'srv' },
    localPayload: { value: 'lcl' },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })

  const before = new Date(Date.now() - 60_000).toISOString()

  const res = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-blk6-001/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-blk6' },
    payload: { choice: 'server', resolve_op_id: 'stable-resolve-hash-blk6' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  assert.strictEqual(JSON.parse(res.payload).status, 'applied')

  const after = new Date(Date.now() + 60_000).toISOString()

  // Server-canonical: the audit row lives only on the server until the next
  // /sync/pull brings it down to the resolver's device. We query directly.
  const audit = await a.auditQueryRepo.queryAudit({
    tenantId: TENANT,
    from: before,
    to: after,
    action: 'conflict_resolve',
    entity: 'settings',
    limit: 100,
  })

  assert.strictEqual(audit.rows.length, 1, 'expected exactly one conflict_resolve audit row')
  const row = audit.rows[0]
  assert.strictEqual(row.action, 'conflict_resolve')
  assert.strictEqual(row.entity, 'settings')
  assert.strictEqual(row.entity_id, 'setting-key-x')
  assert.strictEqual(row.actor_user_id, SUPERADMIN_ID)
  const delta = row.delta as { choice: string; opId: string; resolveOpId: string | null }
  assert.strictEqual(delta.choice, 'server')
  assert.strictEqual(delta.opId, 'op-blk6-001')
  assert.strictEqual(delta.resolveOpId, 'stable-resolve-hash-blk6')
})

test('Idempotent retry on resolve_op_id does not emit a second audit row', async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike
  const token = tokenFor(a)

  await a.conflictsRepo.park({
    opId: 'op-blk6-002',
    entity: 'visits',
    entityId: 'visit-blk6',
    serverPayload: { id: 'visit-blk6' },
    localPayload: { id: 'visit-blk6' },
    reason: 'manual_policy_visit_divergence',
    tenantId: TENANT,
  })

  const before = new Date(Date.now() - 60_000).toISOString()
  const body = { choice: 'local', resolve_op_id: 'stable-resolve-blk6-002' }

  const first = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-blk6-002/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-A' },
    payload: body,
  })
  assert.strictEqual(first.statusCode, 200)
  assert.strictEqual(JSON.parse(first.payload).status, 'applied')

  // Same resolve_op_id -> duplicate, no double-apply, no second audit row.
  const retry = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-blk6-002/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-A' },
    payload: body,
  })
  assert.strictEqual(retry.statusCode, 200)
  assert.strictEqual(JSON.parse(retry.payload).status, 'duplicate')

  const after = new Date(Date.now() + 60_000).toISOString()
  const audit = await a.auditQueryRepo.queryAudit({
    tenantId: TENANT,
    from: before,
    to: after,
    action: 'conflict_resolve',
    entity: 'visits',
    limit: 100,
  })
  assert.strictEqual(audit.rows.length, 1, 'idempotent retry must NOT emit a second audit row')
})
