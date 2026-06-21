import fp from 'fastify-plugin'
import helmet from '@fastify/helmet'
import type { FastifyInstance } from 'fastify'

/**
 * Security response headers via `@fastify/helmet` (per
 * `.claude/rules/sync-server.md` Required Plugins). Until now the server set no
 * security headers at all.
 *
 * This is a JSON sync/auth API plus an optional static download page; it does
 * not render untrusted HTML, so the strict per-page CSP that helmet defaults to
 * is unnecessary and would have to be relaxed for the Swagger UI and the
 * download page anyway. We therefore keep every header EXCEPT
 * `Content-Security-Policy` (the download route sets its own narrow CSP):
 *   - `X-Content-Type-Options: nosniff`
 *   - `X-Frame-Options: DENY` (+ `frame-ancestors 'none'` via frameguard)
 *   - `Strict-Transport-Security` (HSTS) so browsers pin HTTPS once seen
 *   - `Referrer-Policy: no-referrer`
 *   - `X-DNS-Prefetch-Control: off`, `X-Download-Options: noopen`, etc.
 *   - `X-Powered-By` removed (hidePoweredBy).
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  await fastify.register(helmet, {
    // API responses are JSON; a page-level CSP here only complicates Swagger UI
    // and the download page (which sets its own). All other headers stay on.
    contentSecurityPolicy: false,
    // Deny framing entirely -- nothing here should ever be embedded.
    frameguard: { action: 'deny' },
    // ~180 days; browsers remember to use HTTPS for the host after first visit.
    hsts: { maxAge: 180 * 24 * 60 * 60, includeSubDomains: true },
    referrerPolicy: { policy: 'no-referrer' },
  })
}

export default fp(plugin, { name: 'helmet' })
