/**
 * Prisma seed script for idempotent superadmin bootstrap (phase-09 DEF-007 G15).
 *
 * Reads `BOOTSTRAP_SUPERADMIN_EMAIL`, `BOOTSTRAP_SUPERADMIN_PASSWORD`, and
 * `BOOTSTRAP_TENANT_ID` from the environment; if any are missing, the seed
 * exits cleanly (a deploy without bootstrap is a valid state). If a user
 * already exists, the seed exits cleanly too -- this is the idempotency
 * contract: running `pnpm prisma db seed` twice in a row never produces a
 * second admin and never errors.
 *
 * The runtime mirrors the prior in-process bootstrap at
 * `plugins/auth-services.ts` so the produced rows are byte-compatible with
 * the existing AuthService.login flow (Argon2 hash, randomUUID id, role
 * `'superadmin'`, the supplied entityId as the tenant scope).
 *
 * Wired via `package.json` `"prisma": { "seed": "tsx prisma/seed.ts" }`.
 */

import { PrismaClient } from '@prisma/client'

import type {
  RefreshTokenRepository,
  UserRepository,
} from '../src/app/auth/domain/repositories.js'
import { AuthService } from '../src/app/auth/service/auth-service.js'
import { PrismaUserStore } from '../src/app/auth/infrastructure/prisma/user-store.js'

export interface SeedEnv {
  BOOTSTRAP_SUPERADMIN_EMAIL?: string
  BOOTSTRAP_SUPERADMIN_PASSWORD?: string
  BOOTSTRAP_TENANT_ID?: string
  BOOTSTRAP_SUPERADMIN_NAME?: string
}

export interface SeedLogger {
  info: (msg: string) => void
  warn: (msg: string) => void
  error: (msg: string) => void
}

export type SeedOutcome =
  | { status: 'skipped'; reason: string }
  | { status: 'already_exists'; userCount: number }
  | { status: 'bootstrapped'; email: string; tenantId: string }

/**
 * Pure-domain bootstrap helper. Pulled out of `main()` so tests can drive it
 * against a MemoryUserStore without booting Prisma. The helper:
 *
 *   1. Validates that all three required env vars are present.
 *   2. Checks the user count -- if any user exists, treats it as a no-op
 *      (idempotency).
 *   3. Delegates to `AuthService.bootstrapSuperadmin` which Argon2-hashes the
 *      password, mints a UUID, and writes the row.
 *
 * Returns a discriminated union so callers can branch on outcome
 * (skipped/already_exists/bootstrapped) without parsing log strings.
 */
export async function bootstrapSuperadminFromEnv (
  users: UserRepository & RefreshTokenRepository,
  env: SeedEnv,
  log: SeedLogger
): Promise<SeedOutcome> {
  const email = env.BOOTSTRAP_SUPERADMIN_EMAIL?.trim()
  const password = env.BOOTSTRAP_SUPERADMIN_PASSWORD?.trim()
  const tenant = env.BOOTSTRAP_TENANT_ID?.trim()
  const name = env.BOOTSTRAP_SUPERADMIN_NAME?.trim() ?? 'Bootstrap Admin'

  if (!email || !password || !tenant) {
    const missing = [
      !email && 'BOOTSTRAP_SUPERADMIN_EMAIL',
      !password && 'BOOTSTRAP_SUPERADMIN_PASSWORD',
      !tenant && 'BOOTSTRAP_TENANT_ID',
    ].filter(Boolean).join(', ')
    const reason = `missing required env vars: ${missing}`
    log.warn(`seed: ${reason}; skipping bootstrap`)
    return { status: 'skipped', reason }
  }

  const userCount = await users.count()
  if (userCount > 0) {
    log.info(`seed: ${userCount} user(s) already exist; skipping idempotent bootstrap`)
    return { status: 'already_exists', userCount }
  }

  // AuthService.bootstrapSuperadmin only uses the user/refresh repos and
  // never invokes the token signer (no login flow during bootstrap), so a
  // no-op signer keeps the wiring minimal.
  const signer = { sign: () => '', verify: () => null }
  const auth = new AuthService(users, users, signer)
  await auth.bootstrapSuperadmin(email, name, password, tenant)
  log.info(`seed: superadmin ${email} bootstrapped under tenant ${tenant}`)
  return { status: 'bootstrapped', email, tenantId: tenant }
}

async function main (): Promise<void> {
  const prisma = new PrismaClient()
  try {
    const users = new PrismaUserStore(prisma)
    await bootstrapSuperadminFromEnv(users, process.env as SeedEnv, console)
  } finally {
    await prisma.$disconnect()
  }
}

// Auto-run only when invoked directly via `prisma db seed`. Importing the
// helpers in tests MUST NOT trigger Prisma connect.
//
// `import.meta.url` check handles both the ESM build (NodeNext) and the
// CJS test runtime. tsx runs the script directly via `process.argv[1]`.
const invokedDirectly =
  typeof require !== 'undefined' && require.main === module
const cwdInvocation =
  typeof process !== 'undefined' &&
  process.argv[1] !== undefined &&
  process.argv[1].endsWith('seed.ts')

if (invokedDirectly || cwdInvocation) {
  main().catch((err: unknown) => {
    // eslint-disable-next-line no-console
    console.error('seed: bootstrap failed:', err)
    process.exit(1)
  })
}
