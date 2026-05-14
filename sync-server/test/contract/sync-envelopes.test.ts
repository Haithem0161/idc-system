// Phase-01 §3.3 contract tests: assert that canonical push/pull envelopes
// conform to the versioned TypeBox schemas served at /sync/push and
// /sync/pull. The schemas are the single source of truth (imported from
// `presentation/schemas/`); these tests pin the v1 wire format so a server
// drift produces a CI failure rather than a silent client breakage.

import { test } from 'node:test'
import * as assert from 'node:assert/strict'
import { Value } from '@sinclair/typebox/value'

import {
  PushBodySchema,
  PushResponseSchema,
} from '../../src/app/sync/presentation/schemas/push'
import {
  PullQuerySchema,
  PullResponseSchema,
} from '../../src/app/sync/presentation/schemas/pull'

// --- /sync/push body --------------------------------------------------

test('PushBodySchema accepts a minimal valid op', () => {
  const body = {
    ops: [
      {
        op_id: '01HZWAB000000000000000001',
        entity: 'audit_log',
        entity_id: '01HZWAB000000000000000002',
        op: 'upsert',
        payload_b64: 'aGVsbG8=',
      },
    ],
  }
  assert.equal(Value.Check(PushBodySchema, body), true)
})

test('PushBodySchema accepts the maximum batch of 200 ops', () => {
  const ops = Array.from({ length: 200 }, (_, i) => ({
    op_id: `op-${String(i).padStart(4, '0')}`,
    entity: 'audit_log',
    entity_id: `row-${i}`,
    op: 'upsert' as const,
    payload_b64: 'aGVsbG8=',
  }))
  assert.equal(Value.Check(PushBodySchema, { ops }), true)
})

test('PushBodySchema rejects an empty ops array (minItems: 1)', () => {
  assert.equal(Value.Check(PushBodySchema, { ops: [] }), false)
})

test('PushBodySchema rejects a batch over 200 ops (maxItems)', () => {
  const ops = Array.from({ length: 201 }, (_, i) => ({
    op_id: `op-${i}`,
    entity: 'audit_log',
    entity_id: `row-${i}`,
    op: 'upsert' as const,
    payload_b64: 'aGVsbG8=',
  }))
  assert.equal(Value.Check(PushBodySchema, { ops }), false)
})

test('PushBodySchema rejects ops with op="delete" (v1 phase-01 §7.15)', () => {
  const body = {
    ops: [
      {
        op_id: 'op-1',
        entity: 'audit_log',
        entity_id: 'row-1',
        op: 'delete',
        payload_b64: 'aGVsbG8=',
      },
    ],
  }
  assert.equal(Value.Check(PushBodySchema, body), false)
})

test('PushBodySchema rejects ops missing op_id', () => {
  const body = {
    ops: [
      {
        entity: 'audit_log',
        entity_id: 'row-1',
        op: 'upsert',
        payload_b64: 'aGVsbG8=',
      },
    ],
  }
  assert.equal(Value.Check(PushBodySchema, body), false)
})

test('PushBodySchema rejects ops with empty payload_b64', () => {
  const body = {
    ops: [
      {
        op_id: 'op-1',
        entity: 'audit_log',
        entity_id: 'row-1',
        op: 'upsert',
        payload_b64: '',
      },
    ],
  }
  assert.equal(Value.Check(PushBodySchema, body), false)
})

test('PushBodySchema rejects ops with empty entity or entity_id', () => {
  const noEntity = {
    ops: [
      {
        op_id: 'op-1',
        entity: '',
        entity_id: 'row-1',
        op: 'upsert',
        payload_b64: 'aGVsbG8=',
      },
    ],
  }
  assert.equal(Value.Check(PushBodySchema, noEntity), false)

  const noEntityId = {
    ops: [
      {
        op_id: 'op-1',
        entity: 'audit_log',
        entity_id: '',
        op: 'upsert',
        payload_b64: 'aGVsbG8=',
      },
    ],
  }
  assert.equal(Value.Check(PushBodySchema, noEntityId), false)
})

// --- /sync/push response ----------------------------------------------

test('PushResponseSchema accepts a fully populated response with both accepted and conflicts', () => {
  const response = {
    accepted: [
      { op_id: 'op-1', status: 'applied' },
      { op_id: 'op-2', status: 'duplicate' },
    ],
    conflicts: [
      {
        op_id: 'op-3',
        entity: 'audit_log',
        entity_id: 'row-3',
        server_payload: { version: 2 },
        local_payload: { version: 1 },
        reason: 'AUDIT_IMMUTABLE',
      },
    ],
  }
  assert.equal(Value.Check(PushResponseSchema, response), true)
})

test('PushResponseSchema accepts both empty arrays (idle push)', () => {
  assert.equal(
    Value.Check(PushResponseSchema, { accepted: [], conflicts: [] }),
    true
  )
})

test('PushResponseSchema rejects accepted status outside {applied, duplicate}', () => {
  const response = {
    accepted: [{ op_id: 'op-1', status: 'rejected' }],
    conflicts: [],
  }
  assert.equal(Value.Check(PushResponseSchema, response), false)
})

test('PushResponseSchema rejects conflicts missing the reason string', () => {
  const response = {
    accepted: [],
    conflicts: [
      {
        op_id: 'op-1',
        entity: 'audit_log',
        entity_id: 'row-1',
        server_payload: {},
        local_payload: {},
      },
    ],
  }
  assert.equal(Value.Check(PushResponseSchema, response), false)
})

// --- /sync/pull query --------------------------------------------------

test('PullQuerySchema accepts the empty query object (first pull)', () => {
  assert.equal(Value.Check(PullQuerySchema, {}), true)
})

test('PullQuerySchema accepts since cursor + limit within bounds', () => {
  assert.equal(
    Value.Check(PullQuerySchema, {
      since: '2026-05-13T10:00:00Z|01HZWAB000000000000000001',
      limit: 250,
    }),
    true
  )
})

test('PullQuerySchema rejects limit below 1', () => {
  assert.equal(Value.Check(PullQuerySchema, { limit: 0 }), false)
  assert.equal(Value.Check(PullQuerySchema, { limit: -5 }), false)
})

test('PullQuerySchema rejects limit above 500 (server batch cap)', () => {
  assert.equal(Value.Check(PullQuerySchema, { limit: 501 }), false)
  assert.equal(Value.Check(PullQuerySchema, { limit: 10000 }), false)
})

// --- /sync/pull response ----------------------------------------------

test('PullResponseSchema accepts a populated changes batch with next_cursor', () => {
  const response = {
    changes: [
      {
        entity: 'audit_log',
        entity_id: '01HZWAB000000000000000001',
        payload: { delta: { status: { from: null, to: 'created' } } },
        updated_at: '2026-05-13T10:00:00Z',
        version: 1,
      },
    ],
    next_cursor: '2026-05-13T10:00:00Z|01HZWAB000000000000000001',
  }
  assert.equal(Value.Check(PullResponseSchema, response), true)
})

test('PullResponseSchema accepts an empty changes array (idle pull)', () => {
  assert.equal(
    Value.Check(PullResponseSchema, { changes: [], next_cursor: '' }),
    true
  )
})

test('PullResponseSchema rejects payloads that are not objects', () => {
  const response = {
    changes: [
      {
        entity: 'audit_log',
        entity_id: 'row-1',
        payload: 'string-not-an-object',
        updated_at: '2026-05-13T10:00:00Z',
        version: 1,
      },
    ],
    next_cursor: '',
  }
  assert.equal(Value.Check(PullResponseSchema, response), false)
})

test('PullResponseSchema rejects changes missing next_cursor', () => {
  const response = { changes: [] }
  assert.equal(Value.Check(PullResponseSchema, response), false)
})

test('PullResponseSchema rejects non-numeric version', () => {
  const response = {
    changes: [
      {
        entity: 'audit_log',
        entity_id: 'row-1',
        payload: {},
        updated_at: '2026-05-13T10:00:00Z',
        version: '1',
      },
    ],
    next_cursor: '',
  }
  assert.equal(Value.Check(PullResponseSchema, response), false)
})
