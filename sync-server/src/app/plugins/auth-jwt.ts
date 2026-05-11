import fp from 'fastify-plugin'
import fjwt from '@fastify/jwt'
import type { FastifyInstance, FastifyRequest, FastifyReply } from 'fastify'

import { DomainError } from '../common/errors/domain'

/**
 * JWT auth plugin (RS256).
 *
 * In Phase-1 the server runs in verify-only mode: the public key arrives via
 * `JWT_PUBLIC_KEY` (PEM-encoded). For tests we accept an HS256 fallback so
 * fixtures can mint tokens without an RSA keypair.
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  const publicKey = process.env.JWT_PUBLIC_KEY
  const sharedSecret = process.env.JWT_SECRET ?? 'dev-only-secret'

  if (publicKey && publicKey.trim().length > 0) {
    await fastify.register(fjwt, {
      secret: { public: publicKey },
      verify: { algorithms: ['RS256'] },
    })
  } else {
    // Test / dev fallback: HS256 with a shared secret. Production MUST set
    // JWT_PUBLIC_KEY.
    await fastify.register(fjwt, { secret: sharedSecret })
  }

  fastify.decorate('authenticate', async function (request: FastifyRequest, _reply: FastifyReply) {
    try {
      await request.jwtVerify()
    } catch (err) {
      throw new DomainError(
        'NOT_AUTHENTICATED',
        'authentication required',
        401,
        { reason: (err as Error).message }
      )
    }
  })
}

export default fp(plugin, { name: 'auth-jwt' })

declare module 'fastify' {
  interface FastifyInstance {
    authenticate(request: FastifyRequest, reply: FastifyReply): Promise<void>
  }
}

declare module '@fastify/jwt' {
  interface FastifyJWT {
    payload: {
      sub: string
      email: string
      entityId: string
      role?: string
    }
    user: {
      sub: string
      email: string
      entityId: string
      role?: string
    }
  }
}
