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

  // A valid DRAFT visit: phase-10 T4 re-validates the resolved payload, so the
  // local payload must satisfy validateVisit (required fields + valid status).
  // This test asserts audit-row idempotency, not validation, so we use a
  // minimal valid draft.
  const validDraftVisit = {
    id: 'visit-blk6',
    patient_id: 'p-blk6',
    receptionist_user_id: 'u-blk6',
    check_type_id: 'c-blk6',
    status: 'draft',
    dye: false,
    entity_id: TENANT,
    version: 3,
  }
  await a.conflictsRepo.park({
    opId: 'op-blk6-002',
    entity: 'visits',
    entityId: 'visit-blk6',
    serverPayload: { ...validDraftVisit, version: 5 },
    localPayload: validDraftVisit,
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

// C4/C7: resolving with choice='local' MUST apply the parked local payload to
// the entity store (previously it only stamped the audit row and silently
// discarded the user's choice, keeping the server's losing version forever).
test("resolve choice='local' applies the local payload to the entity store at a bumped version", async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike & {
    entityStore: { settings: Map<string, { id: string; value: string; version: number }> }
  }
  const token = tokenFor(a)

  const settingId = 'setting-c4-001'
  await a.conflictsRepo.park({
    opId: 'op-c4-001',
    entity: 'settings',
    entityId: settingId,
    // Server is on version 5 with value 'srv'; the client wants 'lcl' at v3.
    serverPayload: { id: settingId, key: 'currency_symbol', value: 'srv', value_type: 'text', entity_id: TENANT, version: 5 },
    localPayload: { id: settingId, key: 'currency_symbol', value: 'lcl', value_type: 'text', entity_id: TENANT, version: 3 },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })

  const res = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-c4-001/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-c4' },
    payload: { choice: 'local', resolve_op_id: 'stable-resolve-c4-001' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  assert.strictEqual(JSON.parse(res.payload).status, 'applied')

  // The store now holds the LOCAL value, at a version above the server's (5)
  // so it wins LWW and propagates to other devices on their next pull.
  const stored = a.entityStore.settings.get(settingId)
  assert.ok(stored, 'the resolved setting must exist in the store')
  assert.strictEqual(stored.value, 'lcl', "choice='local' must write the local value")
  assert.ok(stored.version > 5, `resolved version must exceed server's 5, got ${stored.version}`)
})

test("resolve choice='server' leaves the store unchanged (server already canonical)", async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike & {
    entityStore: { settings: Map<string, { value: string }> }
  }
  const token = tokenFor(a)

  const settingId = 'setting-c4-002'
  await a.conflictsRepo.park({
    opId: 'op-c4-002',
    entity: 'settings',
    entityId: settingId,
    serverPayload: { id: settingId, key: 'currency_symbol', value: 'srv', value_type: 'text', entity_id: TENANT, version: 5 },
    localPayload: { id: settingId, key: 'currency_symbol', value: 'lcl', value_type: 'text', entity_id: TENANT, version: 3 },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })

  const res = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-c4-002/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-c4' },
    payload: { choice: 'server', resolve_op_id: 'stable-resolve-c4-002' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  // choice='server' applies nothing -- the store never received this setting.
  assert.strictEqual(a.entityStore.settings.get(settingId), undefined)
})

// --- Phase-10 T4: merged payloads are re-validated before they hit the store -

test("resolve choice='merged' rejects a malformed visit payload with 422 and writes nothing", async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike & {
    entityStore: { visits: Map<string, unknown> }
  }
  const token = tokenFor(a)

  const visitId = 'visit-t4-001'
  await a.conflictsRepo.park({
    opId: 'op-t4-visit',
    entity: 'visits',
    entityId: visitId,
    serverPayload: { id: visitId, version: 5 },
    localPayload: { id: visitId, version: 3 },
    reason: 'manual_policy_visit_divergence',
    tenantId: TENANT,
  })

  // A locked visit MUST carry the financial snapshot fields; this merged
  // payload omits them, so the shared validateVisit must reject it (422)
  // instead of persisting a corrupt financial record.
  const res = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-t4-visit/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-t4' },
    payload: {
      choice: 'merged',
      resolve_op_id: 'stable-resolve-t4-visit',
      merged: {
        id: visitId,
        patient_id: 'p1',
        receptionist_user_id: 'u1',
        check_type_id: 'c1',
        status: 'locked',
        entity_id: TENANT,
        version: 4,
      },
    },
  })
  assert.strictEqual(res.statusCode, 422, res.payload)
  assert.strictEqual(JSON.parse(res.payload).code, 'VALIDATION_ERROR')
  assert.strictEqual(
    a.entityStore.visits.get(visitId),
    undefined,
    'a rejected merge must not write the visit to the store'
  )
})

test("resolve choice='merged' applies a valid settings payload", async (t) => {
  const app = await build(t)
  const a = app as unknown as FastifyAppLike & {
    entityStore: { settings: Map<string, { value: string; version: number }> }
  }
  const token = tokenFor(a)

  const settingId = 'setting-t4-merged'
  await a.conflictsRepo.park({
    opId: 'op-t4-setting',
    entity: 'settings',
    entityId: settingId,
    serverPayload: { id: settingId, key: 'currency_symbol', value: 'srv', value_type: 'text', entity_id: TENANT, version: 5 },
    localPayload: { id: settingId, key: 'currency_symbol', value: 'lcl', value_type: 'text', entity_id: TENANT, version: 3 },
    reason: 'manual_policy_version_divergence',
    tenantId: TENANT,
  })

  const res = await app.inject({
    method: 'POST',
    url: '/sync/conflicts/op-t4-setting/resolve',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': 'dev-t4' },
    payload: {
      choice: 'merged',
      resolve_op_id: 'stable-resolve-t4-setting',
      merged: { id: settingId, key: 'currency_symbol', value: 'merged', value_type: 'text', entity_id: TENANT, version: 4 },
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const stored = a.entityStore.settings.get(settingId)
  assert.ok(stored, 'a valid merged setting must be written')
  assert.strictEqual(stored.value, 'merged')
  assert.ok(stored.version > 5, 'merged setting wins LWW above the server version')
})
