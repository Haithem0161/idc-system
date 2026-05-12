import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsync } from 'fastify'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

const LoginBody = Type.Object({
  email: Type.String({ format: 'email' }),
  password: Type.String({ minLength: 8 }),
  entityId: Type.Optional(Type.String()),
  deviceId: Type.Optional(Type.String()),
})

const LoginResponse = Type.Object({
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

const RefreshBody = Type.Object({
  refreshToken: Type.String(),
})

const RefreshResponse = Type.Object({
  accessToken: Type.String(),
  refreshToken: Type.String(),
  expiresAt: Type.String(),
})

const ChangePasswordBody = Type.Object({
  oldPassword: Type.String({ minLength: 1 }),
  newPassword: Type.String({ minLength: 8 }),
})

const BootstrapBody = Type.Object({
  email: Type.String({ format: 'email' }),
  name: Type.String({ minLength: 1 }),
  password: Type.String({ minLength: 8 }),
  entityId: Type.String({ minLength: 1 }),
})

const BootstrapResponse = Type.Object({
  id: Type.String(),
  email: Type.String(),
  name: Type.String(),
  role: Type.String(),
})

const ErrorRef = Type.Ref('ErrorResponse')

const routes: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.post('/auth/login', {
    schema: {
      tags: ['auth'],
      summary: 'Login with email + password',
      body: LoginBody,
      response: { 200: LoginResponse, 401: ErrorRef, 422: ErrorRef, 500: ErrorRef },
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
    schema: {
      tags: ['auth'],
      summary: 'Rotate refresh + access tokens',
      body: RefreshBody,
      response: { 200: RefreshResponse, 401: ErrorRef, 500: ErrorRef },
    },
    handler: async (request) => {
      const deviceId = (request.headers['x-device-id'] as string | undefined) ?? null
      return fastify.authService.refresh(request.body.refreshToken, deviceId)
    },
  })

  app.post('/auth/logout', {
    schema: {
      tags: ['auth'],
      summary: 'Revoke a refresh token',
      body: RefreshBody,
      response: { 204: Type.Null() },
    },
    handler: async (request, reply) => {
      await fastify.authService.logout(request.body.refreshToken)
      return reply.code(204).send(null)
    },
  })

  app.post('/auth/change-password', {
    onRequest: [fastify.authenticate],
    schema: {
      tags: ['auth'],
      summary: 'Change the current user password',
      body: ChangePasswordBody,
      security: [{ bearerAuth: [] }],
      response: { 204: Type.Null(), 401: ErrorRef, 404: ErrorRef, 422: ErrorRef, 500: ErrorRef },
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
        request.body.entityId
      )
      return {
        id: created.id,
        email: created.email,
        name: created.name,
        role: created.role,
      }
    },
  })
}

export default routes
