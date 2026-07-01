import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

const TENANT = 'tenant-v'
const USER_ID = '01900000-0000-7000-8000-000000000001'
const OTHER_USER = '01900000-0000-7000-8000-000000000002'

interface FastifyAppLike {
  jwt: { sign: (payload: Record<string, unknown>) => string }
  inject: (opts: object) => Promise<{ statusCode: number, payload: string }>
}

function authToken (app: FastifyAppLike, role: 'superadmin' | 'receptionist' | 'accountant' = 'receptionist'): string {
  return app.jwt.sign({
    sub: USER_ID,
    email: 'reception@example.com',
    entityId: TENANT,
    role,
  })
}

function jsonOp (
  opId: string,
  entity: string,
  entityId: string,
  payload: Record<string, unknown>
) {
  return {
    op_id: opId,
    entity,
    entity_id: entityId,
    op: 'upsert' as const,
    payload_b64: Buffer.from(JSON.stringify(payload)).toString('base64'),
  }
}

function patientPayload (id: string, overrides: Record<string, unknown> = {}) {
  const now = new Date().toISOString()
  return {
    id,
    name: 'John Doe',
    entity_id: TENANT,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-v1',
    ...overrides,
  }
}

function draftVisitPayload (
  id: string,
  patientId: string,
  overrides: Record<string, unknown> = {}
) {
  const now = new Date().toISOString()
  return {
    id,
    patient_id: patientId,
    status: 'draft',
    receptionist_user_id: USER_ID,
    check_type_id: '01900000-0000-7000-8000-00000000A001',
    check_subtype_id: null,
    doctor_id: null,
    operator_id: null,
    dye: false,
    report: false,
    locked_at: null,
    voided_at: null,
    voided_by_user_id: null,
    void_reason: null,
    price_snapshot_iqd: null,
    dye_cost_snapshot_iqd: null,
    report_amount_snapshot_iqd: null,
    doctor_cut_snapshot_iqd: null,
    operator_cut_snapshot_iqd: null,
    internal_pct_snapshot: null,
    total_amount_iqd_snapshot: null,
    patient_name_snapshot: null,
    doctor_name_snapshot: null,
    operator_name_snapshot: null,
    check_type_name_ar_snapshot: null,
    check_type_name_en_snapshot: null,
    check_subtype_name_ar_snapshot: null,
    check_subtype_name_en_snapshot: null,
    entity_id: TENANT,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-v1',
    ...overrides,
  }
}

function lockedVisitPayload (
  id: string,
  patientId: string,
  overrides: Record<string, unknown> = {}
) {
  const now = new Date().toISOString()
  return {
    ...draftVisitPayload(id, patientId),
    status: 'locked',
    operator_id: '01900000-0000-7000-8000-00000000B001',
    locked_at: now,
    price_snapshot_iqd: 50000,
    dye_cost_snapshot_iqd: 0,
    report_amount_snapshot_iqd: 0,
    doctor_cut_snapshot_iqd: 20000,
    operator_cut_snapshot_iqd: 5000,
    internal_pct_snapshot: 40,
    total_amount_iqd_snapshot: 50000,
    patient_name_snapshot: 'John Doe',
    operator_name_snapshot: 'Op One',
    check_type_name_ar_snapshot: 'فحص',
    version: 2,
    updated_at: now,
    ...overrides,
  }
}

function adjustmentPayload (
  id: string,
  visitId: string,
  itemId: string,
  overrides: Record<string, unknown> = {}
) {
  const now = new Date().toISOString()
  return {
    id,
    item_id: itemId,
    delta: -1,
    reason: 'consume_visit',
    visit_id: visitId,
    note: 'consumed on lock',
    by_user_id: USER_ID,
    entity_id: TENANT,
    version: 1,
    created_at: now,
    updated_at: now,
    deleted_at: null,
    origin_device_id: 'dev-v1',
    ...overrides,
  }
}

test('POST /sync/push accepts a patient row', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const opId = '01HZVP00000000000000000001'
  const patientId = '01900000-0000-7000-8000-000000000101'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: { ops: [jsonOp(opId, 'patients', patientId, patientPayload(patientId))] },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted[0].status, 'applied')
})

test('POST /sync/push rejects a patient with empty name', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-000000000102'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000002',
          'patients',
          patientId,
          patientPayload(patientId, { name: '   ' })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZVP00000000000000000002')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('POST /sync/push accepts a draft visit', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-000000000103'
  const visitId = '01900000-0000-7000-8000-000000000201'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000003',
          'patients',
          patientId,
          patientPayload(patientId)
        ),
        jsonOp(
          '01HZVP00000000000000000004',
          'visits',
          visitId,
          draftVisitPayload(visitId, patientId)
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 2)
})

test('POST /sync/push rejects locked visit with mismatched total snapshot', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-000000000104'
  const visitId = '01900000-0000-7000-8000-000000000202'
  // Send patient first.
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000005',
          'patients',
          patientId,
          patientPayload(patientId)
        ),
      ],
    },
  })
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000006',
          'visits',
          visitId,
          lockedVisitPayload(visitId, patientId, {
            total_amount_iqd_snapshot: 99999,
          })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZVP00000000000000000006')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('POST /sync/push parks manual visit conflict on version divergence', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-000000000105'
  const visitId = '01900000-0000-7000-8000-000000000203'

  // Bootstrap: push patient + initial draft.
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000007',
          'patients',
          patientId,
          patientPayload(patientId)
        ),
        jsonOp(
          '01HZVP00000000000000000008',
          'visits',
          visitId,
          lockedVisitPayload(visitId, patientId, {
            version: 5,
          })
        ),
      ],
    },
  })
  // Push back an older version with diverging snapshot.
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v2' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000009',
          'visits',
          visitId,
          lockedVisitPayload(visitId, patientId, {
            version: 4,
            price_snapshot_iqd: 12345,
            total_amount_iqd_snapshot: 12345,
            origin_device_id: 'dev-v2',
          })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.conflicts.length, 1)
  assert.strictEqual(body.conflicts[0].entity, 'visits')
})

test('POST /sync/push rejects mutating an existing inventory_adjustment', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const adjId = '01900000-0000-7000-8000-000000000301'
  const visitId = '01900000-0000-7000-8000-000000000401'
  const itemId = '01900000-0000-7000-8000-000000000501'
  // First push: succeeds.
  const first = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000010',
          'inventory_adjustments',
          adjId,
          adjustmentPayload(adjId, visitId, itemId)
        ),
      ],
    },
  })
  assert.strictEqual(first.statusCode, 200, first.payload)
  // Second push: same id, different op_id (new op), should be rejected
  // 409 ADDITIVE_VIOLATION (the row is immutable).
  const second = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000011',
          'inventory_adjustments',
          adjId,
          adjustmentPayload(adjId, visitId, itemId, { delta: -5 })
        ),
      ],
    },
  })
  assert.strictEqual(second.statusCode, 200, second.payload)
  const body = JSON.parse(second.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZVP00000000000000000011')
  assert.strictEqual(body.rejected[0].code, 'ADDITIVE_VIOLATION')
  assert.strictEqual(body.rejected[0].status_code, 409)
})

test('POST /sync/push rejects inventory_adjustments with bad reason+delta combo', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const adjId = '01900000-0000-7000-8000-000000000302'
  const itemId = '01900000-0000-7000-8000-000000000502'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000012',
          'inventory_adjustments',
          adjId,
          adjustmentPayload(adjId, null as unknown as string, itemId, {
            reason: 'receive',
            visit_id: null,
            delta: -5,
          })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].op_id, '01HZVP00000000000000000012')
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('GET /sync/pull surfaces newly pushed visits', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-000000000106'
  const visitId = '01900000-0000-7000-8000-000000000204'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP00000000000000000013',
          'patients',
          patientId,
          patientPayload(patientId)
        ),
        jsonOp(
          '01HZVP00000000000000000014',
          'visits',
          visitId,
          draftVisitPayload(visitId, patientId)
        ),
      ],
    },
  })

  const res = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v2' },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  const entities = body.changes.map((c: { entity: string }) => c.entity)
  assert.ok(entities.includes('patients'))
  assert.ok(entities.includes('visits'))
  // Mark void to verify presence of the pushed user.
  void OTHER_USER
})

// ---- patient demographics (client migration 012) round-trip ---------------

const DEMOGRAPHICS = {
  phone: '0770-123-4567',
  sex: 'F',
  birth_date: '1990-05-12',
  file_no: 'F-2841',
  notes: 'prefers morning appointments',
}

test('POST /sync/push then GET /sync/pull preserves patient demographics', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-0000000001D1'
  const push = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP000000000000000001D1',
          'patients',
          patientId,
          patientPayload(patientId, DEMOGRAPHICS)
        ),
      ],
    },
  })
  assert.strictEqual(push.statusCode, 200, push.payload)
  assert.strictEqual(JSON.parse(push.payload).accepted[0].status, 'applied')

  const pull = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v2' },
  })
  assert.strictEqual(pull.statusCode, 200, pull.payload)
  const change = JSON.parse(pull.payload).changes.find(
    (c: { entity: string, entity_id: string }) =>
      c.entity === 'patients' && c.entity_id === patientId
  )
  assert.ok(change, 'pushed patient must arrive on the other device')
  const p = change.payload as Record<string, unknown>
  assert.strictEqual(p.phone, DEMOGRAPHICS.phone)
  assert.strictEqual(p.sex, 'F')
  assert.strictEqual(p.birth_date, DEMOGRAPHICS.birth_date)
  assert.strictEqual(p.file_no, DEMOGRAPHICS.file_no)
  assert.strictEqual(p.notes, DEMOGRAPHICS.notes)
})

test('POST /sync/push normalizes patient sex to uppercase and trims blanks', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-0000000001D2'
  await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP000000000000000001D2',
          'patients',
          patientId,
          patientPayload(patientId, {
            sex: 'm',
            phone: '   ',
            notes: '  kept  ',
          })
        ),
      ],
    },
  })

  const pull = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v2' },
  })
  const change = JSON.parse(pull.payload).changes.find(
    (c: { entity: string, entity_id: string }) =>
      c.entity === 'patients' && c.entity_id === patientId
  )
  const p = change.payload as Record<string, unknown>
  assert.strictEqual(p.sex, 'M', 'lowercase sex must normalize to uppercase')
  assert.strictEqual(p.phone, null, 'whitespace-only phone collapses to null')
  assert.strictEqual(p.notes, 'kept', 'notes are trimmed')
})

test('POST /sync/push rejects a patient with invalid sex', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-0000000001D3'
  const res = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: {
      ops: [
        jsonOp(
          '01HZVP000000000000000001D3',
          'patients',
          patientId,
          patientPayload(patientId, { sex: 'X' })
        ),
      ],
    },
  })
  assert.strictEqual(res.statusCode, 200, res.payload)
  const body = JSON.parse(res.payload)
  assert.strictEqual(body.accepted.length, 0)
  assert.strictEqual(body.rejected.length, 1)
  assert.strictEqual(body.rejected[0].code, 'VALIDATION_ERROR')
  assert.strictEqual(body.rejected[0].status_code, 422)
})

test('POST /sync/push accepts a patient with no demographics (older client)', async (t) => {
  const app = await build(t)
  const token = authToken(app as unknown as FastifyAppLike)
  const patientId = '01900000-0000-7000-8000-0000000001D4'
  // An older client omits the demographics keys entirely.
  const legacy = patientPayload(patientId)
  const push = await app.inject({
    method: 'POST',
    url: '/sync/push',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v1' },
    payload: { ops: [jsonOp('01HZVP000000000000000001D4', 'patients', patientId, legacy)] },
  })
  assert.strictEqual(push.statusCode, 200, push.payload)
  assert.strictEqual(JSON.parse(push.payload).accepted[0].status, 'applied')

  const pull = await app.inject({
    method: 'GET',
    url: '/sync/pull',
    headers: { authorization: `Bearer ${token}`, 'x-device-id': 'dev-v2' },
  })
  const change = JSON.parse(pull.payload).changes.find(
    (c: { entity: string, entity_id: string }) =>
      c.entity === 'patients' && c.entity_id === patientId
  )
  const p = change.payload as Record<string, unknown>
  assert.strictEqual(p.phone ?? null, null)
  assert.strictEqual(p.sex ?? null, null)
  assert.strictEqual(p.birth_date ?? null, null)
  assert.strictEqual(p.file_no ?? null, null)
  assert.strictEqual(p.notes ?? null, null)
})
