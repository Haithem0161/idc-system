import fp from 'fastify-plugin'

import { MemorySyncStore } from '../sync/infrastructure/memory/store'
import { ConflictResolveService } from '../sync/service/conflict-service'
import { SyncPullService } from '../sync/service/pull-service'
import { SyncPushService } from '../sync/service/push-service'

/**
 * Wires sync services into the Fastify instance.
 *
 * Phase-1 uses an in-memory store so the suite can run without Postgres. The
 * Prisma-backed store will land alongside it and is swapped in via the
 * `SYNC_STORE` env var (`memory` | `prisma`).
 */
export default fp(async (fastify) => {
  const store = new MemorySyncStore()
  const pushService = new SyncPushService(store, store, store)
  const pullService = new SyncPullService(store, store)
  const conflictService = new ConflictResolveService(store)

  fastify.decorate('syncStore', store)
  fastify.decorate('pushService', pushService)
  fastify.decorate('pullService', pullService)
  fastify.decorate('conflictService', conflictService)
})

declare module 'fastify' {
  interface FastifyInstance {
    syncStore: MemorySyncStore
    pushService: SyncPushService
    pullService: SyncPullService
    conflictService: ConflictResolveService
  }
}
