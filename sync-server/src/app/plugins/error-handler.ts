import fp from 'fastify-plugin'
import { randomUUID } from 'node:crypto'
import type { FastifyError } from 'fastify'

import { DomainError } from '../common/errors/domain'

/**
 * Maps thrown errors onto the canonical `ErrorResponseSchema` (phase-01
 * §7.26). Domain errors carry their own status + code; Fastify validation
 * errors map to 422; everything else degrades to 500.
 */
export default fp(async (fastify) => {
  fastify.setErrorHandler((err: FastifyError, request, reply) => {
    const traceId = (request.headers['x-request-id'] as string | undefined) ?? randomUUID()
    request.log.error({ err, traceId }, 'request failed')

    if (err instanceof DomainError) {
      void reply.status(err.status).send({
        code: err.code,
        message: err.message,
        details: err.details,
        traceId,
      })
      return
    }

    if (err.validation) {
      void reply.status(422).send({
        code: 'VALIDATION_ERROR',
        message: err.message,
        details: { issues: err.validation },
        traceId,
      })
      return
    }

    if (err.code && typeof err.code === 'string' && err.code.startsWith('FAST_JWT_')) {
      void reply.status(401).send({
        code: 'SESSION_EXPIRED',
        message: err.message,
        traceId,
      })
      return
    }

    void reply.status(err.statusCode ?? 500).send({
      code: 'INTERNAL_ERROR',
      message: err.message ?? 'internal server error',
      traceId,
    })
  })
})
