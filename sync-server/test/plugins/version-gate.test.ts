import { test } from 'node:test'
import * as assert from 'node:assert'
import Fastify, { type FastifyInstance } from 'fastify'
import fp from 'fastify-plugin'

import { compareVersions } from '../../src/app/plugins/version-gate.js'

test('compareVersions: equal versions', () => {
  assert.strictEqual(compareVersions('1.2.3', '1.2.3'), 0)
  assert.strictEqual(compareVersions('1.0', '1.0.0'), 0)
})

test('compareVersions: lower client', () => {
  assert.strictEqual(compareVersions('1.2.0', '1.2.1'), -1)
  assert.strictEqual(compareVersions('0.9.9', '1.0.0'), -1)
  assert.strictEqual(compareVersions('1.2', '1.10'), -1)
})

test('compareVersions: higher client', () => {
  assert.strictEqual(compareVersions('1.3.0', '1.2.9'), 1)
  assert.strictEqual(compareVersions('2.0.0', '1.9.9'), 1)
  assert.strictEqual(compareVersions('1.10', '1.2'), 1)
})

test('compareVersions: missing segments treated as 0', () => {
  assert.strictEqual(compareVersions('1', '1.0.0'), 0)
  assert.strictEqual(compareVersions('1.0.1', '1'), 1)
})

// --- Phase-10 T3: schema-version gate (integer migration count) -------------
//
// Builds env + version-gate over a dummy `/sync/probe` route and drives the
// X-Schema-Version header through the gate.

async function buildGated (env: Record<string, string>): Promise<FastifyInstance> {
  const prev = { ...process.env }
  // Apply the per-test env, then restore after ready() so the gate's
  // register-time read sees exactly these values.
  Object.assign(process.env, env)
  try {
    const fastify = Fastify({ logger: false })
    const envMod = await import(`../../src/app/plugins/env.js?bust=${Date.now()}`)
    await fastify.register(fp(envMod.default, { name: 'env-for-gate-test' }))
    const gateMod = await import(`../../src/app/plugins/version-gate.js?bust=${Date.now()}`)
    await fastify.register(fp(gateMod.default, { name: 'gate-test' }))
    fastify.get('/sync/probe', async () => ({ ok: true }))
    await fastify.ready()
    return fastify
  } finally {
    process.env = prev
  }
}

test('schema gate: rejects 426 when X-Schema-Version is below the minimum', async () => {
  const app = await buildGated({ MIN_CLIENT_SCHEMA_VERSION: '11' })
  const res = await app.inject({
    method: 'GET',
    url: '/sync/probe',
    headers: { 'x-schema-version': '9' },
  })
  assert.strictEqual(res.statusCode, 426)
  const body = JSON.parse(res.body) as { code: string, reason: string, minSchemaVersion: number }
  assert.strictEqual(body.code, 'UPGRADE_REQUIRED')
  assert.strictEqual(body.reason, 'schema_version')
  assert.strictEqual(body.minSchemaVersion, 11)
  await app.close()
})

test('schema gate: allows a client at or above the minimum', async () => {
  const app = await buildGated({ MIN_CLIENT_SCHEMA_VERSION: '11' })
  const atMin = await app.inject({
    method: 'GET',
    url: '/sync/probe',
    headers: { 'x-schema-version': '11' },
  })
  assert.strictEqual(atMin.statusCode, 200)
  const above = await app.inject({
    method: 'GET',
    url: '/sync/probe',
    headers: { 'x-schema-version': '12' },
  })
  assert.strictEqual(above.statusCode, 200)
  await app.close()
})

test('schema gate: fails open when the header is missing or garbage', async () => {
  const app = await buildGated({ MIN_CLIENT_SCHEMA_VERSION: '11' })
  const missing = await app.inject({ method: 'GET', url: '/sync/probe' })
  assert.strictEqual(missing.statusCode, 200, 'missing header must fail open')
  const garbage = await app.inject({
    method: 'GET',
    url: '/sync/probe',
    headers: { 'x-schema-version': 'not-a-number' },
  })
  assert.strictEqual(garbage.statusCode, 200, 'garbage header must fail open')
  await app.close()
})

test('schema gate: no gate configured lets every client through', async () => {
  const app = await buildGated({ MIN_CLIENT_SCHEMA_VERSION: '' })
  const res = await app.inject({
    method: 'GET',
    url: '/sync/probe',
    headers: { 'x-schema-version': '1' },
  })
  assert.strictEqual(res.statusCode, 200)
  await app.close()
})
