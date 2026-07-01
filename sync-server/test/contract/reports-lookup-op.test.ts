// Phase-09 §3.1 contract tests for two remaining route surfaces:
//
// 1. `GET /reports/visits` + `GET /reports/daily-close/:date` -- the
//    accountant-facing reports surface. Schemas validate request
//    queries, totals + rows + groups (tagged-union response shape),
//    and the full daily-close artifact (per-doctor + per-operator +
//    per-check-type breakdowns).
// 2. `POST /sync/lookup-op` -- the startup-reconcile lookup that
//    closes the audit-log push-recovery gap (phase-01 §7.20).
//
// `Value.Check` from `@sinclair/typebox/value` runs the same validation
// pipeline as Fastify's runtime Ajv compiler when format checkers are
// registered (this file inherits the date-time / uuid / email
// registrations from the sibling contract tests).

import { test } from 'node:test'
import * as assert from 'node:assert/strict'
import { FormatRegistry } from '@sinclair/typebox'
import { Value } from '@sinclair/typebox/value'

import {
  VisitsQuerySchema,
  TotalsSchema,
  RowSchema,
  GroupSchema,
  VisitsResponseSchema,
  DailyCloseParamsSchema,
  DailyCloseQuerySchema,
  DailyCloseResponseSchema,
} from '../../src/app/domains/reports/routes/reports'
import {
  LookupBodySchema,
  LookupResponseSchema,
} from '../../src/app/sync/routes/lookup-op'

// Register the same formats as the sibling contract tests so this file
// is self-contained when run in isolation.
const ISO_DATE_TIME =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,9})?(Z|[+-]\d{2}:?\d{2})$/
const UUID =
  /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/

if (!FormatRegistry.Has('date-time')) {
  FormatRegistry.Set('date-time', (value) => ISO_DATE_TIME.test(value))
}
if (!FormatRegistry.Has('uuid')) {
  FormatRegistry.Set('uuid', (value) => UUID.test(value))
}

const VALID_UUID = '01931a8a-7c8e-7c4a-9b2d-1234567890ab'

// =========================================================================
// VisitsQuerySchema (GET /reports/visits request)
// =========================================================================

test('VisitsQuerySchema accepts the minimal valid query: from + to only', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
  }
  assert.equal(Value.Check(VisitsQuerySchema, q), true)
})

test('VisitsQuerySchema accepts each of the 7 groupBy modes', () => {
  for (const groupBy of [
    'none', 'by_date', 'by_doctor', 'by_operator',
    'by_check_type', 'by_subtype', 'by_status',
  ]) {
    const q = {
      from: '2026-05-01T00:00:00.000Z',
      to: '2026-05-18T23:59:59.999Z',
      groupBy,
    }
    assert.equal(Value.Check(VisitsQuerySchema, q), true, `groupBy=${groupBy} must be accepted`)
  }
})

test('VisitsQuerySchema rejects groupBy outside the 7-value closed enum', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    groupBy: 'by_year',
  }
  assert.equal(Value.Check(VisitsQuerySchema, q), false)
})

test('VisitsQuerySchema accepts the 3 status tri-states', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    statuses: ['draft', 'locked', 'voided'],
  }
  assert.equal(Value.Check(VisitsQuerySchema, q), true)
})

test('VisitsQuerySchema accepts the 3 dye + report tri-states', () => {
  for (const flag of ['y', 'n', 'all']) {
    const q = {
      from: '2026-05-01T00:00:00.000Z',
      to: '2026-05-18T23:59:59.999Z',
      dye: flag,
      report: flag,
    }
    assert.equal(Value.Check(VisitsQuerySchema, q), true, `${flag} accepted`)
  }
})

test('VisitsQuerySchema rejects dye outside {y, n, all}', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    dye: 'maybe',
  }
  assert.equal(Value.Check(VisitsQuerySchema, q), false)
})

test('VisitsQuerySchema rejects UUID arrays with malformed entries', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    doctorIds: ['not-a-uuid'],
  }
  assert.equal(Value.Check(VisitsQuerySchema, q), false)
})

test('VisitsQuerySchema rejects limit with non-digit pattern', () => {
  const q = {
    from: '2026-05-01T00:00:00.000Z',
    to: '2026-05-18T23:59:59.999Z',
    limit: '1000abc',
  }
  assert.equal(Value.Check(VisitsQuerySchema, q), false)
})

// =========================================================================
// TotalsSchema + RowSchema + GroupSchema (response leaves)
// =========================================================================

test('TotalsSchema accepts all-zero day (idle slice)', () => {
  const t = {
    visits: 0,
    revenue_iqd: 0,
    doctor_cut_iqd: 0,
    operator_cut_iqd: 0,
    report_iqd: 0,
    mandoub_cut_iqd: 0,
    net_iqd: 0,
  }
  assert.equal(Value.Check(TotalsSchema, t), true)
})

test('TotalsSchema rejects non-integer values (money is always Int IQD)', () => {
  const t = {
    visits: 1,
    revenue_iqd: 1.5,
    doctor_cut_iqd: 0,
    operator_cut_iqd: 0,
    net_iqd: 0,
  }
  assert.equal(Value.Check(TotalsSchema, t), false)
})

test('RowSchema accepts a house visit (doctor_name null) with subtype', () => {
  const row = {
    visit_id: '01HZ',
    locked_at: '2026-05-18T10:30:00.000Z',
    status: 'locked',
    patient_name: 'Layla',
    doctor_name: null,
    operator_name: 'Kareem',
    check_type_name_ar: 'صدى',
    check_type_name_en: 'Echo',
    check_subtype_name_ar: 'فحص شامل',
    check_subtype_name_en: 'Comprehensive',
    dye: true,
    report: true,
    price_iqd: 75_000,
    doctor_cut_iqd: 0,
    operator_cut_iqd: 2_500,
    report_iqd: 0,
    mandoub_cut_iqd: 0,
    net_iqd: 72_500,
  }
  assert.equal(Value.Check(RowSchema, row), true)
})

test('RowSchema accepts a doctor visit without subtype (all subtype fields null)', () => {
  const row = {
    visit_id: '01HZ',
    locked_at: '2026-05-18T10:30:00.000Z',
    status: 'locked',
    patient_name: 'Layla',
    doctor_name: 'Dr Sara',
    operator_name: 'Kareem',
    check_type_name_ar: 'أشعة',
    check_type_name_en: null,
    check_subtype_name_ar: null,
    check_subtype_name_en: null,
    dye: false,
    report: false,
    price_iqd: 50_000,
    doctor_cut_iqd: 15_000,
    operator_cut_iqd: 1_000,
    report_iqd: 0,
    mandoub_cut_iqd: 0,
    net_iqd: 34_000,
  }
  assert.equal(Value.Check(RowSchema, row), true)
})

test('GroupSchema accepts a group row with key + label + per-group totals', () => {
  const g = {
    key: 'dr-sara',
    label: 'Dr Sara',
    visits: 4,
    revenue_iqd: 200_000,
    doctor_cut_iqd: 60_000,
    operator_cut_iqd: 4_000,
    report_iqd: 0,
    mandoub_cut_iqd: 0,
    net_iqd: 136_000,
  }
  assert.equal(Value.Check(GroupSchema, g), true)
})

// =========================================================================
// VisitsResponseSchema (tagged-union: rows mode | groups mode)
// =========================================================================

test('VisitsResponseSchema accepts rows-mode response', () => {
  const body = {
    mode: 'rows',
    rows: [],
    totals: {
      visits: 0,
      revenue_iqd: 0,
      doctor_cut_iqd: 0,
      operator_cut_iqd: 0,
      report_iqd: 0,
      mandoub_cut_iqd: 0,
      net_iqd: 0,
    },
  }
  assert.equal(Value.Check(VisitsResponseSchema, body), true)
})

test('VisitsResponseSchema accepts groups-mode response', () => {
  const body = {
    mode: 'groups',
    groups: [],
    totals: {
      visits: 0,
      revenue_iqd: 0,
      doctor_cut_iqd: 0,
      operator_cut_iqd: 0,
      report_iqd: 0,
      mandoub_cut_iqd: 0,
      net_iqd: 0,
    },
  }
  assert.equal(Value.Check(VisitsResponseSchema, body), true)
})

test('VisitsResponseSchema rejects rows-mode with groups field (tag/payload mismatch)', () => {
  const body = {
    mode: 'rows',
    groups: [],
    totals: {
      visits: 0,
      revenue_iqd: 0,
      doctor_cut_iqd: 0,
      operator_cut_iqd: 0,
      net_iqd: 0,
    },
  }
  assert.equal(Value.Check(VisitsResponseSchema, body), false)
})

test('VisitsResponseSchema rejects mode outside the rows|groups union', () => {
  const body = {
    mode: 'aggregated',
    rows: [],
    totals: {
      visits: 0,
      revenue_iqd: 0,
      doctor_cut_iqd: 0,
      operator_cut_iqd: 0,
      net_iqd: 0,
    },
  }
  assert.equal(Value.Check(VisitsResponseSchema, body), false)
})

// =========================================================================
// DailyCloseParamsSchema + DailyCloseQuerySchema (request)
// =========================================================================

test('DailyCloseParamsSchema accepts YYYY-MM-DD format', () => {
  assert.equal(Value.Check(DailyCloseParamsSchema, { date: '2026-05-18' }), true)
})

test('DailyCloseParamsSchema rejects RFC3339 timestamps + invalid date strings', () => {
  assert.equal(
    Value.Check(DailyCloseParamsSchema, { date: '2026-05-18T10:00:00Z' }),
    false,
  )
  assert.equal(Value.Check(DailyCloseParamsSchema, { date: '2026-5-18' }), false)
  assert.equal(Value.Check(DailyCloseParamsSchema, { date: '' }), false)
})

test('DailyCloseQuerySchema accepts positive + negative tzOffsetMinutes integers', () => {
  assert.equal(
    Value.Check(DailyCloseQuerySchema, { tzOffsetMinutes: '180' }),
    true,
  )
  assert.equal(
    Value.Check(DailyCloseQuerySchema, { tzOffsetMinutes: '-300' }),
    true,
  )
  assert.equal(Value.Check(DailyCloseQuerySchema, {}), true, 'optional accepts absent')
})

test('DailyCloseQuerySchema rejects tzOffsetMinutes with non-digit chars', () => {
  assert.equal(
    Value.Check(DailyCloseQuerySchema, { tzOffsetMinutes: '180m' }),
    false,
  )
})

// =========================================================================
// DailyCloseResponseSchema (response artifact)
// =========================================================================

test('DailyCloseResponseSchema accepts the canonical full artifact', () => {
  const body = {
    tenant_id: 'tenant-1',
    target_date: '2026-05-18',
    tz_offset: '+03:00',
    total_revenue_iqd: 1_000_000,
    total_doctor_cuts_iqd: 200_000,
    total_operator_cuts_iqd: 40_000,
    total_report_iqd: 0,
    total_mandoub_cuts_iqd: 0,
    total_inventory_consumption_value_iqd: 25_000,
    net_iqd: 735_000,
    locked_count: 12,
    voided_count: 1,
    voided_value_iqd: 50_000,
    per_doctor: [
      {
        doctor_id: VALID_UUID,
        name: 'Dr Sara',
        visits: 4,
        revenue_iqd: 200_000,
        doctor_cut_iqd: 60_000,
      },
    ],
    per_operator: [
      {
        operator_id: VALID_UUID,
        name: 'Kareem',
        visits: 8,
        dye_visits: 2,
        operator_cut_iqd: 8_000,
        hours_on_shift_milli: 28_800_000,
      },
    ],
    per_mandoub: [
      {
        mandoub_id: VALID_UUID,
        name: 'Mandoub One',
        visits: 2,
        mandoub_cut_iqd: 1_500,
      },
    ],
    per_check_type: [
      {
        check_type_id: VALID_UUID,
        name_ar: 'صدى',
        name_en: 'Echo',
        visits: 4,
        revenue_iqd: 200_000,
        doctor_cut_iqd: 60_000,
        operator_cut_iqd: 2_000,
      },
    ],
    generated_at: '2026-05-18T23:59:00.000Z',
  }
  assert.equal(Value.Check(DailyCloseResponseSchema, body), true)
})

test('DailyCloseResponseSchema accepts house-row doctor_id=null', () => {
  // The "House" pseudo-row aggregates doctor_id IS NULL visits per
  // phase-07 §7.4. The schema MUST allow null here.
  const body = {
    tenant_id: 'tenant-1',
    target_date: '2026-05-18',
    tz_offset: '+03:00',
    total_revenue_iqd: 0,
    total_doctor_cuts_iqd: 0,
    total_operator_cuts_iqd: 0,
    total_report_iqd: 0,
    total_mandoub_cuts_iqd: 0,
    total_inventory_consumption_value_iqd: 0,
    net_iqd: 0,
    locked_count: 0,
    voided_count: 0,
    voided_value_iqd: 0,
    per_doctor: [
      {
        doctor_id: null,
        name: 'House',
        visits: 0,
        revenue_iqd: 0,
        doctor_cut_iqd: 0,
      },
    ],
    per_operator: [],
    per_mandoub: [],
    per_check_type: [],
    generated_at: '2026-05-18T23:59:00.000Z',
  }
  assert.equal(Value.Check(DailyCloseResponseSchema, body), true)
})

test('DailyCloseResponseSchema rejects bodies missing per_doctor breakdown', () => {
  const body = {
    tenant_id: 'tenant-1',
    target_date: '2026-05-18',
    tz_offset: '+03:00',
    total_revenue_iqd: 0,
    total_doctor_cuts_iqd: 0,
    total_operator_cuts_iqd: 0,
    total_inventory_consumption_value_iqd: 0,
    net_iqd: 0,
    locked_count: 0,
    voided_count: 0,
    voided_value_iqd: 0,
    per_operator: [],
    per_check_type: [],
    generated_at: '2026-05-18T23:59:00.000Z',
  }
  assert.equal(Value.Check(DailyCloseResponseSchema, body), false)
})

// =========================================================================
// LookupBodySchema + LookupResponseSchema (POST /sync/lookup-op)
// =========================================================================

test('LookupBodySchema accepts a single op_id (minItems: 1)', () => {
  assert.equal(Value.Check(LookupBodySchema, { op_ids: ['op-1'] }), true)
})

test('LookupBodySchema accepts the maximum batch of 200 op_ids', () => {
  const op_ids = Array.from({ length: 200 }, (_, i) => `op-${i}`)
  assert.equal(Value.Check(LookupBodySchema, { op_ids }), true)
})

test('LookupBodySchema rejects an empty op_ids array', () => {
  assert.equal(Value.Check(LookupBodySchema, { op_ids: [] }), false)
})

test('LookupBodySchema rejects batches over 200 op_ids', () => {
  const op_ids = Array.from({ length: 201 }, (_, i) => `op-${i}`)
  assert.equal(Value.Check(LookupBodySchema, { op_ids }), false)
})

test('LookupBodySchema rejects op_ids with empty string entries', () => {
  assert.equal(Value.Check(LookupBodySchema, { op_ids: [''] }), false)
})

test('LookupResponseSchema accepts an empty found array (no matches)', () => {
  assert.equal(Value.Check(LookupResponseSchema, { found: [] }), true)
})

test('LookupResponseSchema accepts a populated found array', () => {
  assert.equal(
    Value.Check(LookupResponseSchema, { found: ['op-1', 'op-3'] }),
    true,
  )
})

test('LookupResponseSchema rejects bodies missing the found field', () => {
  assert.equal(Value.Check(LookupResponseSchema, {}), false)
})
