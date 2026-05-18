import fp from 'fastify-plugin'
import fjwt from '@fastify/jwt'
import type { FastifyInstance, FastifyRequest, FastifyReply } from 'fastify'

import { DomainError } from '../common/errors/domain'

/**
 * JWT auth plugin.
 *
 * Production (`NODE_ENV=production`): RS256 only. `JWT_PUBLIC_KEY` must be a
 * PEM-encoded public key; the server refuses to boot otherwise.
 *
 * Non-production: falls back to HS256 with `JWT_SECRET` (>= 32 chars) for
 * local dev and the existing test fixtures. The phase-09 §3 rewrite ELIMINATED
 * the prior hardcoded constant-string fallback (the CI grep guardrail in
 * `test/plugins/auth-jwt-boot.test.ts` enforces it never reappears).
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  const publicKey = process.env.JWT_PUBLIC_KEY
  const sharedSecret = process.env.JWT_SECRET
  const isProd = process.env.NODE_ENV === 'production'

  if (publicKey && publicKey.trim().length > 0) {
    await fastify.register(fjwt, {
      secret: { public: publicKey },
      verify: { algorithms: ['RS256'] },
    })
  } else if (!isProd && sharedSecret && sharedSecret.length >= 32) {
    fastify.log.warn('JWT running in HS256 dev fallback. Set JWT_PUBLIC_KEY for production.')
    await fastify.register(fjwt, { secret: sharedSecret })
  } else {
    throw new Error(
      'JWT plugin: production requires JWT_PUBLIC_KEY (RS256). '
      + 'In non-production set JWT_SECRET to a 32+ char shared secret.'
    )
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
