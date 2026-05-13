import type { FastifyPluginAsync } from 'fastify'

/**
 * GET /metrics -- Prometheus exposition format (phase-08 §7.17).
 *
 * Gated by `X-Internal-Token` header. The shared secret is taken from the
 * `METRICS_TOKEN` env var; if unset, the endpoint refuses every request
 * with 404 to avoid leaking metrics shape. No JWT (Prometheus scrapers
 * don't speak it).
 */
const route: FastifyPluginAsync = async (fastify) => {
  fastify.get('/metrics', {
    schema: {
      tags: ['health'],
      summary: 'Prometheus metrics scrape endpoint',
      description: `Internal-only. Requires the \`X-Internal-Token\`
header to match the \`METRICS_TOKEN\` env var; when the var is unset the
endpoint returns 404 (no JWT either).`,
      hide: true,
    },
  }, async (request, reply) => {
    const expected = process.env.METRICS_TOKEN
    if (!expected || expected.length === 0) {
      void reply.status(404).send({ code: 'NOT_FOUND', message: 'metrics disabled' })
      return
    }
    const supplied = request.headers['x-internal-token']
    if (typeof supplied !== 'string' || supplied !== expected) {
      void reply.status(404).send({ code: 'NOT_FOUND', message: 'metrics disabled' })
      return
    }
    void reply
      .header('Content-Type', 'text/plain; version=0.0.4; charset=utf-8')
      .send(fastify.metricsRegistry.expose())
  })
}

export default route
