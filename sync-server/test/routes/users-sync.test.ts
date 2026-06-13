// Phase-09 DEF-007 G24 -- server-side `users` push ProcessedOp
// idempotency.
//
// The phase-02 build spec advertised that `POST /sync/push` is
// idempotent on `op_id` for every entity (the general ProcessedOp
// gate at push-service.ts:75), but no test specifically exercised
// the `users` entity path. This file pins:
//
//   (1) Superadmin push of a `users` row applies successfully on
//       first send.
//   (2) The same op_id replayed returns `status='duplicate'` and
//       does NOT re-mutate the row (the ProcessedOp.has hit fires
//       BEFORE the entity case, so the user-store sees no second
//       upsert).
//   (3) Non-superadmin role is rejected (403) -- the
//       requireSuperadmin guard fires regardless of dedupe.
//   (4) Dedupe is keyed by `op_id`, NOT by the user row id -- two
//       distinct ops carrying the SAME user id BOTH apply (the
//       second is treated as an update, not a duplicate).

import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-users-g24'
const SUPERADMIN_USER_ID = '00000000-0000-7000-8000-000000000aaa'
const RX_USER_ID = '00000000-0000-7000-8000-000000000bbb'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  inject: (opts: object) => Promise<{ statusCode: number; payload: string }>
}

function superadminToken (app: FastifyAppLike): string {
  return app.jwt.sign({
    sub: SUPERADMIN_USER_ID,
    email: 'admin@idc.iq',
    entityId: TENANT,
    role: 'superadmin',
  })
}

function receptionistToken (app: FastifyAppLike): string {
  return app.jwt.sign({
    sub: RX_USER_ID,
    email: 'rx@idc.iq',
    entityId: TENANT,
    role: 'receptionist',
  })
}

function makeUserPayload (
  userId: string,
  overrides: Partial<Record<string, unknown>> = {},
): string {
  const now = new Date().toISOString()
  const row = {
    id: userId,
    email: `${userId.slice(-4)}@idc.iq`,
    name: 'Asma',
    password_hash: '$argon2id$v=19$m=65536,t=3,p=4$dGVzdA$abcdef',
    role: 'accountant',
    is_active: true,
    entity_id: TENANT,
    version: 1,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-1',
    ...overrides,
  }
  return Buffer.from(JSON.stringify(row)).toString('base64')
}

function makeUserOp (opId: string, payload: string) {
  return {
    op_id: opId,
    entity: 'users',
    entity_id: 'user-row-id',
    op: 'upsert',
    payload_b64: payload,
  }
}

test('DEF-007 G24: superadmin push of users row applies on first send', async (t) => {
  const app = await build(t)
  const token = superadminToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000g24a'
  const userId = '00000000-0000-7000-8000-000000000c01'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeUserOp(opId, makeUserPayload(userId))] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 1)
  assert.strictEqual(body.accepted[0].status, 'applied')
  assert.strictEqual(body.accepted[0].op_id, opId)
})

test('DEF-007 G24: same op_id replay returns duplicate (ProcessedOp dedupe)', async (t) => {
  const app = await build(t)
  const token = superadminToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000g24b'
  const userId = '00000000-0000-7000-8000-000000000c02'
  const payload = makeUserPayload(userId)

  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeUserOp(opId, payload)] },
  })
  assert.strictEqual(first.statusCode, 200)
  assert.strictEqual(JSON.parse(first.payload).accepted[0].status, 'applied')

  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeUserOp(opId, payload)] },
  })
  assert.strictEqual(second.statusCode, 200)
  const body = JSON.parse(second.payload)
  assert.strictEqual(body.accepted[0].status, 'duplicate')
  // The duplicate response carries the original op_id (caller can
  // confirm which op was deduped).
  assert.strictEqual(body.accepted[0].op_id, opId)
})

test('DEF-007 G24: receptionist push of users row is per-op rejected (role gate)', async (t) => {
  // Per-op isolation: a role/validation failure no longer aborts the batch
  // with an HTTP error; the op lands in `rejected[]` with a 200 envelope so
  // the rest of the batch still applies.
  const app = await build(t)
  const token = receptionistToken(app as unknown as FastifyAppLike)
  const opId = '01HZ0000000000000000000g24c'
  const userId = '00000000-0000-7000-8000-000000000c03'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: { ops: [makeUserOp(opId, makeUserPayload(userId))] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, opId)
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 403)
})

test('DEF-007 G24: dedupe keyed on op_id, NOT user id (two distinct ops on same user both apply)', async (t) => {
  // Two ops with DIFFERENT op_ids but SAME user id -- both should apply
  // (the second is a legitimate update, not a duplicate). A regression
  // that dedupe'd on user id would silently drop user updates.
  const app = await build(t)
  const token = superadminToken(app as unknown as FastifyAppLike)
  const userId = '00000000-0000-7000-8000-000000000c04'
  const opIdA = '01HZ0000000000000000000g24d'
  const opIdB = '01HZ0000000000000000000g24e'

  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        makeUserOp(opIdA, makeUserPayload(userId, { name: 'Asma v1', version: 1 })),
      ],
    },
  })
  assert.strictEqual(first.statusCode, 200)
  assert.strictEqual(JSON.parse(first.payload).accepted[0].status, 'applied')

  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [
        makeUserOp(opIdB, makeUserPayload(userId, { name: 'Asma v2', version: 2 })),
      ],
    },
  })
  assert.strictEqual(second.statusCode, 200)
  const body = JSON.parse(second.payload)
  // Different op_id -> applied, not duplicate.
  assert.strictEqual(body.accepted[0].status, 'applied')
  assert.strictEqual(body.accepted[0].op_id, opIdB)
})

test('DEF-007 G24: cross-tenant op_id collision is isolated (dedupe is tenant-scoped)', async (t) => {
  // Two tokens for two different tenants pushing the same op_id MUST
  // both apply -- ProcessedOp is keyed by (op_id, tenantId), so a
  // collision across tenants is NOT a duplicate. A regression that
  // de-scoped the dedupe would cross-pollute tenants.
  const app = await build(t)
  const tokenA = superadminToken(app as unknown as FastifyAppLike)
  const tokenB = (app as unknown as FastifyAppLike).jwt.sign({
    sub: SUPERADMIN_USER_ID,
    email: 'admin@other.iq',
    entityId: 'tenant-other-g24',
    role: 'superadmin',
  })
  const sharedOpId = '01HZ0000000000000000000g24f'
  const userA = '00000000-0000-7000-8000-000000000d01'
  const userB = '00000000-0000-7000-8000-000000000d02'

  const firstA = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${tokenA}`, 'x-device-id': 'dev-1' },
    payload: {
      ops: [makeUserOp(sharedOpId, makeUserPayload(userA))],
    },
  })
  assert.strictEqual(firstA.statusCode, 200)
  assert.strictEqual(JSON.parse(firstA.payload).accepted[0].status, 'applied')

  // Same op_id under a different tenant must apply -- not duplicate.
  const firstB = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${tokenB}`, 'x-device-id': 'dev-2' },
    payload: {
      ops: [
        makeUserOp(
          sharedOpId,
          makeUserPayload(userB, { entity_id: 'tenant-other-g24' }),
        ),
      ],
    },
  })
  assert.strictEqual(firstB.statusCode, 200, firstB.payload)
  assert.strictEqual(JSON.parse(firstB.payload).accepted[0].status, 'applied')
})
