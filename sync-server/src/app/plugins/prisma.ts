import fp from 'fastify-plugin'
import { PrismaClient } from '@prisma/client'
import { PrismaPg } from '@prisma/adapter-pg'
import type { FastifyInstance } from 'fastify'

/**
 * Singleton `PrismaClient` decorator + lifecycle hook.
 *
 * Behaviour:
 *   - When `DATABASE_URL` is set the plugin constructs a client, connects
 *     eagerly so a misconfiguration fails boot rather than the first request,
 *     and disconnects on `onClose`.
 *   - When `DATABASE_URL` is NOT set the plugin is a no-op. Downstream service
 *     plugins detect the missing decorator and fall back to the in-memory
 *     store (test bootstrap path).
 *
 * Authored per phase-09 §3 Sync Server (Plugin wiring).
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  if (!fastify.appEnv.databaseUrl) {
    fastify.log.info('prisma plugin: no DATABASE_URL set; skipping Prisma client construction')
    return
  }

  const adapter = new PrismaPg({ connectionString: fastify.appEnv.databaseUrl })
  const prisma = new PrismaClient({
    adapter,
    log: fastify.appEnv.nodeEnv === 'development' ? ['warn', 'error'] : ['error'],
  })

  try {
    // $connect lazily defers actual DB I/O in Prisma 7 with the pg adapter.
    // Force a round-trip so a stale `.env` pointing at a dead host fails
    // here, not on first request.
    await prisma.$queryRaw`SELECT 1`
  } catch (err) {
    if (fastify.appEnv.isProduction) {
      fastify.log.error({ err }, 'prisma plugin: connection probe failed in production')
      throw err
    }
    // Non-production: a stale .env can point DATABASE_URL at a host that
    // isn't running. Skip Prisma wiring so the sync-services plugin falls
    // back to the in-memory store. Production refuses to boot per
    // phase-09 §3 (env plugin enforces DATABASE_URL there).
    fastify.log.warn({ err }, 'prisma plugin: connection probe failed; falling back to memory store')
    await prisma.$disconnect().catch(() => undefined)
    return
  }

  fastify.decorate('prisma', prisma)
  fastify.addHook('onClose', async () => {
    await prisma.$disconnect()
  })
}

export default fp(plugin, {
  name: 'prisma',
  dependencies: ['env'],
})

declare module 'fastify' {
  interface FastifyInstance {
    prisma?: PrismaClient
  }
}
