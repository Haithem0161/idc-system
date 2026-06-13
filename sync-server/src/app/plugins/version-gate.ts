import fp from 'fastify-plugin'
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify'

/**
 * Client app-version gate (offline-first rule: the server may reject
 * incompatible app versions with 426; the desktop UI prompts for upgrade).
 *
 * Every sync request carries `X-App-Version`. When `MIN_CLIENT_VERSION` is set
 * and the client's version is below it, sync routes return 426 Upgrade
 * Required with `{ code: 'UPGRADE_REQUIRED', minVersion }` so the client can
 * surface an upgrade prompt instead of silently failing or hot-retrying.
 *
 * A missing/unparseable client version is allowed through (fail-open) so a
 * misconfigured header never bricks a client; only a parseable version that
 * is genuinely lower than the minimum is rejected.
 */

/** Compare dotted numeric versions. Returns -1/0/1 (a<b / a==b / a>b). */
export function compareVersions (a: string, b: string): number {
  const pa = a.split('.').map((n) => Number.parseInt(n, 10))
  const pb = b.split('.').map((n) => Number.parseInt(n, 10))
  const len = Math.max(pa.length, pb.length)
  for (let i = 0; i < len; i++) {
    const x = Number.isFinite(pa[i]) ? pa[i] : 0
    const y = Number.isFinite(pb[i]) ? pb[i] : 0
    if (x !== y) return x < y ? -1 : 1
  }
  return 0
}

function parseable (v: string | undefined): v is string {
  return typeof v === 'string' && /^\d+(\.\d+)*$/.test(v.trim())
}

async function plugin (fastify: FastifyInstance): Promise<void> {
  const minVersion = (fastify.config.MIN_CLIENT_VERSION ?? '').trim()
  if (!minVersion) return // no gate configured

  fastify.addHook('onRequest', async (request: FastifyRequest, reply: FastifyReply) => {
    // Only gate the sync surface; auth/login must stay reachable so a stale
    // client can still authenticate and learn it must upgrade.
    if (!request.url.startsWith('/sync/')) return
    const clientVersion = request.headers['x-app-version'] as string | undefined
    if (!parseable(clientVersion)) return // fail-open on missing/garbage header
    if (compareVersions(clientVersion.trim(), minVersion) < 0) {
      void reply.code(426).send({
        code: 'UPGRADE_REQUIRED',
        message: `client version ${clientVersion} is below the minimum ${minVersion}`,
        minVersion,
      })
    }
  })
}

export default fp(plugin, {
  name: 'version-gate',
  dependencies: ['env'],
})
