import fp from 'fastify-plugin'
import rateLimit from '@fastify/rate-limit'
import type { FastifyInstance } from 'fastify'

/**
 * Per-IP rate limiting via `@fastify/rate-limit` (per
 * `.claude/rules/sync-server.md` Required Plugins). Until now there was NO
 * throttle on any route -- `/auth/login` allowed unlimited password guessing.
 *
 * Strategy:
 *   - A generous GLOBAL limit so normal multi-device sync is never throttled
 *     (a clinic with a handful of devices stays far under it) while a runaway
 *     or abusive client is still bounded.
 *   - STRICT per-route limits on the abuse-sensitive endpoints are declared in
 *     the route files themselves via `config.rateLimit` (auth login/refresh/
 *     change-password and `/sync/push`). Those override the global numbers.
 *
 * Keyed by `request.ip`. When the server sits behind nginx, set
 * `trustProxy` on the Fastify instance (deployment concern) so `request.ip`
 * reflects the real client via `X-Forwarded-For` rather than the proxy.
 *
 * In-memory store (single-instance VPS deployment). If the server is ever
 * horizontally scaled, pass the shared ioredis instance via `redis` so the
 * window is consistent across replicas.
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  await fastify.register(rateLimit, {
    global: true,
    // Generous default: ample headroom for legitimate sync polling + pushes
    // from every device in a clinic, far below what an abusive loop would hit.
    max: 300,
    timeWindow: '1 minute',
    // Standardized envelope to match the server's error shape.
    errorResponseBuilder (_request, context) {
      return {
        success: false,
        error: {
          code: 'RATE_LIMITED',
          message: `Too many requests. Retry in ${Math.ceil(context.ttl / 1000)}s.`,
        },
      }
    },
  })
}

export default fp(plugin, { name: 'rate-limit' })
