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
async function plugin (fastify: FastifyInstance): Promise<void> {
  const users: UserRepository & RefreshTokenRepository = fastify.prisma
    ? new PrismaUserStore(fastify.prisma)
    : (fastify.log.warn(
        'auth-services: Prisma client not available; falling back to MemoryUserStore (test/dev only)'
      ), new MemoryUserStore())

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
  const auth = new AuthService(users, users, signer)

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
  dependencies: ['auth-jwt', 'prisma'],
})

declare module 'fastify' {
  interface FastifyInstance {
    authService: AuthService
    userStore: UserRepository & RefreshTokenRepository
  }
}
