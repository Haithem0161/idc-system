// Phase-09 §3.3 canonical wire-snapshot tests.
//
// For each of the 13 wire-shape snapshots under
// `test/expected/snapshots/`, this file asserts:
//   (a) the JSON sample validates against the route's TypeBox schema --
//       any drift on the server-side contract surface (key add/remove/
//       rename/type change) fails here before it can ship.
//   (b) the SHA-256 of the file bytes (trailing newline stripped, matching
//       the BLOCKER-5 healthz pattern) matches the committed `.sha256`
//       sidecar -- any silent edit of the sample without a deliberate
//       hash regeneration fails here.
//
// Together (a) and (b) form a two-way drift gate. Regen workflow:
//   1. Edit `<name>.json`.
//   2. `node -e '...'` recompute and write `<name>.json.sha256`.
//   3. Commit both. Reviewer sees the canonical bytes and the new hash
//      together in the PR diff.

import { test } from 'node:test'
import * as assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { Value } from '@sinclair/typebox/value'
import { FormatRegistry } from '@sinclair/typebox'

import { PushBodySchema } from '../../src/app/sync/presentation/schemas/push'
import { PullResponseSchema } from '../../src/app/sync/presentation/schemas/pull'
import {
  ConflictsListResponseSchema,
  ResolveResponseSchema,
} from '../../src/app/sync/routes/conflicts'
import { AuditQueryResponseSchema } from '../../src/app/domains/audit/routes/audit'
import { ErrorResponseSchema } from '../../src/app/common/schemas/error'

// FormatRegistry mirrors the Ajv-with-formats wiring used at runtime.
// Without these, `Value.Check` treats `format: 'date-time'` / `format: 'uuid'`
// as unknown and rejects valid samples. The audit query response schema's
// row payloads carry `at` as a free-form string (no `format` keyword), but
// keeping the registration prevents drift if a future schema tightens it.
if (!FormatRegistry.Has('date-time')) {
  FormatRegistry.Set('date-time', (value) => {
    if (typeof value !== 'string') return false
    return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/.test(value)
  })
}
if (!FormatRegistry.Has('uuid')) {
  FormatRegistry.Set('uuid', (value) => {
    if (typeof value !== 'string') return false
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)
  })
}

// __dirname is available -- the test tsconfig compiles to CJS.
const snapshotDir = join(__dirname, '..', 'expected', 'snapshots')

function sha256 (input: string): string {
  return createHash('sha256').update(input).digest('hex')
}

function loadSnapshot (basename: string): { raw: string; parsed: unknown; expectedHash: string } {
  const raw = readFileSync(join(snapshotDir, basename), 'utf8').replace(/\n$/, '')
  const expectedHash = readFileSync(join(snapshotDir, `${basename}.sha256`), 'utf8').trim()
  let parsed: unknown
  if (basename.endsWith('.json')) {
    parsed = JSON.parse(raw)
  } else {
    parsed = raw
  }
  return { raw, parsed, expectedHash }
}

// --- PushBody samples ----------------------------------------------------

const PUSH_SAMPLES = [
  'patient-push.json',
  'visit-push-locked.json',
  'visit-push-voided.json',
  'inventory-adjustment-push.json',
  'operator-shift-push.json',
  'operator-shift-soft-delete.json',
] as const

for (const name of PUSH_SAMPLES) {
  test(`${name} validates against PushBodySchema`, () => {
    const { parsed } = loadSnapshot(name)
    const ok = Value.Check(PushBodySchema, parsed)
    if (!ok) {
      const errors = [...Value.Errors(PushBodySchema, parsed)].slice(0, 3)
      assert.fail(`${name} failed PushBodySchema: ${JSON.stringify(errors)}`)
    }
  })

  test(`${name} SHA-256 matches sidecar`, () => {
    const { raw, expectedHash } = loadSnapshot(name)
    assert.equal(sha256(raw), expectedHash, `${name} hash drift`)
  })
}

// --- PullResponse samples ------------------------------------------------

const PULL_SAMPLES = ['visit-pull-row.json', 'operator-shift-pull.json'] as const

for (const name of PULL_SAMPLES) {
  test(`${name} validates against PullResponseSchema`, () => {
    const { parsed } = loadSnapshot(name)
    const ok = Value.Check(PullResponseSchema, parsed)
    if (!ok) {
      const errors = [...Value.Errors(PullResponseSchema, parsed)].slice(0, 3)
      assert.fail(`${name} failed PullResponseSchema: ${JSON.stringify(errors)}`)
    }
  })

  test(`${name} SHA-256 matches sidecar`, () => {
    const { raw, expectedHash } = loadSnapshot(name)
    assert.equal(sha256(raw), expectedHash, `${name} hash drift`)
  })
}

// --- Audit query response (mixed 50-row) ---------------------------------

test('audit-query-response-mixed-50-row.json validates against AuditQueryResponseSchema', () => {
  const { parsed } = loadSnapshot('audit-query-response-mixed-50-row.json')
  const ok = Value.Check(AuditQueryResponseSchema, parsed)
  if (!ok) {
    const errors = [...Value.Errors(AuditQueryResponseSchema, parsed)].slice(0, 3)
    assert.fail(`schema mismatch: ${JSON.stringify(errors)}`)
  }
  const payload = parsed as { rows: unknown[]; next_cursor: string | null }
  assert.equal(payload.rows.length, 50, 'sample must carry exactly 50 rows (mixed-50 invariant)')
})

test('audit-query-response-mixed-50-row.json exercises all 14 action values', () => {
  const { parsed } = loadSnapshot('audit-query-response-mixed-50-row.json')
  const rows = (parsed as { rows: Array<{ action: string }> }).rows
  const distinctActions = new Set(rows.map((r) => r.action))
  // The generator cycles through all 14 actions modulo row index; in 50 rows
  // every action appears at least 3 times. A future tightening of the
  // action enum would still keep this invariant -- only an enum shrink
  // would fail. That's exactly the drift signal we want.
  assert.equal(distinctActions.size, 14, `expected 14 distinct actions, got ${distinctActions.size}`)
})

test('audit-query-response-mixed-50-row.json exercises ip tri-state (string | null)', () => {
  const { parsed } = loadSnapshot('audit-query-response-mixed-50-row.json')
  const rows = (parsed as { rows: Array<{ ip: string | null }> }).rows
  const hasNull = rows.some((r) => r.ip === null)
  const hasString = rows.some((r) => typeof r.ip === 'string')
  assert.equal(hasNull, true, 'at least one row must carry ip=null')
  assert.equal(hasString, true, 'at least one row must carry ip=string')
})

test('audit-query-response-mixed-50-row.json SHA-256 matches sidecar', () => {
  const { raw, expectedHash } = loadSnapshot('audit-query-response-mixed-50-row.json')
  assert.equal(sha256(raw), expectedHash, 'audit-query-response-mixed-50-row.json hash drift')
})

// --- Conflict-list response ---------------------------------------------

test('conflict-list-response-canonical.json validates against ConflictsListResponseSchema', () => {
  const { parsed } = loadSnapshot('conflict-list-response-canonical.json')
  const ok = Value.Check(ConflictsListResponseSchema, parsed)
  if (!ok) {
    const errors = [...Value.Errors(ConflictsListResponseSchema, parsed)].slice(0, 3)
    assert.fail(`schema mismatch: ${JSON.stringify(errors)}`)
  }
})

test('conflict-list-response-canonical.json carries two open conflicts with resolved_at null', () => {
  const { parsed } = loadSnapshot('conflict-list-response-canonical.json')
  const conflicts = (parsed as { conflicts: Array<{ resolved_at: string | null }> }).conflicts
  assert.equal(conflicts.length, 2, 'canonical sample fixes two rows')
  for (const c of conflicts) {
    assert.equal(c.resolved_at, null, 'open conflicts carry resolved_at=null')
  }
})

test('conflict-list-response-canonical.json SHA-256 matches sidecar', () => {
  const { raw, expectedHash } = loadSnapshot('conflict-list-response-canonical.json')
  assert.equal(sha256(raw), expectedHash, 'conflict-list-response-canonical.json hash drift')
})

// --- Conflict-resolve responses -----------------------------------------

test('conflict-resolve-applied-response.json validates against ResolveResponseSchema', () => {
  const { parsed } = loadSnapshot('conflict-resolve-applied-response.json')
  const ok = Value.Check(ResolveResponseSchema, parsed)
  if (!ok) {
    const errors = [...Value.Errors(ResolveResponseSchema, parsed)].slice(0, 3)
    assert.fail(`schema mismatch: ${JSON.stringify(errors)}`)
  }
  assert.deepEqual(parsed, { ok: true, status: 'applied' })
})

test('conflict-resolve-applied-response.json SHA-256 matches sidecar', () => {
  const { raw, expectedHash } = loadSnapshot('conflict-resolve-applied-response.json')
  assert.equal(sha256(raw), expectedHash, 'conflict-resolve-applied-response.json hash drift')
})

test('conflict-resolve-already-resolved-response.json validates against ErrorResponseSchema', () => {
  const { parsed } = loadSnapshot('conflict-resolve-already-resolved-response.json')
  const ok = Value.Check(ErrorResponseSchema, parsed)
  if (!ok) {
    const errors = [...Value.Errors(ErrorResponseSchema, parsed)].slice(0, 3)
    assert.fail(`schema mismatch: ${JSON.stringify(errors)}`)
  }
  const body = parsed as { code: string; details?: Record<string, unknown> }
  assert.equal(body.code, 'ALREADY_RESOLVED', 'error code is load-bearing for client retry logic')
  assert.ok(body.details?.resolvedAt, 'details.resolvedAt is load-bearing for the 409 UI')
})

test('conflict-resolve-already-resolved-response.json SHA-256 matches sidecar', () => {
  const { raw, expectedHash } = loadSnapshot('conflict-resolve-already-resolved-response.json')
  assert.equal(sha256(raw), expectedHash, 'conflict-resolve-already-resolved-response.json hash drift')
})

// --- Prometheus exposition ----------------------------------------------

test('prometheus-exposition-sample.txt SHA-256 matches sidecar', () => {
  const { raw, expectedHash } = loadSnapshot('prometheus-exposition-sample.txt')
  assert.equal(sha256(raw), expectedHash, 'prometheus-exposition-sample.txt hash drift')
})

test('prometheus-exposition-sample.txt carries all 10 named metrics from MetricsRegistry', () => {
  const { raw } = loadSnapshot('prometheus-exposition-sample.txt')
  // The 10 named metrics defined in `src/app/plugins/metrics.ts` -- a future
  // metric rename or removal must update both the registry and this sample.
  const REQUIRED_METRICS = [
    'sync_push_duration_seconds_count',
    'sync_push_duration_seconds_sum',
    'sync_push_fail_total',
    'sync_pull_duration_seconds_count',
    'sync_pull_duration_seconds_sum',
    'sync_pull_fail_total',
    'sync_conflict_total',
    'audit_query_duration_seconds_count',
    'audit_query_duration_seconds_sum',
    'outbox_depth_gauge',
  ]
  for (const metric of REQUIRED_METRICS) {
    assert.ok(
      raw.includes(`# HELP ${metric} `) && raw.includes(`# TYPE ${metric} `),
      `Prometheus sample missing HELP/TYPE pair for ${metric}`
    )
  }
})

test('prometheus-exposition-sample.txt outbox_depth_gauge carries a tenant label', () => {
  const { raw } = loadSnapshot('prometheus-exposition-sample.txt')
  // The label escape function in `metrics.ts` produces `tenant="<id>"`. The
  // tenant label is required for any meaningful alerting -- a regression
  // that drops the label would silently make the gauge un-routable.
  assert.match(raw, /outbox_depth_gauge\{tenant="[^"]+"\} \d+/, 'tenant-labelled outbox_depth_gauge missing')
})

// --- Cross-cutting invariants -------------------------------------------

test('every snapshot has a matching .sha256 sidecar and vice versa', () => {
  const { readdirSync } = require('node:fs') as typeof import('node:fs')
  const files = readdirSync(snapshotDir).sort()
  const jsonOrTxt = files.filter((f) => (f.endsWith('.json') || f.endsWith('.txt')) && !f.endsWith('.sha256'))
  const hashes = new Set(files.filter((f) => f.endsWith('.sha256')))
  for (const f of jsonOrTxt) {
    assert.ok(hashes.has(`${f}.sha256`), `missing sidecar: ${f}.sha256`)
  }
  for (const h of hashes) {
    const base = h.replace(/\.sha256$/, '')
    assert.ok(jsonOrTxt.includes(base), `orphan sidecar: ${h}`)
  }
})

test('snapshot count matches phase-09 §3.3 brief (13 wire shapes)', () => {
  const { readdirSync } = require('node:fs') as typeof import('node:fs')
  const files = readdirSync(snapshotDir)
  const samples = files.filter((f) => (f.endsWith('.json') || f.endsWith('.txt')) && !f.endsWith('.sha256'))
  // Phase-09 §3.3 brief enumerates 13 distinct wire shapes (the prose
  // groups some with slashes -- patient-push,
  // visit-push-locked/voided/pull-row, inventory-adjustment-push,
  // operator-shift-push/pull/soft-delete, audit-query-response-mixed-50-row,
  // conflict-list-response-canonical, conflict-resolve-applied-response,
  // conflict-resolve-already-resolved-response, prometheus-exposition-sample).
  assert.equal(samples.length, 13, `expected 13 snapshots, found ${samples.length}: ${samples.join(', ')}`)
})
