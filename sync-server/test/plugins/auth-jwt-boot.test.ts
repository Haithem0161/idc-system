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

test('T7: JWT plugin refuses to boot in a non-dev env (staging) with only JWT_SECRET', async () => {
  // The HS256 fallback must be permitted ONLY for NODE_ENV development/test.
  // A `staging` (or any non-dev) deploy with just JWT_SECRET would previously
  // silently sign HS256 while clients pin the RS256 public key -- a forge trap.
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'staging'
    delete process.env.JWT_PUBLIC_KEY
    delete process.env.JWT_PRIVATE_KEY
    process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'

    const fastify = Fastify({ logger: false })
    const plugin = await loadJwtPlugin()
    await assert.rejects(
      async () => {
        await fastify.register(fp(plugin, { name: 'auth-jwt-test-staging-hs256-refusal' }))
        await fastify.ready()
      },
      (err: unknown) => {
        const msg = (err as Error).message
        assert.match(msg, /non-dev environment requires JWT_PUBLIC_KEY/i)
        assert.match(msg, /staging/i)
        return true
      },
    )
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

// =============================================================
// DEF-007 G20: @fastify/jwt registered with RS256 keypair
// =============================================================

test('DEF-007 G20: JWT plugin signs AND verifies with RS256 when both keys are set', async () => {
  // The production server signs its OWN tokens (auth-services.ts -> fastify
  // .jwt.sign). So the plugin MUST register the private key, not just the
  // public one. This test mints a token through the PLUGIN's own
  // `fastify.jwt.sign` (the real login path) -- if the plugin were
  // verify-only, sign() would throw, which is exactly the bug this guards.
  const { generateKeyPairSync, createPrivateKey } = await import('node:crypto')
  const { publicKey, privateKey } = generateKeyPairSync('rsa', {
    modulusLength: 2048,
    publicKeyEncoding: { type: 'spki', format: 'pem' },
    privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
  })

  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    process.env.JWT_PUBLIC_KEY = publicKey
    process.env.JWT_PRIVATE_KEY = privateKey
    delete process.env.JWT_SECRET

    const fastify = Fastify({ logger: false }) as FastifyInstance & {
      jwt: { sign: (p: object) => string, verify: (t: string) => unknown }
    }
    const plugin = await loadJwtPlugin()
    await fastify.register(fp(plugin, { name: 'auth-jwt-test-rs256' }))
    await fastify.ready()
    assert.strictEqual(typeof fastify.authenticate, 'function')

    // Sign through the plugin itself -- this is the production token-issuance
    // path. It throws if no private key is registered.
    const token = fastify.jwt.sign({ sub: 'test', email: 'a@b.c', entityId: 'e1' })
    const header = JSON.parse(
      Buffer.from(token.split('.')[0], 'base64url').toString('utf8')
    ) as { alg: string }
    assert.strictEqual(header.alg, 'RS256', 'token header MUST be RS256')

    // And the plugin verifies its own token with the public half.
    const claims = fastify.jwt.verify(token) as { sub: string }
    assert.strictEqual(claims.sub, 'test')

    // Defense against a future refactor that silently downgrades modulus.
    assert.strictEqual(createPrivateKey(privateKey).asymmetricKeyType, 'rsa')

    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('DEF-007 G20b: production with only JWT_PUBLIC_KEY is VERIFY-ONLY (sign throws)', async () => {
  // Without the private key the plugin still boots (a read-only replica is a
  // valid shape) but token issuance must fail loudly rather than silently
  // mint unsigned/HS256 tokens.
  const { generateKeyPairSync } = await import('node:crypto')
  const { publicKey } = generateKeyPairSync('rsa', {
    modulusLength: 2048,
    publicKeyEncoding: { type: 'spki', format: 'pem' },
    privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
  })

  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'production'
    process.env.JWT_PUBLIC_KEY = publicKey
    delete process.env.JWT_PRIVATE_KEY
    delete process.env.JWT_SECRET

    const fastify = Fastify({ logger: false }) as FastifyInstance & {
      jwt: { sign: (p: object) => string }
    }
    const plugin = await loadJwtPlugin()
    await fastify.register(fp(plugin, { name: 'auth-jwt-test-verifyonly' }))
    await fastify.ready()

    assert.throws(
      () => fastify.jwt.sign({ sub: 'test', email: 'a@b.c', entityId: 'e1' }),
      'verify-only registration MUST NOT be able to sign tokens'
    )

    await fastify.close()
  } finally {
    process.env = prev
  }
})

// =============================================================
// DEF-007 G08 server-side companion: GET /auth/public-key
// =============================================================

test('GET /auth/public-key returns the PEM body when JWT_PUBLIC_KEY is set', async () => {
  const { generateKeyPairSync } = await import('node:crypto')
  const { publicKey } = generateKeyPairSync('rsa', {
    modulusLength: 2048,
    publicKeyEncoding: { type: 'spki', format: 'pem' },
    privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
  })

  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'development'
    process.env.JWT_PUBLIC_KEY = publicKey
    process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'

    const fastify = Fastify({ logger: false })
    fastify.addSchema({
      $id: 'ErrorResponse',
      type: 'object',
      properties: {
        code: { type: 'string' },
        message: { type: 'string' },
        traceId: { type: 'string' },
      },
      required: ['code', 'message'],
    })
    const jwtPlugin = await loadJwtPlugin()
    await fastify.register(fp(jwtPlugin, { name: 'auth-jwt-for-pubkey-test' }))
    const authRoutesMod = await import(`../../src/app/auth/routes/auth.js?bust=${Date.now()}`)
    await fastify.register(authRoutesMod.default)
    await fastify.ready()
    const resp = await fastify.inject({ method: 'GET', url: '/auth/public-key' })
    assert.strictEqual(resp.statusCode, 200)
    // The body is the literal PEM bytes (no JSON envelope).
    assert.ok(resp.body.includes('BEGIN PUBLIC KEY'))
    assert.strictEqual(resp.headers['content-type']?.toString().includes('pem-file'), true)
    await fastify.close()
  } finally {
    process.env = prev
  }
})

test('GET /auth/public-key returns 404 when JWT_PUBLIC_KEY is unset (HS256 dev mode)', async () => {
  const prev = { ...process.env }
  try {
    process.env.NODE_ENV = 'development'
    delete process.env.JWT_PUBLIC_KEY
    process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'

    const fastify = Fastify({ logger: false })
    fastify.addSchema({
      $id: 'ErrorResponse',
      type: 'object',
      properties: {
        code: { type: 'string' },
        message: { type: 'string' },
        traceId: { type: 'string' },
      },
      required: ['code', 'message'],
    })
    const jwtPlugin = await loadJwtPlugin()
    await fastify.register(fp(jwtPlugin, { name: 'auth-jwt-for-pubkey-404-test' }))
    const authRoutesMod = await import(`../../src/app/auth/routes/auth.js?bust=${Date.now()}`)
    await fastify.register(authRoutesMod.default)
    await fastify.ready()
    const resp = await fastify.inject({ method: 'GET', url: '/auth/public-key' })
    assert.strictEqual(resp.statusCode, 404)
    await fastify.close()
  } finally {
    process.env = prev
  }
})
