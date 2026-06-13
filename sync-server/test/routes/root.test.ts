import { test } from 'node:test'
import * as assert from 'node:assert'
import { build } from '../helper'
import { SERVER_VERSION } from '../../src/app/common/version.js'

test('GET / returns root marker', async (t) => {
  const app = await build(t)
  const res = await app.inject({ url: '/' })
  assert.strictEqual(res.statusCode, 200)
  assert.deepStrictEqual(JSON.parse(res.payload), { root: true })
})

test('GET /healthz returns ok', async (t) => {
  const app = await build(t)
  const res = await app.inject({ url: '/healthz' })
  assert.strictEqual(res.statusCode, 200)
  // Phase-08 §7.17 enriches /healthz with db/redis/migrationsApplied.
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.status, 'ok')
  // Version is read from package.json (no longer hardcoded), so assert against
  // the same source of truth rather than a stale literal.
  assert.strictEqual(body.version, SERVER_VERSION)
  assert.strictEqual(body.db, 'ok')
  assert.strictEqual(body.redis, 'ok')
  assert.strictEqual(body.migrationsApplied, true)
})
