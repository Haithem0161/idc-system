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
