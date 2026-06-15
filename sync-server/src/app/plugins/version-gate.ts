import fp from 'fastify-plugin'
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify'

/**
 * Client app-version and sync-schema-version gate (offline-first rule: the
 * server may reject incompatible clients with 426; the desktop UI prompts for
 * upgrade).
 *
 * Every sync request carries `X-App-Version` and `X-Schema-Version`. Two
 * independent gates run:
 *
 * - `MIN_CLIENT_VERSION` vs `X-App-Version` (dotted semver compare). Guards
 *   against running an app build older than the server supports.
 * - `MIN_CLIENT_SCHEMA_VERSION` vs `X-Schema-Version` (integer migration count).
 *   Guards against silent data loss when a server migration adds a required
 *   field: an old client whose local schema predates it is told to upgrade
 *   rather than pushing payloads missing the new column (phase-10 T3).
 *
 * A missing/unparseable header is allowed through (fail-open) so a misconfigured
 * or older client never bricks; only a parseable value genuinely below the
 * minimum is rejected. The 426 body carries `reason` so the client can tell the
 * two gates apart.
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

/** Parse a non-negative integer schema version, or null if absent/garbage. */
function parseSchemaVersion (v: string | undefined): number | null {
  if (typeof v !== 'string' || !/^\d+$/.test(v.trim())) return null
  return Number.parseInt(v.trim(), 10)
}

async function plugin (fastify: FastifyInstance): Promise<void> {
  const minVersion = (fastify.config.MIN_CLIENT_VERSION ?? '').trim()
  const minSchemaRaw = (fastify.config.MIN_CLIENT_SCHEMA_VERSION ?? '').trim()
  const minSchema = parseSchemaVersion(minSchemaRaw)
  if (!minVersion && minSchema === null) return // no gate configured

  fastify.addHook('onRequest', async (request: FastifyRequest, reply: FastifyReply) => {
    // Only gate the sync surface; auth/login must stay reachable so a stale
    // client can still authenticate and learn it must upgrade.
    if (!request.url.startsWith('/sync/')) return

    // App-version gate.
    if (minVersion) {
      const clientVersion = request.headers['x-app-version'] as string | undefined
      if (parseable(clientVersion) && compareVersions(clientVersion.trim(), minVersion) < 0) {
        void reply.code(426).send({
          code: 'UPGRADE_REQUIRED',
          reason: 'app_version',
          message: `client version ${clientVersion} is below the minimum ${minVersion}`,
          minVersion,
        })
        return
      }
    }

    // Schema-version gate (phase-10 T3). Integer migration count.
    if (minSchema !== null) {
      const clientSchema = parseSchemaVersion(
        request.headers['x-schema-version'] as string | undefined
      )
      // fail-open on a missing/garbage header (older clients predate it).
      if (clientSchema !== null && clientSchema < minSchema) {
        void reply.code(426).send({
          code: 'UPGRADE_REQUIRED',
          reason: 'schema_version',
          message: `client schema version ${clientSchema} is below the minimum ${minSchema}`,
          minSchemaVersion: minSchema,
        })
      }
    }
  })
}

export default fp(plugin, {
  name: 'version-gate',
  dependencies: ['env'],
})
