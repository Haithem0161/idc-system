import fp from 'fastify-plugin'
import type { FastifyInstance } from 'fastify'

import type {
  AuditLogRepository,
  ConflictParkedRepository,
  ProcessedOpRepository,
  SyncCursorRepository,
} from '../sync/domain/repositories'
import type { SyncEntityStore } from '../sync/domain/sync-store'
import { MemorySyncStore } from '../sync/infrastructure/memory/store'
import { PrismaAuditLogRepo } from '../sync/infrastructure/prisma/audit-repo'
import { PrismaConflictParkedRepo } from '../sync/infrastructure/prisma/conflict-parked-repo'
import { PrismaEntityStore } from '../sync/infrastructure/prisma/entity-store'
import { PrismaProcessedOpRepo } from '../sync/infrastructure/prisma/processed-op-repo'
import { PrismaSyncCursorRepo } from '../sync/infrastructure/prisma/sync-cursor-repo'
import { ConflictResolveService } from '../sync/service/conflict-service'
import { SyncPullService } from '../sync/service/pull-service'
import { SyncPushService } from '../sync/service/push-service'

/**
 * Wires sync services into the Fastify instance.
 *
 * Production path: `prisma` plugin decorated `fastify.prisma` from the
 * configured `DATABASE_URL` — we wire a Prisma-backed
 * `(audit, processed, cursor, conflicts, entityStore)` quintet.
 *
 * Test path (no `DATABASE_URL`, prisma plugin no-op): we fall back to
 * `MemorySyncStore` so the existing test suite continues to run without a
 * Postgres dependency. The memory store stays in the tree for that purpose
 * only — production code never instantiates it.
 *
 * Authored per phase-09 §3 Sync Server (Plugin wiring).
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  let auditRepo: AuditLogRepository
  let processedRepo: ProcessedOpRepository
  let cursorRepo: SyncCursorRepository
  let conflictRepo: ConflictParkedRepository
  let entityStore: SyncEntityStore

  if (fastify.prisma) {
    // Production path. MemorySyncStore is NOT instantiated — sync routes
    // run exclusively through Prisma. Phase-09 BLOCKER-3 invariant.
    const prisma = fastify.prisma
    const prismaEntity = new PrismaEntityStore(prisma)
    auditRepo = new PrismaAuditLogRepo(prisma, prismaEntity)
    processedRepo = new PrismaProcessedOpRepo(prisma)
    cursorRepo = new PrismaSyncCursorRepo(prisma)
    conflictRepo = new PrismaConflictParkedRepo(prisma)
    entityStore = prismaEntity
  } else {
    // Test / dev path only. The memory store stays in the tree exclusively
    // for unit + integration tests that don't spin Postgres.
    fastify.log.warn(
      'sync-services: Prisma client not available; falling back to MemorySyncStore (test/dev only)'
    )
    const memoryStore = new MemorySyncStore()
    auditRepo = memoryStore
    processedRepo = memoryStore
    cursorRepo = memoryStore
    conflictRepo = memoryStore
    entityStore = memoryStore
  }

  const pushService = new SyncPushService(auditRepo, conflictRepo, processedRepo, entityStore)
  const pullService = new SyncPullService(auditRepo, cursorRepo)
  // Phase-09 BLOCKER-6: when Prisma is wired, hand the client to the
  // resolver service so the conflict_resolve audit row commits in the
  // SAME $transaction as the resolve write.
  const conflictService = new ConflictResolveService(
    conflictRepo,
    processedRepo,
    auditRepo,
    entityStore,
    fastify.prisma ?? null,
  )

  fastify.decorate('entityStore', entityStore)
  fastify.decorate('auditQueryRepo', auditRepo)
  fastify.decorate('conflictsRepo', conflictRepo)
  fastify.decorate('processedOpRepo', processedRepo)
  fastify.decorate('pushService', pushService)
  fastify.decorate('pullService', pullService)
  fastify.decorate('conflictService', conflictService)
}

export default fp(plugin, {
  name: 'sync-services',
  dependencies: ['prisma'],
})

declare module 'fastify' {
  interface FastifyInstance {
    entityStore: SyncEntityStore
    auditQueryRepo: AuditLogRepository
    conflictsRepo: ConflictParkedRepository
    processedOpRepo: ProcessedOpRepository
    pushService: SyncPushService
    pullService: SyncPullService
    conflictService: ConflictResolveService
  }
}
