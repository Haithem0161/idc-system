// Phase-09 §2.3 -- env-schema-phase09. Validates the `@fastify/env` plugin
// at `src/app/plugins/env.ts`:
//
// - Production with empty `DATABASE_URL` -> boot refuses (throw mentions
//   `DATABASE_URL`).
// - Production with empty `JWT_PUBLIC_KEY` -> boot refuses (throw mentions
//   `JWT_PUBLIC_KEY`).
// - Production with both required vars set -> boots; `appEnv.isProduction`
//   is true; `appEnv.databaseUrl` carries the configured value.
// - Non-production with empty `DATABASE_URL` -> boots with a warn-level log
//   (does NOT throw); `appEnv.databaseUrl` is null so consumers know to
//   fall back to MemorySyncStore.
// - Non-production with `DATABASE_URL` set -> boots; `appEnv.databaseUrl`
//   carries the configured value.
//
// Each test mutates `process.env`, imports the plugin afresh with a
// cache-busting query string (the env plugin reads `process.env` at
// register-time), then restores the prior env in a `finally` so concurrent
// tests under `tsx --test` see a clean slate.

// Scrub the env BEFORE any plugin module loads. Mirrors auth-jwt-boot.test.ts.
delete process.env.DATABASE_URL
delete process.env.JWT_PUBLIC_KEY

import { test } from 'node:test'
import * as assert from 'node:assert'
import Fastify, { type FastifyInstance } from 'fastify'
import fp from 'fastify-plugin'

async function loadEnvPlugin (): Promise<(fastify: FastifyInstance) => Promise<void>> {
  const mod = await import(`../../src/app/plugins/env.js?bust=${Date.now()}`)
  return mod.default as (fastify: FastifyInstance) => Promise<void>
}

test('env plugin refuses to boot in production without DATABASE_URL', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    delete process.env.DATABASE_URL
    process.env.JWT_PUBLIC_KEY = '---test-rs256-public-key---'

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'env-test-prod-no-db' }))
        await fastify.ready()
      },
      (err: unknown) => {
        const msg = (err as Error).message
        assert.match(msg, /env plugin/i)
        assert.match(msg, /DATABASE_URL/)
        return true
      },
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin refuses to boot in production without JWT_PUBLIC_KEY', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    process.env.DATABASE_URL = 'postgresql://test:test@localhost:5432/test'
    delete process.env.JWT_PUBLIC_KEY

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'env-test-prod-no-jwt' }))
        await fastify.ready()
      },
      (err: unknown) => {
        const msg = (err as Error).message
        assert.match(msg, /env plugin/i)
        assert.match(msg, /JWT_PUBLIC_KEY/)
        return true
      },
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin lists all missing required vars in production when both unset', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    delete process.env.DATABASE_URL
    delete process.env.JWT_PUBLIC_KEY

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'env-test-prod-no-both' }))
        await fastify.ready()
      },
      (err: unknown) => {
        const msg = (err as Error).message
        // The plugin enumerates every missing var so deployers don't have
        // to redeploy twice to discover both. Pin that contract.
        assert.match(msg, /DATABASE_URL/, 'message should list DATABASE_URL')
        assert.match(msg, /JWT_PUBLIC_KEY/, 'message should list JWT_PUBLIC_KEY')
        return true
      },
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin boots in production when both DATABASE_URL and JWT_PUBLIC_KEY are set', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    process.env.DATABASE_URL = 'postgresql://prod:prod@db.example:5432/idc'
    process.env.JWT_PUBLIC_KEY = '---test-rs256-public-key---'

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await fastify.register(fp(plugin, { name: 'env-test-prod-ok' }))
    await fastify.ready()

    const env = fastify.appEnv
    assert.equal(env.nodeEnv, 'production')
    assert.equal(env.isProduction, true)
    assert.equal(env.databaseUrl, 'postgresql://prod:prod@db.example:5432/idc')
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin boots in development with empty DATABASE_URL and exposes null databaseUrl', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'development'
    delete process.env.DATABASE_URL
    delete process.env.JWT_PUBLIC_KEY

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await fastify.register(fp(plugin, { name: 'env-test-dev-no-db' }))
    await fastify.ready()

    const env = fastify.appEnv
    assert.equal(env.nodeEnv, 'development')
    assert.equal(env.isProduction, false)
    assert.equal(
      env.databaseUrl,
      null,
      'empty DATABASE_URL exposed as null so callers know to use MemorySyncStore',
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin boots in development with DATABASE_URL set and exposes it via appEnv', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'development'
    process.env.DATABASE_URL = 'postgresql://dev:dev@localhost:5432/idc-dev'
    delete process.env.JWT_PUBLIC_KEY

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await fastify.register(fp(plugin, { name: 'env-test-dev-with-db' }))
    await fastify.ready()

    const env = fastify.appEnv
    assert.equal(env.nodeEnv, 'development')
    assert.equal(env.isProduction, false)
    assert.equal(env.databaseUrl, 'postgresql://dev:dev@localhost:5432/idc-dev')
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin treats whitespace-only DATABASE_URL as missing in production', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    process.env.DATABASE_URL = '   '
    process.env.JWT_PUBLIC_KEY = '---test-rs256-public-key---'

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'env-test-prod-whitespace-db' }))
        await fastify.ready()
      },
      (err: unknown) => {
        const msg = (err as Error).message
        assert.match(msg, /DATABASE_URL/, 'whitespace-only DATABASE_URL must be rejected')
        return true
      },
    )
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('env plugin defaults NODE_ENV to development when unset', async () => {
  const prev = { ...process.env }
  try {
    delete process.env.NODE_ENV
    delete process.env.DATABASE_URL
    delete process.env.JWT_PUBLIC_KEY

    const fastify = Fastify({ logger: false })
    const plugin = await loadEnvPlugin()
    await fastify.register(fp(plugin, { name: 'env-test-no-node-env' }))
    await fastify.ready()

    const env = fastify.appEnv
    assert.equal(env.nodeEnv, 'development', 'NODE_ENV defaults to development per env.ts schema')
    assert.equal(env.isProduction, false)
    await fastify.close()
  } finally {
    process.env = prev
  }
})
