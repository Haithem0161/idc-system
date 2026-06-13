import fp from 'fastify-plugin'
import type { FastifyInstance } from 'fastify'

import { AuthService, type TokenSigner } from '../auth/service/auth-service'
import { MemoryUserStore } from '../auth/infrastructure/memory-user-store'
import { PrismaUserStore } from '../auth/infrastructure/prisma/user-store'
import type {
  RefreshTokenRepository,
  UserRepository,
} from '../auth/domain/repositories'

/**
 * Wires the auth service.
 *
 * Production path: Prisma-backed `PrismaUserStore` against the `users` and
 * `refresh_tokens` tables.
 *
 * Test path (no `DATABASE_URL`): falls back to `MemoryUserStore` so the
 * existing test fixtures keep running without Postgres.
 *
 * Authored per phase-09 §3 Sync Server (auth-services rewrite) and §4
 * (Refresh-token persistence semantics).
 */
function parsePositiveInt (raw: string | undefined, fallback: number): number {
  const n = Number.parseInt((raw ?? '').trim(), 10)
  return Number.isFinite(n) && n > 0 ? n : fallback
}

async function plugin (fastify: FastifyInstance): Promise<void> {
  // JWT TTLs from env-validated config. Falls back to the documented defaults
  // (access 15m / refresh 30d) when unset or non-numeric so dev/test without
  // env still behaves. Previously JWT_ACCESS_TTL_SECONDS / JWT_REFRESH_TTL_SECONDS
  // were declared but never read (dead config).
  const accessTtlSec = parsePositiveInt(fastify.config?.JWT_ACCESS_TTL_SECONDS, 15 * 60)
  const refreshTtlSec = parsePositiveInt(
    fastify.config?.JWT_REFRESH_TTL_SECONDS,
    30 * 24 * 60 * 60
  )

  const users: UserRepository & RefreshTokenRepository = fastify.prisma
    ? new PrismaUserStore(fastify.prisma, refreshTtlSec)
    : (fastify.log.warn(
        'auth-services: Prisma client not available; falling back to MemoryUserStore (test/dev only)'
      ), new MemoryUserStore(refreshTtlSec))

  const signer: TokenSigner = {
    sign (payload, ttlSec) {
      return fastify.jwt.sign(
        payload as unknown as { sub: string; email: string; entityId: string; role?: string },
        { expiresIn: ttlSec }
      )
    },
    verify (token) {
      try {
        return fastify.jwt.verify(token) as unknown as Record<string, unknown>
      } catch {
        return null
      }
    },
  }
  const auth = new AuthService(users, users, signer, { accessTtlSec, refreshTtlSec })

  // Optional bootstrap from env (phase-02 §7.21).
  const bootEmail = process.env.BOOTSTRAP_SUPERADMIN_EMAIL
  const bootPassword = process.env.BOOTSTRAP_SUPERADMIN_PASSWORD
  const bootTenant = process.env.BOOTSTRAP_TENANT_ID
  if (bootEmail && bootPassword && bootTenant) {
    auth
      .bootstrapSuperadmin(bootEmail, 'Bootstrap Admin', bootPassword, bootTenant)
      .catch(() => undefined)
  }

  fastify.decorate('authService', auth)
  fastify.decorate('userStore', users)
}

export default fp(plugin, {
  name: 'auth-services',
  dependencies: ['env', 'auth-jwt', 'prisma'],
})

declare module 'fastify' {
  interface FastifyInstance {
    authService: AuthService
    userStore: UserRepository & RefreshTokenRepository
  }
}
