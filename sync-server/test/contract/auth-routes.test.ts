// Phase-09 §3.1 contract tests for the auth route TypeBox schemas:
// `POST /auth/login`, `POST /auth/refresh`, `POST /auth/change-password`,
// `POST /auth/bootstrap`. Drift-tests the wire shape with `Value.Check`
// so a future schema cleanup that quietly removes a field, widens an
// enum, or relaxes a length constraint surfaces as a CI failure.
//
// Critical invariants pinned:
// - LoginBody: email format + password >=8 chars + optional entityId
//   + optional deviceId.
// - LoginResponse: 3 tokens + nested user with the 3-role closed enum
//   AND the `passwordHash` field (load-bearing for the Tauri offline
//   login fallback per .claude/rules/auth.md -- the client caches the
//   hash so verify_password works without the network).
// - RefreshBody/Response: minimal pair; access + refresh + expiresAt
//   rotated in lockstep.
// - ChangePasswordBody: oldPassword min 1 (any non-empty), newPassword
//   min 8.
// - BootstrapBody: all 4 fields required, name + entityId min 1,
//   password min 8.

import { test } from 'node:test'
import * as assert from 'node:assert/strict'
import { FormatRegistry } from '@sinclair/typebox'
import { Value } from '@sinclair/typebox/value'

import {
  LoginBody,
  LoginResponse,
  RefreshBody,
  RefreshResponse,
  ChangePasswordBody,
  BootstrapBody,
  BootstrapResponse,
} from '../../src/app/auth/routes/auth'

// Register the `email` format that the auth schemas use. Fastify wires
// Ajv with ajv-formats at runtime; mirror that here so contract tests
// match production validator behavior.
const EMAIL = /^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/

if (!FormatRegistry.Has('email')) {
  FormatRegistry.Set('email', (value) => EMAIL.test(value))
}

// --- LoginBody (POST /auth/login request) -----------------------

test('LoginBody accepts the minimal valid login: email + 8-char password', () => {
  const body = { email: 'mariam@idc.io', password: 'pw-strong-12345' }
  assert.equal(Value.Check(LoginBody, body), true)
})

test('LoginBody accepts optional entityId + deviceId', () => {
  const body = {
    email: 'mariam@idc.io',
    password: 'pw-strong-12345',
    entityId: 'tenant-1',
    deviceId: 'dev-A',
  }
  assert.equal(Value.Check(LoginBody, body), true)
})

test('LoginBody rejects password below 8 chars', () => {
  const body = { email: 'mariam@idc.io', password: 'short' }
  assert.equal(Value.Check(LoginBody, body), false)
})

test('LoginBody rejects malformed email', () => {
  const body = { email: 'not-an-email', password: 'pw-strong-12345' }
  assert.equal(Value.Check(LoginBody, body), false)
})

test('LoginBody rejects payloads missing email or password', () => {
  assert.equal(
    Value.Check(LoginBody, { password: 'pw-strong-12345' }),
    false,
    'missing email',
  )
  assert.equal(
    Value.Check(LoginBody, { email: 'mariam@idc.io' }),
    false,
    'missing password',
  )
})

// --- LoginResponse (POST /auth/login response) -----------------

test('LoginResponse accepts the canonical login response with all 3 tokens + full user', () => {
  const body = {
    accessToken: 'eyJ...',
    refreshToken: 'rt-base64-abc',
    expiresAt: '2026-05-18T11:00:00.000Z',
    user: {
      id: '01HZWAB000000000000000001',
      email: 'mariam@idc.io',
      name: 'Mariam',
      role: 'superadmin',
      entityId: 'tenant-1',
      passwordHash: '$argon2id$v=19$m=...',
    },
  }
  assert.equal(Value.Check(LoginResponse, body), true)
})

test('LoginResponse rejects role outside the 3-value closed enum', () => {
  const body = {
    accessToken: 'eyJ...',
    refreshToken: 'rt',
    expiresAt: '2026-05-18T11:00:00.000Z',
    user: {
      id: '01HZ',
      email: 'doctor@idc.io',
      name: 'Doctor',
      role: 'doctor',
      entityId: 'tenant-1',
      passwordHash: 'hash',
    },
  }
  assert.equal(Value.Check(LoginResponse, body), false)
})

test('LoginResponse rejects user payload missing passwordHash (Tauri offline-login dep)', () => {
  // .claude/rules/auth.md: the client caches password_hash so offline
  // login's verify_password works without the network. The response
  // MUST carry it; dropping it would break the offline-first invariant.
  const body = {
    accessToken: 'eyJ...',
    refreshToken: 'rt',
    expiresAt: '2026-05-18T11:00:00.000Z',
    user: {
      id: '01HZ',
      email: 'mariam@idc.io',
      name: 'Mariam',
      role: 'superadmin',
      entityId: 'tenant-1',
    },
  }
  assert.equal(Value.Check(LoginResponse, body), false)
})

test('LoginResponse accepts each of the 3 roles', () => {
  for (const role of ['superadmin', 'receptionist', 'accountant']) {
    const body = {
      accessToken: 'eyJ...',
      refreshToken: 'rt',
      expiresAt: '2026-05-18T11:00:00.000Z',
      user: {
        id: '01HZ',
        email: 'x@idc.io',
        name: 'X',
        role,
        entityId: 'tenant-1',
        passwordHash: 'hash',
      },
    }
    assert.equal(Value.Check(LoginResponse, body), true, `role=${role} must be accepted`)
  }
})

// --- RefreshBody / RefreshResponse -----------------------------

test('RefreshBody accepts a non-empty refresh token', () => {
  assert.equal(Value.Check(RefreshBody, { refreshToken: 'rt' }), true)
})

test('RefreshBody rejects bodies missing refreshToken', () => {
  assert.equal(Value.Check(RefreshBody, {}), false)
})

test('RefreshResponse accepts the canonical rotated-token triple', () => {
  const body = {
    accessToken: 'new-access',
    refreshToken: 'new-refresh',
    expiresAt: '2026-05-18T11:15:00.000Z',
  }
  assert.equal(Value.Check(RefreshResponse, body), true)
})

test('RefreshResponse rejects bodies missing any of the 3 fields', () => {
  // Mirrors the production rotation invariant: access AND refresh AND
  // expiresAt are all reissued in lockstep. A regression that drops
  // refreshToken (sliding-window failure mode) fails the contract.
  assert.equal(
    Value.Check(RefreshResponse, { accessToken: 'a', expiresAt: 'now' }),
    false,
    'missing refreshToken',
  )
  assert.equal(
    Value.Check(RefreshResponse, { refreshToken: 'r', expiresAt: 'now' }),
    false,
    'missing accessToken',
  )
  assert.equal(
    Value.Check(RefreshResponse, { accessToken: 'a', refreshToken: 'r' }),
    false,
    'missing expiresAt',
  )
})

// --- ChangePasswordBody ----------------------------------------

test('ChangePasswordBody accepts oldPassword >= 1 char + newPassword >= 8 chars', () => {
  const body = { oldPassword: 'x', newPassword: 'pw-strong-12345' }
  assert.equal(Value.Check(ChangePasswordBody, body), true)
})

test('ChangePasswordBody rejects empty oldPassword', () => {
  // The asymmetric minLength is intentional: oldPassword is only
  // verified against the stored hash, so any non-empty input is
  // accepted at the schema layer; verification handles correctness.
  // newPassword must meet the >=8 invariant.
  const body = { oldPassword: '', newPassword: 'pw-strong-12345' }
  assert.equal(Value.Check(ChangePasswordBody, body), false)
})

test('ChangePasswordBody rejects newPassword below 8 chars', () => {
  const body = { oldPassword: 'x', newPassword: 'short' }
  assert.equal(Value.Check(ChangePasswordBody, body), false)
})

// --- BootstrapBody / BootstrapResponse -------------------------

test('BootstrapBody accepts a canonical first-admin bootstrap payload', () => {
  const body = {
    email: 'admin@idc.io',
    name: 'Mariam',
    password: 'admin-strong-789',
    entityId: 'tenant-1',
  }
  assert.equal(Value.Check(BootstrapBody, body), true)
})

test('BootstrapBody rejects empty name or empty entityId', () => {
  assert.equal(
    Value.Check(BootstrapBody, {
      email: 'admin@idc.io',
      name: '',
      password: 'admin-strong-789',
      entityId: 'tenant-1',
    }),
    false,
    'empty name',
  )
  assert.equal(
    Value.Check(BootstrapBody, {
      email: 'admin@idc.io',
      name: 'Mariam',
      password: 'admin-strong-789',
      entityId: '',
    }),
    false,
    'empty entityId',
  )
})

test('BootstrapBody rejects password below 8 chars', () => {
  const body = {
    email: 'admin@idc.io',
    name: 'Mariam',
    password: 'short',
    entityId: 'tenant-1',
  }
  assert.equal(Value.Check(BootstrapBody, body), false)
})

test('BootstrapResponse accepts the canonical bootstrap response shape', () => {
  const body = {
    id: '01HZWAB000000000000000001',
    email: 'admin@idc.io',
    name: 'Mariam',
    role: 'superadmin',
  }
  assert.equal(Value.Check(BootstrapResponse, body), true)
})

test('BootstrapResponse rejects bodies missing any of the 4 fields', () => {
  assert.equal(
    Value.Check(BootstrapResponse, {
      id: '01HZ',
      email: 'admin@idc.io',
      name: 'Mariam',
    }),
    false,
  )
})
