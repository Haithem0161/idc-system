// Phase-09 BLOCKER-2 / SHIP-CONCERN: JWT plugin MUST refuse to boot in
// production when neither `JWT_PUBLIC_KEY` (RS256) nor a sufficient
// `JWT_SECRET` (32+ char HS256 dev fallback) is configured. The previous
// silent 'dev-only-secret' fallback was a P0 ship-blocker (anyone could mint
// valid tokens against a deployed server) and was removed in phase-09 §3.

// Scrub the env BEFORE any plugin module loads. The plugin reads
// `process.env` at register-time, so we set up scenarios per test by
// clearing/reseting the vars and re-importing.
delete process.env.DATABASE_URL

import { test } from 'node:test'
import * as assert from 'node:assert'
import Fastify, { type FastifyInstance } from 'fastify'
import fp from 'fastify-plugin'

async function loadJwtPlugin (): Promise<(fastify: FastifyInstance) => Promise<void>> {
  // Bust the module cache so each test sees a fresh closure over the
  // mutated env. The plugin reads `process.env.JWT_*` at register-time, not
  // at module-load time, so a single import works — but importing fresh is
  // safer if a future refactor moves the read up to top-level.
  const mod = await import(`../../src/app/plugins/auth-jwt.js?bust=${Date.now()}`)
  return mod.default as (fastify: FastifyInstance) => Promise<void>
}

test('JWT plugin refuses to boot in production without JWT_PUBLIC_KEY', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    delete process.env.JWT_PUBLIC_KEY
    delete process.env.JWT_SECRET

    const fastify = Fastify({ logger: false })
    const plugin = await loadJwtPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'auth-jwt-test-boot-refusal' }))
        await fastify.ready()
      },
      (err: unknown) => {
        const msg = (err as Error).message
        assert.match(msg, /JWT plugin/i)
        assert.match(msg, /JWT_PUBLIC_KEY/i)
        return true
      },
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('JWT plugin refuses to boot in production with only a short JWT_SECRET', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    delete process.env.JWT_PUBLIC_KEY
    process.env.JWT_SECRET = 'too-short'

    const fastify = Fastify({ logger: false })
    const plugin = await loadJwtPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'auth-jwt-test-prod-short-secret' }))
        await fastify.ready()
      },
      /JWT plugin/i,
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('JWT plugin boots in non-production with a 32+ char JWT_SECRET (HS256 dev fallback)', async () => {
  const prev = { ...process.env }
  try {
    delete process.env.NODE_ENV
    delete process.env.JWT_PUBLIC_KEY
    process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'

    const fastify = Fastify({ logger: false })
    const plugin = await loadJwtPlugin()
    await fastify.register(fp(plugin, { name: 'auth-jwt-test-dev-fallback' }))
    await fastify.ready()
    assert.strictEqual(typeof fastify.authenticate, 'function')
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('JWT plugin boots in non-production without JWT_PUBLIC_KEY when JWT_SECRET present', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'development'
    delete process.env.JWT_PUBLIC_KEY
    process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'

    const fastify = Fastify({ logger: false })
    const plugin = await loadJwtPlugin()
    await fastify.register(fp(plugin, { name: 'auth-jwt-test-dev-explicit' }))
    await fastify.ready()
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('No source file references the removed \'dev-only-secret\' fallback (CI grep parity)', async () => {
  const { readFileSync } = await import('node:fs')
  const { glob } = await import('node:fs/promises')
  let matches = 0
  for await (const file of glob('src/**/*.ts')) {
    const text = readFileSync(file, 'utf8')
    if (text.includes('dev-only-secret')) matches += 1
  }
  assert.strictEqual(
    matches,
    0,
    'Found the removed \'dev-only-secret\' fallback in src/. Phase-09 §3 SHIP-1 stipulates this string MUST NOT reappear.',
  )
})
