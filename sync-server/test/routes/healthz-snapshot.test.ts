// Phase-09 BLOCKER-5: pin the /healthz canonical response to the SHA-256
// snapshot in test/expected/healthz/. Any drift in the envelope shape (keys
// added, removed, reordered, or value semantics changed) fails this test
// until the snapshot is regenerated with a documented PR review.

import { test } from 'node:test'
import * as assert from 'node:assert'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { build } from '../helper'

// The tsconfig compiles tests to CJS, so __dirname is available natively.
const expectedDir = join(__dirname, '..', 'expected', 'healthz')

function sha256 (input: string): string {
  return createHash('sha256').update(input).digest('hex')
}

function loadCanonical (name: string): { json: string; hash: string } {
  const json = readFileSync(join(expectedDir, `${name}.json`), 'utf8').replace(/\n$/, '')
  const hash = readFileSync(join(expectedDir, `${name}.json.sha256`), 'utf8').trim()
  return { json, hash }
}

test('GET /healthz response matches healthz-ok-canonical SHA-256 (memory-store path)', async (t) => {
  const app = await build(t)
  const res = await app.inject({ method: 'GET', url: '/healthz' })
  assert.strictEqual(res.statusCode, 200, res.payload)

  // The payload is JSON; we re-stringify with the exact canonical key order
  // to lock the shape, then hash. Tests bootstrap without DATABASE_URL so the
  // memory-store fallback applies: every probe reports ok, migrations true.
  const parsed = JSON.parse(res.payload) as Record<string, unknown>
  const canonicalJson = JSON.stringify({
    status: parsed.status,
    db: parsed.db,
    redis: parsed.redis,
    migrationsApplied: parsed.migrationsApplied,
    version: parsed.version,
  })

  const expected = loadCanonical('healthz-ok-canonical')

  assert.strictEqual(canonicalJson, expected.json, 'response should match canonical JSON byte-for-byte')
  assert.strictEqual(sha256(canonicalJson), expected.hash, 'response should match canonical SHA-256')
})

test('healthz-fail-canonical hash matches the JSON file (snapshot regen invariant)', () => {
  const { json, hash } = loadCanonical('healthz-fail-canonical')
  assert.strictEqual(sha256(json), hash, 'fail snapshot hash must match its JSON')
})

test('healthz-ok-canonical hash matches the JSON file (snapshot regen invariant)', () => {
  const { json, hash } = loadCanonical('healthz-ok-canonical')
  assert.strictEqual(sha256(json), hash, 'ok snapshot hash must match its JSON')
})
