import fp from 'fastify-plugin'
import fjwt from '@fastify/jwt'
import type { FastifyInstance, FastifyRequest, FastifyReply } from 'fastify'

import { DomainError } from '../common/errors/domain'

/**
 * JWT auth plugin.
 *
 * Production (`NODE_ENV=production`): RS256. The server SIGNS access/refresh
 * tokens with `JWT_PRIVATE_KEY` and VERIFIES them with `JWT_PUBLIC_KEY` (both
 * PEM-encoded). The desktop app verifies offline with the bundled public key.
 * `JWT_PUBLIC_KEY` is mandatory; `JWT_PRIVATE_KEY` is mandatory for the server
 * to issue tokens at all -- without it `/auth/login` cannot sign. The server
 * refuses to boot if the public key is missing.
 *
 * When only `JWT_PUBLIC_KEY` is set (no private key), the plugin registers in
 * VERIFY-ONLY mode -- useful for a future read-only replica, but `sign()` will
 * throw. We log a warning so this is never a silent surprise in production.
 *
 * Non-production: falls back to HS256 with `JWT_SECRET` (>= 32 chars) for
 * local dev and the existing test fixtures. The phase-09 §3 rewrite ELIMINATED
 * the prior hardcoded constant-string fallback (the CI grep guardrail in
 * `test/plugins/auth-jwt-boot.test.ts` enforces it never reappears).
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  const publicKey = process.env.JWT_PUBLIC_KEY
  const privateKey = process.env.JWT_PRIVATE_KEY
  const sharedSecret = process.env.JWT_SECRET
  // Phase-10 T7: the HS256 dev fallback is allowed ONLY in an explicit dev/test
  // environment. Previously this keyed off `NODE_ENV === 'production'`, so a
  // `staging`/unset/typo NODE_ENV with JWT_SECRET set would silently sign HS256
  // while clients pin the RS256 public key -- a token-forge trap. Treat
  // anything that is not explicitly dev/test as production-strict (RS256 only).
  const nodeEnv = process.env.NODE_ENV
  const isDevLike = nodeEnv === 'development' || nodeEnv === 'test' || nodeEnv === undefined || nodeEnv === ''

  if (publicKey && publicKey.trim().length > 0) {
    if (privateKey && privateKey.trim().length > 0) {
      // Full RS256: sign with the private key, verify with the public key.
      await fastify.register(fjwt, {
        secret: { private: privateKey, public: publicKey },
        sign: { algorithm: 'RS256' },
        verify: { algorithms: ['RS256'] },
      })
    } else {
      // Verify-only: the server can validate tokens but NOT issue them. Any
      // token-minting route (login/refresh) will throw on sign().
      fastify.log.warn(
        'JWT registered in VERIFY-ONLY mode (no JWT_PRIVATE_KEY). '
        + 'Token issuance (login/refresh) will fail. Set JWT_PRIVATE_KEY to sign.'
      )
      await fastify.register(fjwt, {
        secret: { public: publicKey },
        verify: { algorithms: ['RS256'] },
      })
    }
  } else if (isDevLike && sharedSecret && sharedSecret.length >= 32) {
    fastify.log.warn('JWT running in HS256 dev fallback. Set JWT_PUBLIC_KEY for production.')
    await fastify.register(fjwt, { secret: sharedSecret })
  } else {
    throw new Error(
      'JWT plugin: any non-dev environment requires JWT_PUBLIC_KEY + JWT_PRIVATE_KEY (RS256). '
      + `The HS256 JWT_SECRET fallback is permitted only when NODE_ENV is development or test (got '${nodeEnv ?? '<unset>'}'). `
      + 'Set the RS256 keys, or set NODE_ENV=development for local dev.'
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
