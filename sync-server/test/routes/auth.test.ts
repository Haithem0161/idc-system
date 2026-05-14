import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-auth'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  authService: {
    bootstrapSuperadmin: (email: string, name: string, password: string, entityId: string) => Promise<unknown>
  }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string }>
}

async function buildWithBootstrap (t: Parameters<typeof build>[0]) {
  const app = await build(t)
  const typed = app as unknown as FastifyAppLike
  await typed.authService.bootstrapSuperadmin('admin@example.com', 'Admin', 'hunter22', TENANT)
  return app
}

test('POST /auth/bootstrap-superadmin creates first user, refuses second', async (t) => {
  const app = await build(t)
  const first = await app.inject({
    method: 'POST',
    url: '/auth/bootstrap-superadmin',
    payload: { email: 'first@example.com', name: 'First', password: 'hunter22', entityId: 'tenant-bs' },
  })
  assert.strictEqual(first.statusCode, 200, first.payload)

  const second = await app.inject({
    method: 'POST',
    url: '/auth/bootstrap-superadmin',
    payload: { email: 'second@example.com', name: 'Second', password: 'hunter22', entityId: 'tenant-bs' },
  })
  assert.strictEqual(second.statusCode, 409, second.payload)
  assert.strictEqual(JSON.parse(second.payload).code, 'VALIDATION_ERROR')
})

test('POST /auth/login returns tokens for valid credentials', async (t) => {
  const app = await buildWithBootstrap(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
    headers: { 'x-device-id': 'dev-1' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.ok(body.accessToken)
  assert.ok(body.refreshToken)
  assert.strictEqual(body.user.email, 'admin@example.com')
  assert.strictEqual(body.user.role, 'superadmin')
})

test('POST /auth/login returns 401 for wrong password', async (t) => {
  const app = await buildWithBootstrap(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'wrongpw1', entityId: TENANT },
  })
  assert.strictEqual(res.statusCode, 401)
  assert.strictEqual(JSON.parse(res.payload).code, 'NOT_AUTHENTICATED')
})

test('POST /auth/refresh rotates tokens; old token rejected on reuse', async (t) => {
  const app = await buildWithBootstrap(t)
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  const original = JSON.parse(login.payload).refreshToken as string

  const rotated = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken: original },
  })
  assert.strictEqual(rotated.statusCode, 200, rotated.payload)

  const reuse = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken: original },
  })
  assert.strictEqual(reuse.statusCode, 401)
})

test('POST /auth/change-password updates hash and revokes refresh tokens', async (t) => {
  const app = await buildWithBootstrap(t)
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  const tokens = JSON.parse(login.payload)

  const changed = await app.inject({
    method: 'POST',
    url: '/auth/change-password',
    headers: { authorization: `Bearer ${tokens.accessToken}` },
    payload: { oldPassword: 'hunter22', newPassword: 'newpassword1' },
  })
  assert.strictEqual(changed.statusCode, 204)

  // Old refresh token is revoked.
  const reuse = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken: tokens.refreshToken },
  })
  assert.strictEqual(reuse.statusCode, 401)

  // Old password no longer works.
  const failedLogin = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  assert.strictEqual(failedLogin.statusCode, 401)

  // New password works.
  const newLogin = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'newpassword1', entityId: TENANT },
  })
  assert.strictEqual(newLogin.statusCode, 200)
})

test('POST /auth/logout revokes the refresh token', async (t) => {
  const app = await buildWithBootstrap(t)
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  const refreshToken = JSON.parse(login.payload).refreshToken as string

  const logout = await app.inject({
    method: 'POST',
    url: '/auth/logout',
    payload: { refreshToken },
  })
  assert.strictEqual(logout.statusCode, 204)

  const reuse = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken },
  })
  assert.strictEqual(reuse.statusCode, 401)
})

test('POST /auth/login accepts mixed-case email via case-insensitive lookup (P02-G37)', async (t) => {
  const app = await buildWithBootstrap(t)
  const mixed = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'Admin@Example.COM', password: 'hunter22', entityId: TENANT },
  })
  assert.strictEqual(mixed.statusCode, 200, mixed.payload)
  const body = JSON.parse(mixed.payload)
  assert.strictEqual(body.user.email, 'admin@example.com')

  const upper = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'ADMIN@EXAMPLE.COM', password: 'hunter22', entityId: TENANT },
  })
  assert.strictEqual(upper.statusCode, 200)
})

test('POST /auth/login returns 401 for unknown email without revealing existence', async (t) => {
  const app = await buildWithBootstrap(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'ghost@example.com', password: 'hunter22', entityId: TENANT },
  })
  // Same status + same code as wrong-password: never disambiguate.
  assert.strictEqual(res.statusCode, 401)
  assert.strictEqual(JSON.parse(res.payload).code, 'NOT_AUTHENTICATED')
})

test('POST /auth/login isolates by tenant: matching email in other tenant 401s', async (t) => {
  const app = await build(t)
  const typed = app as unknown as FastifyAppLike
  await typed.authService.bootstrapSuperadmin('shared@example.com', 'A', 'hunter22', 'tenant-A')
  await typed.authService.bootstrapSuperadmin('shared@example.com', 'B', 'differentpw1', 'tenant-B')

  // Tenant-A's password against tenant-B does NOT cross over.
  const cross = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'shared@example.com', password: 'hunter22', entityId: 'tenant-B' },
  })
  assert.strictEqual(cross.statusCode, 401)
})

test('POST /auth/login issues an access token whose TTL is 15 minutes (900s)', async (t) => {
  const app = await buildWithBootstrap(t)
  const before = Math.floor(Date.now() / 1000)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  const body = JSON.parse(res.payload)
  // The `expiresAt` field is the access-token expiry: now + 15min.
  const expiresAtSec = Math.floor(new Date(body.expiresAt).getTime() / 1000)
  const delta = expiresAtSec - before
  assert.ok(delta >= 900 - 2 && delta <= 900 + 2, `expected ~900s, got ${delta}`)
})

test('POST /auth/login persists a 30-day refresh token (TTL invariant, P02-G05)', async (t) => {
  const app = await buildWithBootstrap(t)
  const beforeMs = Date.now()
  await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })

  // Inspect the in-memory store directly.
  const stored = app as unknown as {
    authService: {
      userStore: { tokens: Map<string, { expiresAt: string, createdAt: string }> }
    }
  }
  const tokens = stored.authService.userStore.tokens
  assert.ok(tokens.size >= 1, 'at least one refresh token should be persisted')
  const [, record] = tokens.entries().next().value as [string, { expiresAt: string, createdAt: string }]
  const expiresMs = new Date(record.expiresAt).getTime()
  const ttlMs = expiresMs - beforeMs
  const thirtyDays = 30 * 24 * 60 * 60 * 1000
  // Allow 5s skew either way.
  assert.ok(
    Math.abs(ttlMs - thirtyDays) < 5_000,
    `expected ~30-day TTL, got ${ttlMs}ms`,
  )
})

test('Refresh tokens are persisted as sha256 hashes, never plaintext (P02-G05 / security)', async (t) => {
  const app = await buildWithBootstrap(t)
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  const plaintext = JSON.parse(login.payload).refreshToken as string
  assert.ok(plaintext.length >= 32, 'plaintext refresh token should be long')

  const stored = app as unknown as {
    authService: {
      userStore: { tokenHashes: Map<string, string>, tokens: Map<string, { tokenHash: string }> }
    }
  }
  const tokens = stored.authService.userStore.tokens
  // No persisted row's tokenHash should equal the plaintext bytes.
  for (const [, record] of tokens.entries()) {
    assert.notStrictEqual(record.tokenHash, plaintext, 'token must be stored as hash, not plaintext')
    // sha256 hex is 64 chars.
    assert.strictEqual(record.tokenHash.length, 64, 'tokenHash should be sha256 hex')
  }
})

test('POST /auth/login persists deviceId onto the refresh-token row (P02-G04)', async (t) => {
  const app = await buildWithBootstrap(t)
  await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
    headers: { 'x-device-id': 'device-A-uuid' },
  })

  const stored = app as unknown as {
    authService: { userStore: { tokens: Map<string, { deviceId: string | null }> } }
  }
  const tokens = Array.from(stored.authService.userStore.tokens.values())
  assert.ok(tokens.length >= 1, 'token should be persisted')
  assert.ok(
    tokens.some((t) => t.deviceId === 'device-A-uuid'),
    'token row should carry the deviceId from x-device-id header',
  )
})

test('POST /auth/login from two devices creates two refresh-token rows (multi-device)', async (t) => {
  const app = await buildWithBootstrap(t)
  await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
    headers: { 'x-device-id': 'device-A' },
  })
  await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
    headers: { 'x-device-id': 'device-B' },
  })

  const stored = app as unknown as {
    authService: { userStore: { tokens: Map<string, { deviceId: string | null }> } }
  }
  const tokens = Array.from(stored.authService.userStore.tokens.values())
  // Both device-A and device-B rows present, no collision.
  const ids = new Set(tokens.map((t) => t.deviceId))
  assert.ok(ids.has('device-A'))
  assert.ok(ids.has('device-B'))
})

test('POST /auth/login returns 400 when email is missing (request schema)', async (t) => {
  const app = await build(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { password: 'hunter22', entityId: TENANT },
  })
  // Fastify validation returns 400 with body-validation error.
  assert.ok(res.statusCode === 400, `expected 400, got ${res.statusCode}: ${res.payload}`)
})

test('POST /auth/refresh returns 401 for a syntactically valid but unknown token', async (t) => {
  const app = await buildWithBootstrap(t)
  const fake = 'f'.repeat(64)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken: fake },
  })
  assert.strictEqual(res.statusCode, 401)
})

test('POST /auth/change-password 400s for wrong old password (P02 §2.3 negative path)', async (t) => {
  const app = await buildWithBootstrap(t)
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
  })
  const tokens = JSON.parse(login.payload)

  const wrongOld = await app.inject({
    method: 'POST',
    url: '/auth/change-password',
    headers: { authorization: `Bearer ${tokens.accessToken}` },
    payload: { oldPassword: 'WRONG-OLD-PW', newPassword: 'newpassword1' },
  })
  // 401 or 400 — never silently succeed.
  assert.ok(
    [400, 401].includes(wrongOld.statusCode),
    `expected 400/401, got ${wrongOld.statusCode}: ${wrongOld.payload}`,
  )

  // Refresh token still valid (the change-password attempt rolled back).
  const stillWorks = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken: tokens.refreshToken },
  })
  assert.strictEqual(stillWorks.statusCode, 200, stillWorks.payload)
})

test('POST /auth/change-password rejects unauthenticated callers (no Authorization header)', async (t) => {
  const app = await buildWithBootstrap(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/change-password',
    payload: { oldPassword: 'hunter22', newPassword: 'newpassword1' },
  })
  assert.ok([401, 403].includes(res.statusCode), `expected 401/403, got ${res.statusCode}: ${res.payload}`)
})
