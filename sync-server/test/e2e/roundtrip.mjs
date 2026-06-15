// Phase-10 live two-device round-trip checks against the REAL sync-server
// (real Postgres, RS256, NODE_ENV=production). Driven by tools/run-roundtrip-e2e.sh,
// which stands up the server and exports JWT_PRIVATE_KEY_PATH so this file can
// forge a valid-signature/foreign-subject token for the T5 binding check.
//
// Proves end-to-end through actual server + Prisma code:
//   - Auth: login, GET /auth/profile (T6), refresh subject binding (T5 positive
//     + foreign-subject 403, foreign-subject logout no-op), RS256 (T7 implied).
//   - Schema negotiation (T3): pull returns server_schema_version.
//   - Pull fan-out: device A pushes -> device B pulls.
//   - Manual-policy conflict round trip (T1): divergent push parks, server value
//     is not clobbered, resolve propagates.
//   - Merged re-validation (T4): a malformed merged payload 422s.
//   - Metrics (T9): /metrics push + conflict counts are non-zero.

import { readFileSync } from 'node:fs'
import { createSign } from 'node:crypto'

const BASE = process.env.ROUNDTRIP_BASE ?? 'http://localhost:3161'
const TENANT = 'clinic-1'
let pass = 0, fail = 0
function ok (name, cond, extra = '') {
  if (cond) { pass++; console.log(`  PASS  ${name}`) }
  else { fail++; console.log(`  FAIL  ${name} ${extra}`) }
}

const now = () => new Date().toISOString()
const uuid = () => crypto.randomUUID()
const runTag = uuid().slice(0, 8)
const b64 = (obj) => Buffer.from(JSON.stringify(obj)).toString('base64')
const b64url = (buf) => Buffer.from(buf).toString('base64url')

async function login (deviceId) {
  const r = await fetch(`${BASE}/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-device-id': deviceId },
    body: JSON.stringify({ email: 'admin@idc.local', password: 'hunter22pw', entityId: TENANT }),
  })
  return r.json()
}
async function push (token, deviceId, ops) {
  const r = await fetch(`${BASE}/sync/push`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json', 'x-device-id': deviceId, 'x-app-version': '0.1.3', 'x-schema-version': '11' },
    body: JSON.stringify({ ops }),
  })
  return { status: r.status, body: await r.json().catch(() => null) }
}
async function pull (token, deviceId) {
  const r = await fetch(`${BASE}/sync/pull`, {
    headers: { authorization: `Bearer ${token}`, 'x-device-id': deviceId, 'x-app-version': '0.1.3', 'x-schema-version': '11' },
  })
  return { status: r.status, body: await r.json().catch(() => null) }
}

// Mint a valid-signature RS256 token with an arbitrary subject (for T5).
function forgeToken (sub) {
  const keyPath = process.env.JWT_PRIVATE_KEY_PATH
  if (!keyPath) return null
  const header = b64url(JSON.stringify({ alg: 'RS256', typ: 'JWT' }))
  const iat = Math.floor(Date.now() / 1000)
  const payload = b64url(JSON.stringify({ sub, email: 'x@y.z', entityId: TENANT, role: 'superadmin', iat, exp: iat + 900 }))
  const signer = createSign('RSA-SHA256')
  signer.update(`${header}.${payload}`)
  const sig = signer.sign(readFileSync(keyPath)).toString('base64url')
  return `${header}.${payload}.${sig}`
}

console.log('\n=== Phase-10 live round-trip ===\n')

console.log('[auth]')
const a = await login('device-A')
const b = await login('device-B')
ok('device A login returns an RS256 access token', !!a.accessToken && JSON.parse(Buffer.from(a.accessToken.split('.')[0], 'base64url')).alg === 'RS256')
ok('device B login returns tokens', !!b.accessToken && !!b.refreshToken)

const prof = await fetch(`${BASE}/auth/profile`, { headers: { authorization: `Bearer ${a.accessToken}` } })
const profBody = await prof.json()
ok('T6: GET /auth/profile returns identity', prof.status === 200 && profBody.email === 'admin@idc.local' && profBody.entityId === TENANT)
ok('T6: profile omits passwordHash', !('passwordHash' in profBody))
ok('T6: GET /auth/profile 401 without bearer', (await fetch(`${BASE}/auth/profile`)).status === 401)

// T5 positive: matching subject refresh succeeds.
const refr = await fetch(`${BASE}/auth/refresh`, {
  method: 'POST',
  headers: { authorization: `Bearer ${a.accessToken}`, 'content-type': 'application/json', 'x-device-id': 'device-A' },
  body: JSON.stringify({ refreshToken: a.refreshToken }),
})
ok('T5: refresh with the matching subject bearer succeeds', refr.status === 200)
const rb = await refr.json(); a.refreshToken = rb.refreshToken; a.accessToken = rb.accessToken

// T5 negative: valid-signature, foreign-subject bearer -> 403 on refresh; logout no-op.
const forged = forgeToken('00000000-0000-0000-0000-000000000bad')
if (forged) {
  const r403 = await fetch(`${BASE}/auth/refresh`, {
    method: 'POST',
    headers: { authorization: `Bearer ${forged}`, 'content-type': 'application/json', 'x-device-id': 'device-A' },
    body: JSON.stringify({ refreshToken: a.refreshToken }),
  })
  ok('T5: refresh with a foreign-subject bearer is rejected 403', r403.status === 403, `got ${r403.status}`)
  const lo = await fetch(`${BASE}/auth/logout`, {
    method: 'POST',
    headers: { authorization: `Bearer ${forged}`, 'content-type': 'application/json' },
    body: JSON.stringify({ refreshToken: a.refreshToken }),
  })
  ok('T5: logout with a foreign-subject bearer returns 204 (no-op)', lo.status === 204)
  const survive = await fetch(`${BASE}/auth/refresh`, {
    method: 'POST',
    headers: { authorization: `Bearer ${a.accessToken}`, 'content-type': 'application/json', 'x-device-id': 'device-A' },
    body: JSON.stringify({ refreshToken: a.refreshToken }),
  })
  ok('T5: the token SURVIVES a foreign-subject logout (not revoked)', survive.status === 200)
  const sb = await survive.json(); a.refreshToken = sb.refreshToken; a.accessToken = sb.accessToken
} else {
  console.log('  SKIP  T5 foreign-subject checks (JWT_PRIVATE_KEY_PATH not set)')
}

console.log('\n[pull fan-out]')
const patientId = uuid()
const pushA = await push(a.accessToken, 'device-A', [{
  op_id: uuid(), entity: 'patients', entity_id: patientId, op: 'upsert',
  payload_b64: b64({ id: patientId, name: 'Round Trip Patient', created_at: now(), updated_at: now(), deleted_at: null, version: 1, origin_device_id: 'device-A', entity_id: TENANT }),
}])
ok('device A pushes a patients row (applied)', pushA.body?.accepted?.[0]?.status === 'applied', JSON.stringify(pushA.body))
const pullB = await pull(b.accessToken, 'device-B')
ok('T3: pull response carries server_schema_version', typeof pullB.body?.server_schema_version === 'number', `got ${pullB.body?.server_schema_version}`)
ok('pull fan-out: device B pulls the row device A pushed', (pullB.body?.changes ?? []).some((c) => c.entity === 'patients' && c.entity_id === patientId))

console.log('\n[manual conflict round trip: settings]')
const settingId = uuid()
const base = { id: settingId, key: `currency_symbol_${runTag}`, value: 'IQD', value_type: 'text', entity_id: TENANT, version: 1, created_at: now(), updated_at: now(), deleted_at: null, origin_device_id: 'device-A' }
await push(a.accessToken, 'device-A', [{ op_id: uuid(), entity: 'settings', entity_id: settingId, op: 'upsert', payload_b64: b64(base) }])
const v2 = await push(a.accessToken, 'device-A', [{ op_id: uuid(), entity: 'settings', entity_id: settingId, op: 'upsert', payload_b64: b64({ ...base, value: 'USD', version: 2, updated_at: now() }) }])
ok('settings advances to USD@v2 on the server', v2.body?.accepted?.[0]?.status === 'applied')
const conflictOpId = uuid()
const divergent = await push(b.accessToken, 'device-B', [{ op_id: conflictOpId, entity: 'settings', entity_id: settingId, op: 'upsert', payload_b64: b64({ ...base, value: 'EUR', version: 1, updated_at: now(), origin_device_id: 'device-B' }) }])
ok('T1: device B divergent push is PARKED (not silently overwritten)', (divergent.body?.conflicts ?? []).some((c) => c.entity_id === settingId), JSON.stringify(divergent.body))
const check = await pull(a.accessToken, 'device-A')
ok('T1: the server value stayed USD (no clobber)', check.body?.changes?.find((c) => c.entity === 'settings' && c.entity_id === settingId)?.payload?.value === 'USD')
const resolve = await fetch(`${BASE}/sync/conflicts/${conflictOpId}/resolve`, {
  method: 'POST',
  headers: { authorization: `Bearer ${b.accessToken}`, 'content-type': 'application/json', 'x-device-id': 'device-B' },
  body: JSON.stringify({ choice: 'merged', resolve_op_id: uuid(), merged: { ...base, value: 'GBP', version: 2, updated_at: now() } }),
})
ok('conflict resolve (merged, valid) succeeds', resolve.status === 200)
const after = await pull(a.accessToken, 'device-A')
ok('conflict resolution propagates the merged value (GBP)', after.body?.changes?.find((c) => c.entity === 'settings' && c.entity_id === settingId)?.payload?.value === 'GBP')

console.log('\n[merged re-validation]')
const msId = uuid()
const ms = { id: msId, key: `thermal_width_${runTag}`, value: '58', value_type: 'int', entity_id: TENANT, version: 1, created_at: now(), updated_at: now(), deleted_at: null }
await push(a.accessToken, 'device-A', [{ op_id: uuid(), entity: 'settings', entity_id: msId, op: 'upsert', payload_b64: b64(ms) }])
await push(a.accessToken, 'device-A', [{ op_id: uuid(), entity: 'settings', entity_id: msId, op: 'upsert', payload_b64: b64({ ...ms, value: '80', version: 2, updated_at: now() }) }])
const mOp = uuid()
await push(b.accessToken, 'device-B', [{ op_id: mOp, entity: 'settings', entity_id: msId, op: 'upsert', payload_b64: b64({ ...ms, value: '57', version: 1, updated_at: now(), origin_device_id: 'device-B' }) }])
const mResolve = await fetch(`${BASE}/sync/conflicts/${mOp}/resolve`, {
  method: 'POST',
  headers: { authorization: `Bearer ${b.accessToken}`, 'content-type': 'application/json', 'x-device-id': 'device-B' },
  body: JSON.stringify({ choice: 'merged', resolve_op_id: uuid(), merged: { id: msId, value: 'X', entity_id: TENANT, version: 3 } }), // no `key`
})
ok('T4: malformed merged payload (missing key) rejected 422', mResolve.status === 422, `got ${mResolve.status}`)

console.log('\n[metrics]')
const metricsText = await (await fetch(`${BASE}/metrics`, { headers: { 'x-internal-token': 'metrics-secret' } })).text()
ok('T9: /metrics push count is non-zero', Number(/sync_push_duration_seconds_count (\d+)/.exec(metricsText)?.[1] ?? '0') > 0)
ok('T9: /metrics conflict_total reflects parked conflicts', Number(/sync_conflict_total (\d+)/.exec(metricsText)?.[1] ?? '0') >= 2)

console.log(`\n=== RESULT: ${pass} passed, ${fail} failed ===\n`)
process.exit(fail === 0 ? 0 : 1)
