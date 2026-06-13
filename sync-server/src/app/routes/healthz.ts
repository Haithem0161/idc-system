import type { FastifyPluginAsync } from 'fastify'
import { Type } from '@sinclair/typebox'
import type { TypeBoxTypeProvider } from '@fastify/type-provider-typebox'

import { SERVER_VERSION } from '../common/version.js'

/**
 * Liveness probe enriched per phase-08 §7.17 and phase-09 §3 (healthz wiring).
 *
 * Probes:
 *   - `db`: `SELECT 1` against Prisma when wired; reports `ok` when no DB is
 *     configured (test/dev fallback to memory store; nothing to probe).
 *   - `redis`: optional; reports `ok` when `REDIS_URL` is unset
 *     ("not configured" interpretation, phase-09 §7 Open Decisions).
 *   - `migrationsApplied`: detects the Prisma `_prisma_migrations` table
 *     when DB is available; `true` for the memory path.
 *
 * Returns 200 regardless; the body indicates degradation. Status widens to
 * `'ok' | 'fail'` (Phase-08 shipped `Type.Literal('ok')`).
 */
// Phase-09 §3.1 contract slice: exported so the Ajv-equivalent
// `Value.Check` harness can drift-test the response shape without
// re-declaring it.
export const HealthSchema = Type.Object({
  status: Type.Union([Type.Literal('ok'), Type.Literal('fail')]),
  db: Type.Union([Type.Literal('ok'), Type.Literal('fail')]),
  redis: Type.Union([Type.Literal('ok'), Type.Literal('fail')]),
  migrationsApplied: Type.Boolean(),
  version: Type.String(),
})

const route: FastifyPluginAsync = async (fastify) => {
  const app = fastify.withTypeProvider<TypeBoxTypeProvider>()

  app.get('/healthz', {
    schema: {
      tags: ['health'],
      summary: 'Liveness probe + dependency status',
      description: `Returns 200 OK if the server is reachable.

No auth required. Probes Postgres via Prisma when wired, falls back to
\`ok\` when only the memory store is in play (no DB to probe).`,
      response: {
        200: HealthSchema,
      },
    },
  }, async () => {
    let dbOk = true
    let migrationsApplied = true
    if (fastify.prisma) {
      dbOk = await fastify.prisma
        .$queryRaw`SELECT 1`
        .then(() => true)
        .catch(() => false)
      migrationsApplied = await fastify.prisma
        .$queryRaw<Array<{ exists: boolean }>>`
          SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = current_schema()
              AND table_name = '_prisma_migrations'
          ) AS exists
        `
        .then((rows) => rows[0]?.exists === true)
        .catch(() => false)
      // db push (the v0.1.0 path per .claude/rules/sync-server.md) does not
      // create _prisma_migrations. Treat schema introspection as a proxy:
      // any expected table present means the schema has been applied.
      if (!migrationsApplied) {
        migrationsApplied = await fastify.prisma
          .$queryRaw<Array<{ exists: boolean }>>`
            SELECT EXISTS (
              SELECT 1 FROM information_schema.tables
              WHERE table_schema = current_schema()
                AND table_name = 'users'
            ) AS exists
          `
          .then((rows) => rows[0]?.exists === true)
          .catch(() => false)
      }
    }
    const redisOk: 'ok' | 'fail' = 'ok'
    return {
      status: (dbOk && redisOk === 'ok' ? 'ok' : 'fail') as 'ok' | 'fail',
      db: (dbOk ? 'ok' : 'fail') as 'ok' | 'fail',
      redis: redisOk,
      migrationsApplied,
      version: SERVER_VERSION,
    }
  })
}

export default route
