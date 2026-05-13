import fp from 'fastify-plugin'
import type { FastifyInstance } from 'fastify'

/**
 * In-process metrics registry (phase-08 §7.17).
 *
 * Lives in-memory; the Prisma + Postgres swap can promote to a real
 * histogram (`prom-client`) later. The contract is the exposition shape,
 * not the storage. Push/pull paths emit timings; the conflicts service
 * bumps the counter.
 */
export interface MetricsRegistry {
  observeSyncPush (durationMs: number, status: 'ok' | 'fail'): void
  observeSyncPull (durationMs: number, status: 'ok' | 'fail'): void
  incrConflict (): void
  observeAuditQuery (durationMs: number): void
  setOutboxDepth (tenantId: string, depth: number): void
  expose (): string
}

class InMemoryMetrics implements MetricsRegistry {
  private pushDurations: number[] = []
  private pushFails = 0
  private pullDurations: number[] = []
  private pullFails = 0
  private conflicts = 0
  private auditDurations: number[] = []
  private outbox = new Map<string, number>()

  observeSyncPush (durationMs: number, status: 'ok' | 'fail'): void {
    this.pushDurations.push(durationMs)
    if (status === 'fail') this.pushFails += 1
    if (this.pushDurations.length > 1024) this.pushDurations.shift()
  }

  observeSyncPull (durationMs: number, status: 'ok' | 'fail'): void {
    this.pullDurations.push(durationMs)
    if (status === 'fail') this.pullFails += 1
    if (this.pullDurations.length > 1024) this.pullDurations.shift()
  }

  incrConflict (): void {
    this.conflicts += 1
  }

  observeAuditQuery (durationMs: number): void {
    this.auditDurations.push(durationMs)
    if (this.auditDurations.length > 1024) this.auditDurations.shift()
  }

  setOutboxDepth (tenantId: string, depth: number): void {
    this.outbox.set(tenantId, depth)
  }

  expose (): string {
    const lines: string[] = []
    lines.push('# HELP sync_push_duration_seconds_count Sync push observations.')
    lines.push('# TYPE sync_push_duration_seconds_count counter')
    lines.push(`sync_push_duration_seconds_count ${this.pushDurations.length}`)
    lines.push('# HELP sync_push_duration_seconds_sum Total seconds spent in push.')
    lines.push('# TYPE sync_push_duration_seconds_sum counter')
    lines.push(`sync_push_duration_seconds_sum ${(sum(this.pushDurations) / 1000).toFixed(3)}`)
    lines.push('# HELP sync_push_fail_total Failed push attempts.')
    lines.push('# TYPE sync_push_fail_total counter')
    lines.push(`sync_push_fail_total ${this.pushFails}`)

    lines.push('# HELP sync_pull_duration_seconds_count Sync pull observations.')
    lines.push('# TYPE sync_pull_duration_seconds_count counter')
    lines.push(`sync_pull_duration_seconds_count ${this.pullDurations.length}`)
    lines.push('# HELP sync_pull_duration_seconds_sum Total seconds spent in pull.')
    lines.push('# TYPE sync_pull_duration_seconds_sum counter')
    lines.push(`sync_pull_duration_seconds_sum ${(sum(this.pullDurations) / 1000).toFixed(3)}`)
    lines.push('# HELP sync_pull_fail_total Failed pull attempts.')
    lines.push('# TYPE sync_pull_fail_total counter')
    lines.push(`sync_pull_fail_total ${this.pullFails}`)

    lines.push('# HELP sync_conflict_total Total parked conflicts.')
    lines.push('# TYPE sync_conflict_total counter')
    lines.push(`sync_conflict_total ${this.conflicts}`)

    lines.push('# HELP audit_query_duration_seconds_count Audit query observations.')
    lines.push('# TYPE audit_query_duration_seconds_count counter')
    lines.push(`audit_query_duration_seconds_count ${this.auditDurations.length}`)
    lines.push('# HELP audit_query_duration_seconds_sum Total seconds in audit queries.')
    lines.push('# TYPE audit_query_duration_seconds_sum counter')
    lines.push(`audit_query_duration_seconds_sum ${(sum(this.auditDurations) / 1000).toFixed(3)}`)

    lines.push('# HELP outbox_depth_gauge Pending outbox depth per tenant.')
    lines.push('# TYPE outbox_depth_gauge gauge')
    for (const [tenant, depth] of this.outbox.entries()) {
      lines.push(`outbox_depth_gauge{tenant="${escapeLabel(tenant)}"} ${depth}`)
    }
    return lines.join('\n') + '\n'
  }
}

function sum (xs: number[]): number {
  return xs.reduce((a, b) => a + b, 0)
}

function escapeLabel (s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n')
}

export default fp(async (fastify: FastifyInstance) => {
  const registry = new InMemoryMetrics()
  fastify.decorate('metricsRegistry', registry)
})

declare module 'fastify' {
  interface FastifyInstance {
    metricsRegistry: MetricsRegistry
  }
}
