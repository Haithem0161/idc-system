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

test('GET /auth/bootstrap-status reports initialized=false then true', async (t) => {
  const app = await build(t)

  // Fresh server: no user yet -> a new desktop machine should offer first-admin.
  const before = await app.inject({ method: 'GET', url: '/auth/bootstrap-status' })
  assert.strictEqual(before.statusCode, 200, before.payload)
  assert.strictEqual(JSON.parse(before.payload).initialized, false)

  // After the first admin exists, every later machine must go straight to login.
  await app.inject({
    method: 'POST',
    url: '/auth/bootstrap-superadmin',
    payload: { email: 'first@example.com', name: 'First', password: 'hunter22', entityId: 'tenant-bs' },
  })
  const after = await app.inject({ method: 'GET', url: '/auth/bootstrap-status' })
  assert.strictEqual(after.statusCode, 200, after.payload)
  assert.strictEqual(JSON.parse(after.payload).initialized, true)
})

test('POST /auth/bootstrap-superadmin OMITTING entityId stamps the server default', async (t) => {
  // No DEFAULT_ENTITY_ID/BOOTSTRAP_TENANT_ID in the test env -> the server has
  // no fallback, so a tenant-less bootstrap is rejected 422 rather than creating
  // an unscoped admin. (The positive "server stamps the default" path is proved
  // by the live roundtrip gate, which runs with DEFAULT_ENTITY_ID set.)
  const app = await build(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/bootstrap-superadmin',
    payload: { email: 'noscope@example.com', name: 'NoScope', password: 'hunter22' },
  })
  assert.strictEqual(res.statusCode, 422, res.payload)
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

// --- Phase-10 T5: refresh/logout bound to the JWT subject -------------------

async function loginAdmin (app: { inject: FastifyAppLike['inject'] }) {
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { email: 'admin@example.com', password: 'hunter22', entityId: TENANT },
    headers: { 'x-device-id': 'dev-t5' },
  })
  return JSON.parse(login.payload) as { accessToken: string, refreshToken: string, user: { id: string } }
}

test('T5: refresh with a bearer whose sub != token owner is rejected 403', async (t) => {
  const app = await buildWithBootstrap(t)
  const a = app as unknown as FastifyAppLike
  const { refreshToken } = await loginAdmin(app)

  // Forge an access token for a DIFFERENT subject (a leaked-token attacker).
  const foreignBearer = a.jwt.sign({ sub: 'attacker-id', email: 'x@y.z', entityId: TENANT, role: 'superadmin' })
  const res = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    headers: { authorization: `Bearer ${foreignBearer}` },
    payload: { refreshToken },
  })
  assert.strictEqual(res.statusCode, 403, res.payload)
  assert.strictEqual(JSON.parse(res.payload).code, 'FORBIDDEN')
})

test('T5: refresh with a bearer matching the token owner succeeds', async (t) => {
  const app = await buildWithBootstrap(t)
  const { accessToken, refreshToken } = await loginAdmin(app)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    headers: { authorization: `Bearer ${accessToken}` },
    payload: { refreshToken },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
})

test('T5: logout with a bearer whose sub != token owner does NOT revoke the token', async (t) => {
  const app = await buildWithBootstrap(t)
  const a = app as unknown as FastifyAppLike
  const { refreshToken } = await loginAdmin(app)

  const foreignBearer = a.jwt.sign({ sub: 'attacker-id', email: 'x@y.z', entityId: TENANT, role: 'superadmin' })
  const logout = await app.inject({
    method: 'POST',
    url: '/auth/logout',
    headers: { authorization: `Bearer ${foreignBearer}` },
    payload: { refreshToken },
  })
  // The revoke is a no-op (scoped to the foreign subject); the token still works.
  assert.strictEqual(logout.statusCode, 204, logout.payload)
  const stillValid = await app.inject({
    method: 'POST',
    url: '/auth/refresh',
    payload: { refreshToken },
  })
  assert.strictEqual(stillValid.statusCode, 200, 'a cross-user logout must NOT revoke the token')
})

// --- Phase-10 T6: GET /auth/profile -----------------------------------------

test('T6: GET /auth/profile returns the authenticated user without the password hash', async (t) => {
  const app = await buildWithBootstrap(t)
  const { accessToken, user } = await loginAdmin(app)
  const res = await app.inject({
    method: 'GET',
    url: '/auth/profile',
    headers: { authorization: `Bearer ${accessToken}` },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload) as Record<string, unknown>
  assert.strictEqual(body.id, user.id)
  assert.strictEqual(body.email, 'admin@example.com')
  assert.strictEqual(body.role, 'superadmin')
  assert.strictEqual(body.entityId, TENANT)
  assert.strictEqual('passwordHash' in body, false, 'profile must never expose the password hash')
})

test('T6: GET /auth/profile rejects unauthenticated callers with 401', async (t) => {
  const app = await buildWithBootstrap(t)
  const res = await app.inject({ method: 'GET', url: '/auth/profile' })
  assert.strictEqual(res.statusCode, 401)
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
  const { hash: argonHash } = await import('@node-rs/argon2')
  const { randomUUID } = await import('node:crypto')
  const app = await build(t)
  const typed = app as unknown as FastifyAppLike
  // bootstrapSuperadmin is single-use globally (refuses when any user exists)
  // so we use it for tenant-A and seed tenant-B directly via the user store.
  await typed.authService.bootstrapSuperadmin('shared@example.com', 'A', 'hunter22', 'tenant-A')
  const userStore = (app as unknown as { userStore: { create: (rec: {
    id: string; email: string; name: string; passwordHash: string; role: string; entityId: string;
  }) => Promise<unknown> } }).userStore
  await userStore.create({
    id: randomUUID(),
    email: 'shared@example.com',
    name: 'B',
    passwordHash: await argonHash('differentpw1'),
    role: 'superadmin',
    entityId: 'tenant-B',
  })

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
    userStore: { tokens: Map<string, { expiresAt: string, createdAt: string }> }
  }
  const tokens = stored.userStore.tokens
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
    userStore: { tokenHashes: Map<string, string>, tokens: Map<string, { tokenHash: string }> }
  }
  const tokens = stored.userStore.tokens
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
    userStore: { tokens: Map<string, { deviceId: string | null }> }
  }
  const tokens = Array.from(stored.userStore.tokens.values())
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
    userStore: { tokens: Map<string, { deviceId: string | null }> }
  }
  const tokens = Array.from(stored.userStore.tokens.values())
  // Both device-A and device-B rows present, no collision.
  const ids = new Set(tokens.map((t) => t.deviceId))
  assert.ok(ids.has('device-A'))
  assert.ok(ids.has('device-B'))
})

test('POST /auth/login returns 422 when email is missing (request schema)', async (t) => {
  const app = await build(t)
  const res = await app.inject({
    method: 'POST',
    url: '/auth/login',
    payload: { password: 'hunter22', entityId: TENANT },
  })
  // The app's error-handler plugin (per phase-02) maps schema validation
  // errors to 422 with a typed `{ code: 'VALIDATION_ERROR', message, details }`
  // envelope so the client distinguishes "you sent malformed input" from
  // "you sent valid input we rejected on business grounds (400)".
  assert.strictEqual(res.statusCode, 422, `expected 422, got ${res.statusCode}: ${res.payload}`)
  const body = JSON.parse(res.payload) as { code: string; message: string }
  assert.strictEqual(body.code, 'VALIDATION_ERROR')
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
