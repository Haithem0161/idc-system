import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

// Phase-09 §3.1 contract slice: schemas exported so the Ajv-equivalent
// (`Value.Check`) harness can drift-test the wire shape without
// re-declaring it. Mirror of the conflicts.ts / healthz.ts pattern.
export const LoginBody = Type.Object({
  email: Type.String({ format: 'email' }),
  password: Type.String({ minLength: 8 }),
  entityId: Type.Optional(Type.String()),
  deviceId: Type.Optional(Type.String()),
})

export const LoginResponse = Type.Object({
  accessToken: Type.String(),
  refreshToken: Type.String(),
  expiresAt: Type.String(),
  user: Type.Object({
    id: Type.String(),
    email: Type.String(),
    name: Type.String(),
    role: Type.Union([
      Type.Literal('superadmin'),
      Type.Literal('receptionist'),
      Type.Literal('accountant'),
    ]),
    entityId: Type.String(),
    passwordHash: Type.String(),
  }),
})

export const RefreshBody = Type.Object({
  refreshToken: Type.String(),
})

export const RefreshResponse = Type.Object({
  accessToken: Type.String(),
  refreshToken: Type.String(),
  expiresAt: Type.String(),
})

export const ProfileResponse = Type.Object({
  id: Type.String(),
  email: Type.String(),
  name: Type.String(),
  role: Type.Union([
    Type.Literal('superadmin'),
    Type.Literal('receptionist'),
    Type.Literal('accountant'),
  ]),
  entityId: Type.String(),
})

export const ChangePasswordBody = Type.Object({
  oldPassword: Type.String({ minLength: 1 }),
  newPassword: Type.String({ minLength: 8 }),
})

export const BootstrapBody = Type.Object({
  id: Type.Optional(Type.String({ format: 'uuid' })),
  email: Type.String({ format: 'email' }),
  name: Type.String({ minLength: 1 }),
  password: Type.String({ minLength: 8 }),
  // Optional: the SERVER is authoritative for tenancy. When the client omits
  // entityId (the single-clinic desktop flow), the server stamps the admin with
  // its configured DEFAULT_ENTITY_ID. A multi-tenant caller may still pass one.
  entityId: Type.Optional(Type.String({ minLength: 1 })),
})

export const BootstrapResponse = Type.Object({
  id: Type.String(),
  email: Type.String(),
  name: Type.String(),
  role: Type.String(),
  // Echo the entityId the server actually assigned so the client can persist
  // the row under the correct tenant scope without having chosen it.
  entityId: Type.String(),
})

// Public read-only probe: does this clinic already have a user (superadmin)?
// A fresh desktop machine calls this AFTER setting the sync URL to decide
// whether to show "create first admin" (initialized=false) or jump to login
// (initialized=true). No auth -- a brand-new machine has no token yet.
export const BootstrapStatusResponse = Type.Object({
  initialized: Type.Boolean(),
})

/// DEF-007 G08: client-side RS256 verifier reads this endpoint at boot
/// to pin the public key in stronghold. Body is the PEM-encoded RSA
/// public key. Returns 404 when `JWT_PUBLIC_KEY` is unset (HS256 dev
/// mode; client must fall back to trusting the dev secret out-of-band).
export const PublicKeyResponse = Type.String()

const ErrorRef = Type.Ref('ErrorResponse')

const routes: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  /**
   * Phase-10 T5: best-effort subject binding for refresh/logout. The refresh
   * token is the credential, but when the caller also presents an access token
   * we verify its `sub` and pass it down so the rotation/revocation is scoped
   * to that subject. We accept an EXPIRED access token (`ignoreExpiration`)
   * because refresh is exactly the moment the access token has lapsed -- the
   * signature is what binds identity, not freshness. A missing or
   * signature-invalid bearer yields `undefined` (offline-first refresh still
   * works); a present-but-mismatching token is caught in the store (403).
   */
  async function subjectFromBearer (
    request: { headers: Record<string, unknown> }
  ): Promise<string | undefined> {
    const header = request.headers.authorization
    if (typeof header !== 'string' || !header.toLowerCase().startsWith('bearer ')) {
      return undefined
    }
    const token = header.slice('bearer '.length).trim()
    try {
      const claims = fastify.jwt.verify(token, { ignoreExpiration: true }) as { sub?: string }
      return typeof claims.sub === 'string' ? claims.sub : undefined
    } catch {
      return undefined
    }
  }

  app.post('/auth/login', {
    // Strict throttle: blunt credential-stuffing / password-guessing. argon2 is
    // slow, but without a per-IP cap an attacker can still grind. 5/min/IP.
    config: { rateLimit: { max: 5, timeWindow: '1 minute' } },
    schema: {
      tags: ['auth'],
      summary: 'Login with email + password',
      body: LoginBody,
      response: { 200: LoginResponse, 401: ErrorRef, 422: ErrorRef, 429: ErrorRef, 500: ErrorRef },
    },
    handler: async (request) => {
      const entityId = request.body.entityId ?? 'unscoped'
      const deviceId = request.body.deviceId
        ?? ((request.headers['x-device-id'] as string | undefined) ?? null)
      return fastify.authService.login(
        request.body.email,
        request.body.password,
        entityId,
        deviceId
      )
    },
  })

  app.post('/auth/refresh', {
    // Bounded: a legitimate device refreshes at most every ~15m, so 20/min/IP
    // is generous headroom while still capping refresh-token brute force.
    config: { rateLimit: { max: 20, timeWindow: '1 minute' } },
    schema: {
      tags: ['auth'],
      summary: 'Rotate refresh + access tokens',
      description: `Rotates the presented refresh token. When an \`Authorization: Bearer\` access token is also sent, its \`sub\` is verified (ignoring expiry) and the rotation is bound to that subject -- a refresh token that does not belong to the bearer is rejected with 403 (phase-10 T5). The access token is optional so the offline-first refresh path still works when it has lapsed.`,
      body: RefreshBody,
      response: { 200: RefreshResponse, 401: ErrorRef, 403: ErrorRef, 429: ErrorRef, 500: ErrorRef },
    },
    handler: async (request) => {
      const deviceId = (request.headers['x-device-id'] as string | undefined) ?? null
      const expectedUserId = await subjectFromBearer(request)
      return fastify.authService.refresh(request.body.refreshToken, deviceId, expectedUserId)
    },
  })

  app.post('/auth/logout', {
    schema: {
      tags: ['auth'],
      summary: 'Revoke a refresh token',
      description: 'Revokes the presented refresh token. When an `Authorization: Bearer` access token is sent, the revocation is bound to its subject so a leaked token cannot force-logout another user (phase-10 T5).',
      body: RefreshBody,
      response: { 204: Type.Null(), 403: ErrorRef },
    },
    handler: async (request, reply) => {
      const expectedUserId = await subjectFromBearer(request)
      await fastify.authService.logout(request.body.refreshToken, expectedUserId)
      return reply.code(204).send(null)
    },
  })

  app.get('/auth/profile', {
    onRequest: [fastify.authenticate],
    schema: {
      tags: ['auth'],
      summary: 'Return the authenticated user (server-canonical identity)',
      description: 'Returns the server-side record for the JWT subject so the client can confirm the server agrees on the current identity after login/refresh (phase-10 T6). 404 if the user no longer exists or is inactive. Never returns the password hash.',
      security: [{ bearerAuth: [] }],
      response: { 200: ProfileResponse, 401: ErrorRef, 404: ErrorRef, 500: ErrorRef },
    },
    handler: async (request, reply) => {
      const userId = (request.user as { sub?: string } | undefined)?.sub
      if (!userId) {
        return reply.code(401).send({
          code: 'NOT_AUTHENTICATED',
          message: 'no user in token',
          traceId: 'n/a',
        })
      }
      return fastify.authService.getProfile(userId)
    },
  })

  app.post('/auth/change-password', {
    onRequest: [fastify.authenticate],
    // Authenticated, but throttle anyway to blunt online guessing of the
    // CURRENT password (the change requires it). 10/min/IP.
    config: { rateLimit: { max: 10, timeWindow: '1 minute' } },
    schema: {
      tags: ['auth'],
      summary: 'Change the current user password',
      body: ChangePasswordBody,
      security: [{ bearerAuth: [] }],
      response: { 204: Type.Null(), 401: ErrorRef, 404: ErrorRef, 422: ErrorRef, 429: ErrorRef, 500: ErrorRef },
    },
    handler: async (request, reply) => {
      const userId = (request.user as { sub?: string } | undefined)?.sub
      if (!userId) {
        return reply.code(401).send({
          code: 'NOT_AUTHENTICATED',
          message: 'no user in token',
          traceId: 'n/a',
        })
      }
      await fastify.authService.changePassword(
        userId,
        request.body.oldPassword,
        request.body.newPassword
      )
      return reply.code(204).send(null)
    },
  })

  app.get('/auth/public-key', {
    schema: {
      tags: ['auth'],
      summary: 'Return the JWT signing public key (PEM)',
      response: { 200: PublicKeyResponse, 404: ErrorRef },
    },
    handler: async (_request, reply) => {
      const pem = process.env.JWT_PUBLIC_KEY ?? ''
      if (pem.trim().length === 0) {
        return reply.code(404).send({
          code: 'NOT_FOUND',
          message: 'JWT_PUBLIC_KEY not configured (HS256 dev mode)',
          traceId: 'n/a',
        })
      }
      reply.type('application/x-pem-file')
      return pem
    },
  })

  app.get('/auth/bootstrap-status', {
    schema: {
      tags: ['auth'],
      summary: 'Whether this clinic already has a user (drives desktop first-launch)',
      description:
        'Public read-only probe. Returns `{ initialized: true }` once any user ' +
        'exists, so a fresh desktop machine knows to go straight to login instead ' +
        'of offering to create a first administrator. No authentication required.',
      response: { 200: BootstrapStatusResponse, 500: ErrorRef },
    },
    handler: async () => {
      const initialized = await fastify.authService.isInitialized()
      return { initialized }
    },
  })

  app.post('/auth/bootstrap-superadmin', {
    schema: {
      tags: ['auth'],
      summary: 'Bootstrap the first superadmin (idempotent: errors if any user exists)',
      body: BootstrapBody,
      response: { 200: BootstrapResponse, 409: ErrorRef, 422: ErrorRef, 500: ErrorRef },
    },
    handler: async (request) => {
      const created = await fastify.authService.bootstrapSuperadmin(
        request.body.email,
        request.body.name,
        request.body.password,
        request.body.entityId,
        request.body.id
      )
      return {
        id: created.id,
        email: created.email,
        name: created.name,
        role: created.role,
        entityId: created.entityId,
      }
    },
  })
}

export default routes
