/**
 * Phase-09 §10 / phase-08 §7.16 -- 12-month synthetic dataset generator.
 *
 * Deterministic SQL emitter for the `scale-12-months.sql` fixture.
 * Scoped to `audit_log` rows because (a) audit is the table whose
 * performance soak matters most -- the 90-day local cliff + server
 * pagination + FTS substring search all live here -- and (b) its
 * schema has zero FK coupling to other tables, so the fixture is
 * loadable into a clean DB without prerequisite catalog/user rows.
 *
 * The other tables called out in the README scale targets (~1k
 * patients with FTS, ~10k visits, 365 days of operator_shifts) need
 * a full factory chain because they FK into users/doctors/check_types/
 * inventory_items -- that's a separate session's work; see the
 * inline TODOs in `scale-12-months.sql` for the dependency list.
 *
 * Determinism: uses mulberry32 PRNG seeded with `SCALE_SEED` (default
 * 1). The same seed always produces byte-identical SQL output. CI can
 * therefore hash the regenerated file and detect accidental hand
 * edits.
 *
 * Scale knobs (env vars):
 *
 *   SCALE_DAYS         365 by default. 30 for the committed smoke
 *                      artifact so the diff stays reviewable.
 *   SCALE_ROWS_PER_DAY 50 by default. 10 for the smoke artifact.
 *   SCALE_SEED         1 by default. Bump only with an explicit
 *                      reviewer sign-off in the PR body.
 *   SCALE_OUT          Output path. Default
 *                      `docs/idc-system/testing/fixtures/scale-12-months.sql`.
 *
 * Usage:
 *
 *   # smoke (committed artifact -- 30 days x 10 rows/day = 300 rows)
 *   npx tsx scripts/regen-scale-12-months.ts
 *
 *   # full-scale (NOT committed -- ~18k rows for the 12-month perf drill)
 *   SCALE_DAYS=365 SCALE_ROWS_PER_DAY=50 SCALE_OUT=/tmp/scale-full.sql \
 *     npx tsx scripts/regen-scale-12-months.ts
 *
 * The 14 audit action values + 15 entity values mirror the
 * server-side `ACTION_VALUES` + `ENTITY_VALUES` arrays in
 * `sync-server/src/app/domains/audit/routes/audit.ts`. A regression
 * that shrinks either enum on the Rust side will leave this generator
 * emitting rows that violate the contract; the phase-09 §3.1 audit
 * query contract test catches that on the next run.
 */

import { writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// --- Deterministic PRNG (mulberry32) -----------------------------------

function mulberry32 (seed: number): () => number {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6D2B79F5) >>> 0
    let t = state
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

// --- Closed enums (mirror server-side audit ACTION/ENTITY_VALUES) ------

const ACTIONS = [
  'create', 'update', 'soft_delete', 'lock', 'void', 'discard',
  'clock_in', 'clock_out', 'password_change', 'login', 'logout',
  'conflict_resolve', 'vacuum', 'daily_close_run',
] as const

const ENTITIES = [
  'users', 'settings', 'check_types', 'check_subtypes', 'doctors',
  'doctor_check_pricing', 'operators', 'operator_specialties',
  'operator_shifts', 'patients', 'visits', 'inventory_items',
  'inventory_consumption_map', 'inventory_adjustments', 'audit_log',
] as const

// --- Stable UUID v7-shaped string from a numeric counter ----------------
//
// audit_log.id is TEXT -- the production code uses UUID v7. For the
// fixture we emit a deterministic counter-based shape that LOOKS like
// a UUID v7 (so casts + length checks accept it) but is fully derived
// from the row index. This keeps the SQL byte-stable across regens.

function detIdFromIndex (i: number, kind: number): string {
  // Format mirrors UUID v7: 8-4-4-4-12 hex chars, 32 hex total.
  // We pack (timestamp, kind, counter) into the layout. Deterministic.
  const ts = ('00000000000000' + (1735689600000 + i * 1000).toString(16)).slice(-12)
  const k = ('0000' + kind.toString(16)).slice(-4)
  const ctr = ('000000000000' + i.toString(16)).slice(-12)
  return `${ts.slice(0, 8)}-${ts.slice(8, 12)}-7000-${k}-${ctr}`
}

// --- Main generator ----------------------------------------------------

function generate (opts: {
  days: number
  rowsPerDay: number
  seed: number
}): string {
  const { days, rowsPerDay, seed } = opts
  const rng = mulberry32(seed)
  const totalRows = days * rowsPerDay

  // The "today" anchor for the fixture is 2026-05-18T00:00:00Z; rows
  // span backwards into the 365-day window. Deterministic anchor means
  // re-runs produce byte-identical SQL.
  const anchor = new Date('2026-05-18T00:00:00.000Z').getTime()
  const dayMs = 24 * 60 * 60 * 1000
  const tenantId = '01923af0-7c1a-7000-e000-000000000001'
  const actors = [
    '01923af0-7c1a-7000-a001-000000000001', // Mariam (superadmin)
    '01923af0-7c1a-7000-a002-000000000001', // Asma (accountant)
    '01923af0-7c1a-7000-a003-000000000001', // Mehdi (reception)
    '01923af0-7c1a-7000-a004-000000000001', // Sara (reception)
  ]
  const devices = ['dev-reception-1', 'dev-reception-2', 'dev-accountant-1', 'dev-admin-1']

  const out: string[] = []
  out.push('-- File: scale-12-months.sql')
  out.push('-- Purpose: 12-month audit_log volume fixture for the perf drill (phase-09 §10).')
  out.push('-- Schema version: local-migration-009 + server-prisma-phase-09')
  out.push(`-- Regenerated: 2026-05-18 (smoke -- days=${days} rows_per_day=${rowsPerDay} seed=${seed})`)
  out.push('-- Regen script: scripts/regen-scale-12-months.ts')
  out.push('-- Loadable into: SQLite | Postgres | Both')
  out.push('-- DO NOT EDIT BY HAND. Regenerate via the script above.')
  out.push('--')
  out.push(`-- Total rows: ${totalRows} audit_log entries spanning ${days} days backwards from`)
  out.push('-- 2026-05-18T00:00:00Z. PRNG is mulberry32; the same seed always produces')
  out.push('-- byte-identical output. Other scale targets (1k patients with FTS, 10k visits,')
  out.push('-- 365 operator_shifts, doctor/operator catalog) require a full factory chain')
  out.push('-- across FK boundaries -- see the README §"Scaling beyond audit_log" follow-up.')
  out.push('')
  out.push('BEGIN;')
  out.push('')

  for (let i = 0; i < totalRows; i++) {
    const dayOffset = Math.floor(i / rowsPerDay)
    const intraDay = Math.floor(rng() * dayMs)
    const at = new Date(anchor - dayOffset * dayMs + intraDay).toISOString()
    const action = ACTIONS[Math.floor(rng() * ACTIONS.length)]
    const entity = ENTITIES[Math.floor(rng() * ENTITIES.length)]
    const actor = actors[Math.floor(rng() * actors.length)]
    const device = devices[Math.floor(rng() * devices.length)]
    const id = detIdFromIndex(i, 0x9000)
    const entityIdRow = detIdFromIndex(i, 0xc000)
    // delta is a JSON-encoded TEXT column on the SQLite side; valid
    // JSON content keeps phase-08 audit-query substring search happy.
    const delta = JSON.stringify({ mode: i % 2 === 0 ? 'manual' : 'system', i })
    const ip = i % 3 === 0 ? 'NULL' : `'10.0.0.${42 + (i % 10)}'`
    // Single line per row. Quotes inside delta are escaped per SQL.
    out.push(
      `INSERT INTO audit_log (id, actor_user_id, action, entity, entity_id, delta, ip, device_id, at, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id_tenant) VALUES (` +
      `'${id}', '${actor}', '${action}', '${entity}', '${entityIdRow}', '${delta.replace(/'/g, "''")}', ${ip}, '${device}', '${at}', '${at}', '${at}', NULL, 1, 0, NULL, '${device}', '${tenantId}');`,
    )
  }

  out.push('')
  out.push('COMMIT;')
  out.push('')
  return out.join('\n')
}

// --- Entry point -------------------------------------------------------

function main (): void {
  const days = Number(process.env.SCALE_DAYS ?? '30')
  const rowsPerDay = Number(process.env.SCALE_ROWS_PER_DAY ?? '10')
  const seed = Number(process.env.SCALE_SEED ?? '1')
  const here = dirname(fileURLToPath(import.meta.url))
  const defaultOut = resolve(
    here,
    '..',
    'docs',
    'idc-system',
    'testing',
    'fixtures',
    'scale-12-months.sql',
  )
  const outPath = process.env.SCALE_OUT ?? defaultOut

  const sql = generate({ days, rowsPerDay, seed })
  writeFileSync(outPath, sql, 'utf8')
  // eslint-disable-next-line no-console
  console.log(
    `[regen-scale-12-months] wrote ${days * rowsPerDay} audit_log rows ` +
      `(days=${days}, rows_per_day=${rowsPerDay}, seed=${seed}) to ${outPath}`,
  )
}

// Auto-run when invoked directly (not on import in tests).
const isMain =
  typeof process !== 'undefined' &&
  process.argv[1] !== undefined &&
  process.argv[1].endsWith('regen-scale-12-months.ts')

if (isMain) {
  main()
}

// Test hook -- exported for the regression test.
export const __testing__ = { generate, mulberry32, ACTIONS, ENTITIES }
