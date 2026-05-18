// Phase-09 DEF-007 G15: Prisma seed.ts regression tests.
//
// The seed script is the canonical bootstrap path -- `pnpm prisma db seed`
// reads env vars and idempotently inserts a superadmin. Phase-02 §7.21
// advertised the env-driven bootstrap behaviour but only an in-process
// best-effort call inside `plugins/auth-services.ts` (with a silent
// `.catch(() => undefined)`) actually existed. A standalone seed script
// + idempotency contract + auto-skip-on-missing-env is what gates the
// supervised deploy story.
//
// These tests drive `bootstrapSuperadminFromEnv` directly against the
// in-memory `MemoryUserStore` (the production swap is the Prisma-backed
// `PrismaUserStore`; both implement `UserRepository & RefreshTokenRepository`).
// The discriminated union return type makes outcome assertions structural,
// not log-string parsing.

import { test } from 'node:test'
import * as assert from 'node:assert/strict'

import { bootstrapSuperadminFromEnv } from '../../prisma/seed.js'
import { MemoryUserStore } from '../../src/app/auth/infrastructure/memory-user-store.js'

class TestLogger {
  readonly info: string[] = []
  readonly warn: string[] = []
  readonly error: string[] = []
  info_ (msg: string): void { this.info.push(msg) }
  warn_ (msg: string): void { this.warn.push(msg) }
  error_ (msg: string): void { this.error.push(msg) }
  asSeedLogger () {
    return {
      info: (m: string): void => { this.info.push(m) },
      warn: (m: string): void => { this.warn.push(m) },
      error: (m: string): void => { this.error.push(m) },
    }
  }
}

// --- Skip cases (missing env) -------------------------------------------

test('seed skips with explicit reason when BOOTSTRAP_SUPERADMIN_EMAIL is unset', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  const outcome = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  assert.equal(outcome.status, 'skipped')
  if (outcome.status === 'skipped') {
    assert.match(outcome.reason, /BOOTSTRAP_SUPERADMIN_EMAIL/)
  }
  assert.equal(users.users.size, 0, 'must not create a row when skipping')
})

test('seed skips when BOOTSTRAP_SUPERADMIN_PASSWORD is unset', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  const outcome = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  assert.equal(outcome.status, 'skipped')
  if (outcome.status === 'skipped') {
    assert.match(outcome.reason, /BOOTSTRAP_SUPERADMIN_PASSWORD/)
  }
})

test('seed skips when BOOTSTRAP_TENANT_ID is unset', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  const outcome = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
    },
    log.asSeedLogger()
  )
  assert.equal(outcome.status, 'skipped')
  if (outcome.status === 'skipped') {
    assert.match(outcome.reason, /BOOTSTRAP_TENANT_ID/)
  }
})

test('seed skip reason enumerates ALL missing vars at once (deployer fixes in one redeploy)', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  const outcome = await bootstrapSuperadminFromEnv(users, {}, log.asSeedLogger())
  assert.equal(outcome.status, 'skipped')
  if (outcome.status === 'skipped') {
    assert.match(outcome.reason, /BOOTSTRAP_SUPERADMIN_EMAIL/)
    assert.match(outcome.reason, /BOOTSTRAP_SUPERADMIN_PASSWORD/)
    assert.match(outcome.reason, /BOOTSTRAP_TENANT_ID/)
  }
})

test('seed treats whitespace-only env value as missing', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  const outcome = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: '   ',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  assert.equal(outcome.status, 'skipped')
})

// --- Happy path ---------------------------------------------------------

test('seed bootstraps the superadmin row when env is complete and no users exist', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  const outcome = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  assert.equal(outcome.status, 'bootstrapped')
  if (outcome.status === 'bootstrapped') {
    assert.equal(outcome.email, 'admin@idc.iq')
    assert.equal(outcome.tenantId, 'tenant-1')
  }
  assert.equal(users.users.size, 1, 'exactly one row created')
})

test('seeded user is a superadmin scoped to the supplied tenant', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  const created = await users.getByEmail('admin@idc.iq', 'tenant-1')
  assert.ok(created)
  assert.equal(created.role, 'superadmin')
  assert.equal(created.entityId, 'tenant-1')
  assert.notEqual(created.passwordHash, 'correct-horse-battery-staple',
    'plaintext password must NOT be stored -- argon2 hash invariant')
  assert.match(created.passwordHash, /^\$argon2/,
    'argon2 hash prefix is the contract for the AuthService login path')
})

test('seed uses BOOTSTRAP_SUPERADMIN_NAME when provided', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'asma@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
      BOOTSTRAP_SUPERADMIN_NAME: 'Asma Karim',
    },
    log.asSeedLogger()
  )
  const created = await users.getByEmail('asma@idc.iq', 'tenant-1')
  assert.equal(created?.name, 'Asma Karim')
})

test('seed defaults the name to "Bootstrap Admin" when BOOTSTRAP_SUPERADMIN_NAME is missing', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  const created = await users.getByEmail('admin@idc.iq', 'tenant-1')
  assert.equal(created?.name, 'Bootstrap Admin')
})

// --- Idempotency --------------------------------------------------------

test('seed is a no-op when a user already exists (idempotency contract)', async () => {
  const users = new MemoryUserStore()
  const log = new TestLogger()
  // First run: bootstraps.
  const first = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  assert.equal(first.status, 'bootstrapped')
  // Second run with identical env: must skip without erroring.
  const second = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  assert.equal(second.status, 'already_exists')
  if (second.status === 'already_exists') {
    assert.equal(second.userCount, 1)
  }
  assert.equal(users.users.size, 1, 'still exactly one user after two seed runs')
})

test('seed idempotency holds even when env credentials differ on second run', async () => {
  // If an operator runs the seed twice with different credentials, the
  // second run must NOT create a second admin. This is the "no second
  // admin under any circumstance" invariant.
  const users = new MemoryUserStore()
  const log = new TestLogger()
  await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'admin@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'correct-horse-battery-staple',
      BOOTSTRAP_TENANT_ID: 'tenant-1',
    },
    log.asSeedLogger()
  )
  const second = await bootstrapSuperadminFromEnv(
    users,
    {
      BOOTSTRAP_SUPERADMIN_EMAIL: 'imposter@idc.iq',
      BOOTSTRAP_SUPERADMIN_PASSWORD: 'different-password',
      BOOTSTRAP_TENANT_ID: 'tenant-2',
    },
    log.asSeedLogger()
  )
  assert.equal(second.status, 'already_exists')
  assert.equal(users.users.size, 1)
  // The first admin survives; the imposter row never lands.
  const original = await users.getByEmail('admin@idc.iq', 'tenant-1')
  const imposter = await users.getByEmail('imposter@idc.iq', 'tenant-2')
  assert.ok(original, 'original admin survives')
  assert.equal(imposter, null, 'imposter row never created')
})

// --- Static-source guarantees -------------------------------------------

test('package.json wires the prisma.seed config to the seed script', async () => {
  const { readFileSync } = await import('node:fs')
  const { join } = await import('node:path')
  const pkgPath = join(__dirname, '..', '..', 'package.json')
  const pkg = JSON.parse(readFileSync(pkgPath, 'utf8')) as {
    prisma?: { seed?: string }
  }
  assert.ok(pkg.prisma, 'package.json must declare a prisma block')
  assert.ok(pkg.prisma.seed, 'prisma.seed must be set so `prisma db seed` works')
  assert.match(
    pkg.prisma.seed,
    /prisma\/seed\.ts/,
    'seed command must point at prisma/seed.ts'
  )
  assert.match(
    pkg.prisma.seed,
    /^tsx /,
    'seed must run via tsx (NodeNext + ESM)'
  )
})
