// Phase-09 §3.1 contract tests for the audit query route's TypeBox
// schemas (`GET /audit/query`). Drift-tests the AuditQuerySchema (request),
// AuditRowSchema (per-row), and AuditQueryResponseSchema (envelope).
//
// Critical invariants pinned:
// - The 14-action and 15-entity closed enums (mirror phase-01 §7.36 + the
//   phase-07 §7.18 `daily_close_run` extension + phase-08's exhaustive
//   syncable-entity list). A regression that adds a new action/entity
//   server-side WITHOUT extending these arrays would silently accept
//   queries that the route handler can't honor.
// - entity_id_prefix bounds (4..36 chars; UUID format constraint).
// - text search bounds (2..100 chars).
// - limit is a string-typed integer (Fastify query params are always
//   string before Type-coerce; the route handler parses to int).
// - next_cursor in the response is string|null (cursor exhausted).
// - delta is Type.Unknown() so per-action shapes can vary without
//   widening the schema.

import { test } from 'node:test'
import * as assert from 'node:assert/strict'
import { FormatRegistry } from '@sinclair/typebox'
import { Value } from '@sinclair/typebox/value'

import {
  ACTION_VALUES,
  ENTITY_VALUES,
  AuditQuerySchema,
  AuditRowSchema,
  AuditQueryResponseSchema,
} from '../../src/app/domains/audit/routes/audit'

// TypeBox's `Value.Check` consults `FormatRegistry` for `format` keywords
// and rejects unknown formats by default. The route schemas use
// `format: 'date-time'` (ISO 8601) and `format: 'uuid'`. Ajv ships these
// by default via `ajv-formats`; TypeBox's `Value.Check` does not. We
// register them here so the contract tests mirror the runtime Fastify
// validator's behavior (Fastify wires Ajv with ajv-formats).
const ISO_DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,9})?(Z|[+-]\d{2}:?\d{2})$/
const UUID = /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/

if (!FormatRegistry.Has('date-time')) {
  FormatRegistry.Set('date-time', (value) => ISO_DATE_TIME.test(value))
}
if (!FormatRegistry.Has('uuid')) {
  FormatRegistry.Set('uuid', (value) => UUID.test(value))
}

// --- ACTION_VALUES / ENTITY_VALUES exhaustiveness ----------------

test('ACTION_VALUES enumerates exactly the 14 phase-09 actions', () => {
  // Pinning the count + the exact set guards against (a) silent drops
  // when a future refactor renames or removes an action, and (b) silent
  // additions that bypass the audit query path's enumeration.
  assert.equal(ACTION_VALUES.length, 14, 'must list exactly 14 actions')
  for (const expected of [
    'create', 'update', 'soft_delete', 'lock', 'void', 'discard',
    'clock_in', 'clock_out', 'password_change', 'login', 'logout',
    'conflict_resolve', 'vacuum', 'daily_close_run',
  ]) {
    assert.ok(
      ACTION_VALUES.includes(expected as never),
      `ACTION_VALUES must include ${expected}`,
    )
  }
})

test('ENTITY_VALUES enumerates exactly the 15 syncable entities', () => {
  assert.equal(ENTITY_VALUES.length, 15, 'must list exactly 15 entities')
  for (const expected of [
    'users', 'settings', 'check_types', 'check_subtypes', 'doctors',
    'doctor_check_pricing', 'operators', 'operator_specialties',
    'operator_shifts', 'patients', 'visits', 'inventory_items',
    'inventory_consumption_map', 'inventory_adjustments', 'audit_log',
  ]) {
    assert.ok(
      ENTITY_VALUES.includes(expected as never),
      `ENTITY_VALUES must include ${expected}`,
    )
  }
})

// --- AuditQuerySchema (request) ----------------------------------

test('AuditQuerySchema accepts the minimal valid query: from + to only', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), true)
})

test('AuditQuerySchema accepts the fully populated 9-field query', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    actor: '01931a8a-7c8e-7c4a-9b2d-1234567890ab',
    action: 'lock',
    entity: 'visits',
    entity_id_prefix: '0000abcd',
    text: 'duplicate billing',
    cursor: 'eyJhdCI6IjIwMjYtMDUtMTgifQ==',
    limit: '50',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), true)
})

test('AuditQuerySchema rejects missing required from', () => {
  const q = { to: '2026-05-18T23:59:59.999Z' }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects missing required to', () => {
  const q = { from: '2026-05-01T00:00:00.000Z' }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects action outside the 14-value closed enum', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    action: 'fabricate',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects entity outside the 15-value closed enum', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    entity: 'workouts',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects entity_id_prefix below 4 chars', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    entity_id_prefix: 'abc',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects entity_id_prefix above 36 chars', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    entity_id_prefix: 'a'.repeat(37),
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects text below 2 chars', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    text: 'a',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects text above 100 chars', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    text: 'x'.repeat(101),
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

test('AuditQuerySchema rejects limit with non-digit characters', () => {
  // The pattern `^\\d+$` allows only digit strings. A common mistake
  // would be passing limit as a number; Fastify would coerce but the
  // raw schema rejects non-string. The route handler parses to int.
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    limit: '50px',
  }
  assert.equal(Value.Check(AuditQuerySchema, q), false)
})

// --- AuditRowSchema (per-row) ------------------------------------

test('AuditRowSchema accepts a fully populated row with non-null ip', () => {
  const row = {
    id: '01HZWAB000000000000000001',
    actor_user_id: '01HZWAB000000000000000002',
    action: 'lock',
    entity: 'visits',
    entity_id: '01HZWAB000000000000000003',
    delta: { from: { status: 'draft' }, to: { status: 'locked' } },
    ip: '10.0.0.1',
    device_id: 'dev-A',
    at: '2026-05-18T10:30:00.000Z',
    version: 1,
    entity_id_tenant: 'tenant-1',
  }
  assert.equal(Value.Check(AuditRowSchema, row), true)
})

test('AuditRowSchema accepts a row with null ip', () => {
  const row = {
    id: '01HZ',
    actor_user_id: '01HZ',
    action: 'login',
    entity: 'users',
    entity_id: '01HZ',
    delta: { method: 'password', mode: 'offline' },
    ip: null,
    device_id: 'dev-A',
    at: '2026-05-18T10:30:00.000Z',
    version: 1,
    entity_id_tenant: 'tenant-1',
  }
  assert.equal(Value.Check(AuditRowSchema, row), true)
})

test('AuditRowSchema accepts arbitrary delta shapes (Unknown -- per-action varies)', () => {
  // delta varies per action: lock has {from, to}, login has
  // {method, mode}, vacuum has {audit_purged}, etc. Type.Unknown
  // is the load-bearing wire decision so the schema stays open
  // for new action shapes without a schema bump.
  const row = {
    id: '01HZ',
    actor_user_id: '01HZ',
    action: 'vacuum',
    entity: 'audit_log',
    entity_id: '00000000-0000-0000-0000-000000000000',
    delta: { audit_purged: 42, cutoff: '2026-02-18T00:00:00.000Z' },
    ip: null,
    device_id: 'system',
    at: '2026-05-18T03:00:00.000Z',
    version: 1,
    entity_id_tenant: 'tenant-1',
  }
  assert.equal(Value.Check(AuditRowSchema, row), true)
})

test('AuditRowSchema rejects rows missing the version (integer required)', () => {
  const row = {
    id: '01HZ',
    actor_user_id: '01HZ',
    action: 'create',
    entity: 'users',
    entity_id: '01HZ',
    delta: {},
    ip: null,
    device_id: 'dev-A',
    at: '2026-05-18T10:30:00.000Z',
    entity_id_tenant: 'tenant-1',
  }
  assert.equal(Value.Check(AuditRowSchema, row), false)
})

test('AuditRowSchema rejects rows where version is a float', () => {
  const row = {
    id: '01HZ',
    actor_user_id: '01HZ',
    action: 'create',
    entity: 'users',
    entity_id: '01HZ',
    delta: {},
    ip: null,
    device_id: 'dev-A',
    at: '2026-05-18T10:30:00.000Z',
    version: 1.5,
    entity_id_tenant: 'tenant-1',
  }
  assert.equal(Value.Check(AuditRowSchema, row), false)
})

test('AuditRowSchema rejects ip as a non-string non-null', () => {
  const row = {
    id: '01HZ',
    actor_user_id: '01HZ',
    action: 'create',
    entity: 'users',
    entity_id: '01HZ',
    delta: {},
    ip: 12345,
    device_id: 'dev-A',
    at: '2026-05-18T10:30:00.000Z',
    version: 1,
    entity_id_tenant: 'tenant-1',
  }
  assert.equal(Value.Check(AuditRowSchema, row), false)
})

// --- AuditQueryResponseSchema (envelope) -------------------------

test('AuditQueryResponseSchema accepts an empty rows array with null next_cursor (exhausted)', () => {
  assert.equal(
    Value.Check(AuditQueryResponseSchema, { rows: [], next_cursor: null }),
    true,
  )
})

test('AuditQueryResponseSchema accepts a populated rows array with a string next_cursor', () => {
  const body = {
    rows: [
      {
        id: '01HZ',
        actor_user_id: '01HZ',
        action: 'create',
        entity: 'users',
        entity_id: '01HZ',
        delta: {},
        ip: null,
        device_id: 'dev-A',
        at: '2026-05-18T10:30:00.000Z',
        version: 1,
        entity_id_tenant: 'tenant-1',
      },
    ],
    next_cursor: 'eyJhdCI6IjIwMjYtMDUtMTgifQ==',
  }
  assert.equal(Value.Check(AuditQueryResponseSchema, body), true)
})

test('AuditQueryResponseSchema rejects bodies missing next_cursor', () => {
  assert.equal(Value.Check(AuditQueryResponseSchema, { rows: [] }), false)
})

test('AuditQueryResponseSchema rejects rows that are not an array', () => {
  assert.equal(
    Value.Check(AuditQueryResponseSchema, { rows: null, next_cursor: null }),
    false,
  )
})
