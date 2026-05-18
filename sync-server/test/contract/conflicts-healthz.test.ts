// Phase-09 §3.1 contract tests: extend the existing sync-envelopes harness
// to cover the conflict-resolver routes (`GET /sync/conflicts`,
// `POST /sync/conflicts/:opId/resolve`) and the enriched `/healthz`
// response. Each schema is imported from the route module so a drift
// between this test and the runtime wire format surfaces as a CI
// failure rather than a silent client breakage.
//
// Pattern mirrors `sync-envelopes.test.ts`: `Value.Check` from
// `@sinclair/typebox/value` is Ajv-equivalent (TypeBox is the canonical
// JSON-schema-via-TS, and `Value.Check` runs Ajv-compatible validation).

import { test } from 'node:test'
import * as assert from 'node:assert/strict'
import { Value } from '@sinclair/typebox/value'

import {
  ConflictRowSchema,
  ConflictsListResponseSchema,
  ResolveBodySchema,
  ResolveParamsSchema,
  ResolveResponseSchema,
} from '../../src/app/sync/routes/conflicts'
import { HealthSchema } from '../../src/app/routes/healthz'

// --- GET /sync/conflicts response ----------------------------------

test('ConflictsListResponseSchema accepts an empty conflicts array (idle resolver)', () => {
  assert.equal(Value.Check(ConflictsListResponseSchema, { conflicts: [] }), true)
})

test('ConflictsListResponseSchema accepts a populated array with both unresolved and resolved rows', () => {
  const body = {
    conflicts: [
      {
        op_id: '01HZWAB000000000000000001',
        entity: 'visits',
        entity_id: '01HZWAB000000000000000002',
        server_payload: { version: 3 },
        local_payload: { version: 2 },
        reason: 'manual',
        resolved_at: null,
      },
      {
        op_id: '01HZWAB000000000000000003',
        entity: 'visits',
        entity_id: '01HZWAB000000000000000004',
        server_payload: { version: 5 },
        local_payload: { version: 5 },
        reason: 'manual',
        resolved_at: '2026-05-18T10:00:00.000Z',
      },
    ],
  }
  assert.equal(Value.Check(ConflictsListResponseSchema, body), true)
})

test('ConflictRowSchema accepts arbitrary unknown payload shapes (cross-entity registry)', () => {
  // server_payload + local_payload are Type.Unknown() so any JSON shape is
  // accepted -- the resolver doesn't deserialize the payload, only the UI
  // does. Pin this so a future "tighten the payload type" cleanup that
  // would break cross-entity reuse fails the contract.
  const row = {
    op_id: 'op',
    entity: 'settings',
    entity_id: '01HZ',
    server_payload: { foo: 'bar', nested: { arr: [1, 2, 3] } },
    local_payload: 'a primitive string is also valid Unknown',
    reason: 'manual',
    resolved_at: null,
  }
  assert.equal(Value.Check(ConflictRowSchema, row), true)
})

test('ConflictRowSchema rejects rows missing the reason string', () => {
  const row = {
    op_id: 'op',
    entity: 'visits',
    entity_id: 'v1',
    server_payload: {},
    local_payload: {},
    resolved_at: null,
  }
  assert.equal(Value.Check(ConflictRowSchema, row), false)
})

test('ConflictRowSchema rejects rows where resolved_at is a number (must be string|null)', () => {
  const row = {
    op_id: 'op',
    entity: 'visits',
    entity_id: 'v1',
    server_payload: {},
    local_payload: {},
    reason: 'manual',
    resolved_at: 1234567890,
  }
  assert.equal(Value.Check(ConflictRowSchema, row), false)
})

test('ConflictsListResponseSchema rejects payloads where conflicts is not an array', () => {
  assert.equal(
    Value.Check(ConflictsListResponseSchema, { conflicts: { not: 'array' } }),
    false,
  )
})

// --- POST /sync/conflicts/:opId/resolve body -----------------------

test('ResolveParamsSchema accepts a non-empty opId', () => {
  assert.equal(Value.Check(ResolveParamsSchema, { opId: '01HZ' }), true)
})

test('ResolveParamsSchema rejects an empty opId', () => {
  assert.equal(Value.Check(ResolveParamsSchema, { opId: '' }), false)
})

test('ResolveBodySchema accepts each of the 3 valid choices: local | server | merged', () => {
  for (const choice of ['local', 'server', 'merged'] as const) {
    assert.equal(
      Value.Check(ResolveBodySchema, { choice }),
      true,
      `choice=${choice} must be accepted`,
    )
  }
})

test('ResolveBodySchema rejects choice outside the closed enum', () => {
  assert.equal(Value.Check(ResolveBodySchema, { choice: 'auto' }), false)
  assert.equal(Value.Check(ResolveBodySchema, { choice: '' }), false)
})

test('ResolveBodySchema accepts the optional `merged` field as a record of unknowns', () => {
  const body = {
    choice: 'merged',
    merged: { field_a: 'left', field_b: 42, field_c: null },
  }
  assert.equal(Value.Check(ResolveBodySchema, body), true)
})

test('ResolveBodySchema accepts the optional resolve_op_id within 1..128 chars', () => {
  assert.equal(
    Value.Check(ResolveBodySchema, { choice: 'local', resolve_op_id: 'r' }),
    true,
  )
  assert.equal(
    Value.Check(ResolveBodySchema, {
      choice: 'local',
      resolve_op_id: 'r'.repeat(128),
    }),
    true,
  )
})

test('ResolveBodySchema rejects resolve_op_id over 128 chars', () => {
  assert.equal(
    Value.Check(ResolveBodySchema, {
      choice: 'local',
      resolve_op_id: 'r'.repeat(129),
    }),
    false,
  )
})

test('ResolveBodySchema rejects resolve_op_id as an empty string', () => {
  assert.equal(
    Value.Check(ResolveBodySchema, { choice: 'local', resolve_op_id: '' }),
    false,
  )
})

// --- POST /sync/conflicts/:opId/resolve response ------------------

test('ResolveResponseSchema accepts both applied and duplicate statuses', () => {
  assert.equal(
    Value.Check(ResolveResponseSchema, { ok: true, status: 'applied' }),
    true,
  )
  assert.equal(
    Value.Check(ResolveResponseSchema, { ok: true, status: 'duplicate' }),
    true,
  )
})

test('ResolveResponseSchema rejects ok=false (the route never returns ok=false; errors throw)', () => {
  // The schema's `ok` field is `Type.Literal(true)` so any other value
  // -- including false -- must be rejected. The route surfaces failures
  // as thrown errors handled by the global error plugin, not as a
  // `{ ok: false }` body.
  assert.equal(
    Value.Check(ResolveResponseSchema, { ok: false, status: 'applied' }),
    false,
  )
})

test('ResolveResponseSchema rejects status outside {applied, duplicate}', () => {
  assert.equal(
    Value.Check(ResolveResponseSchema, { ok: true, status: 'pending' }),
    false,
  )
})

// --- GET /healthz response -----------------------------------------

test('HealthSchema accepts the canonical fully-healthy response', () => {
  const body = {
    status: 'ok',
    db: 'ok',
    redis: 'ok',
    migrationsApplied: true,
    version: '0.1.0',
  }
  assert.equal(Value.Check(HealthSchema, body), true)
})

test('HealthSchema accepts the canonical fully-degraded response', () => {
  const body = {
    status: 'fail',
    db: 'fail',
    redis: 'fail',
    migrationsApplied: false,
    version: '0.1.0',
  }
  assert.equal(Value.Check(HealthSchema, body), true)
})

test('HealthSchema accepts mixed status: db ok, redis fail (partial degradation)', () => {
  // Per phase-09 §3 healthz wiring: each probe reports independently.
  // The top-level `status` aggregates but the per-probe fields are
  // independent. Pin this so a future "collapse to top-level only"
  // cleanup fails the contract.
  const body = {
    status: 'fail',
    db: 'ok',
    redis: 'fail',
    migrationsApplied: true,
    version: '0.1.0',
  }
  assert.equal(Value.Check(HealthSchema, body), true)
})

test('HealthSchema rejects status outside the ok|fail union', () => {
  const body = {
    status: 'degraded',
    db: 'ok',
    redis: 'ok',
    migrationsApplied: true,
    version: '0.1.0',
  }
  assert.equal(Value.Check(HealthSchema, body), false)
})

test('HealthSchema rejects migrationsApplied as a non-boolean', () => {
  const body = {
    status: 'ok',
    db: 'ok',
    redis: 'ok',
    migrationsApplied: 'yes',
    version: '0.1.0',
  }
  assert.equal(Value.Check(HealthSchema, body), false)
})

test('HealthSchema rejects payloads missing the version field', () => {
  const body = {
    status: 'ok',
    db: 'ok',
    redis: 'ok',
    migrationsApplied: true,
  }
  assert.equal(Value.Check(HealthSchema, body), false)
})
