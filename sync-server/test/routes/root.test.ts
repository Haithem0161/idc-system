import { test } from 'node:test'
import * as assert from 'node:assert'
import { build } from '../helper'

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
  assert.deepStrictEqual(JSON.parse(res.payload), { status: 'ok', version: '0.1.0' })
})
