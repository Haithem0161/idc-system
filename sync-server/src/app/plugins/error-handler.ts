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

    // @fastify/rate-limit throws a 429 (or 403 ban) that reaches this custom
    // handler. Without this branch the fall-through emitted 500 because the
    // thrown error's body carried no recognised shape. Preserve the real status
    // and surface Retry-After so clients back off correctly.
    if (err.statusCode === 429) {
      const ttlMs = (err as FastifyError & { ttl?: number }).ttl
      if (typeof ttlMs === 'number') {
        void reply.header('Retry-After', Math.ceil(ttlMs / 1000))
      }
      void reply.status(429).send({
        code: 'RATE_LIMITED',
        message: err.message ?? 'Too many requests.',
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
