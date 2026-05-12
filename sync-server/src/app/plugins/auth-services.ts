import fp from 'fastify-plugin'
import type { FastifyInstance } from 'fastify'

import { AuthService, type TokenSigner } from '../auth/service/auth-service'
import { MemoryUserStore } from '../auth/infrastructure/memory-user-store'

async function plugin (fastify: FastifyInstance): Promise<void> {
  const userStore = new MemoryUserStore()
  const signer: TokenSigner = {
    sign (payload, ttlSec) {
      // The @fastify/jwt declaration is locked to our payload shape; assert
      // that the dynamic claim map is structurally compatible.
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
  const auth = new AuthService(userStore, userStore, signer)

  // Optional bootstrap from env (Phase-02 §7.21).
  const bootEmail = process.env.BOOTSTRAP_SUPERADMIN_EMAIL
  const bootPassword = process.env.BOOTSTRAP_SUPERADMIN_PASSWORD
  const bootTenant = process.env.BOOTSTRAP_TENANT_ID
  if (bootEmail && bootPassword && bootTenant) {
    auth
      .bootstrapSuperadmin(bootEmail, 'Bootstrap Admin', bootPassword, bootTenant)
      .catch(() => undefined)
  }

  fastify.decorate('authService', auth)
  fastify.decorate('userStore', userStore)
}

export default fp(plugin, { name: 'auth-services', dependencies: ['auth-jwt'] })

declare module 'fastify' {
  interface FastifyInstance {
    authService: AuthService
    userStore: MemoryUserStore
  }
}
