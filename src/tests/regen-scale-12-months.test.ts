/// <reference types="node" />
// Phase-09 §10 -- regen-scale-12-months.ts determinism + invariant tests.
//
// The fixture file `docs/idc-system/testing/fixtures/scale-12-months.sql`
// is generated; the canonical committed form is the smoke size (30 days
// x 10 rows/day = 300 rows). These tests pin:
//
//   (1) Determinism -- same seed produces byte-identical SQL across runs.
//   (2) Closed-enum invariant -- every emitted `action` and `entity`
//       value is in the corresponding ACTION_VALUES / ENTITY_VALUES
//       arrays mirrored on the server side (a future enum shrink leaves
//       this generator out of sync; the test fails immediately).
//   (3) Row count math -- generate(days, rows_per_day) emits exactly
//       (days * rows_per_day) INSERT statements.
//   (4) Header contract -- the output starts with the canonical
//       `-- File:` / `-- Purpose:` / `-- Schema version:` /
//       `-- DO NOT EDIT BY HAND` lines from the fixtures README.
//
// The committed `scale-12-months.sql` artifact is verified end-to-end
// here: read the file, re-derive it via the generator with the same
// seed/days/rows_per_day used at commit time, assert byte equality.

import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

import { __testing__ } from "../../scripts/regen-scale-12-months"

const { generate, ACTIONS, ENTITIES, mulberry32 } = __testing__

describe("Phase-09 §10 regen-scale-12-months -- determinism + invariants", () => {
  it("mulberry32 is deterministic across runs given the same seed", () => {
    const a = mulberry32(1)
    const b = mulberry32(1)
    for (let i = 0; i < 100; i++) {
      expect(a()).toBe(b())
    }
  })

  it("mulberry32 with different seeds produces different streams", () => {
    const a = mulberry32(1)
    const b = mulberry32(2)
    const diff = Array.from({ length: 10 }, () => a() !== b()).filter(Boolean).length
    // At least 8 of 10 should differ -- a collision rate above that
    // would indicate a broken PRNG.
    expect(diff).toBeGreaterThanOrEqual(8)
  })

  it("generate produces byte-identical output across two invocations", () => {
    const opts = { days: 7, rowsPerDay: 5, seed: 1 }
    const first = generate(opts)
    const second = generate(opts)
    expect(first).toBe(second)
  })

  it("generate emits exactly (days * rows_per_day) INSERT statements", () => {
    const sql = generate({ days: 7, rowsPerDay: 5, seed: 1 })
    const inserts = sql.split("\n").filter((l: string) =>
      l.startsWith("INSERT INTO audit_log"),
    )
    expect(inserts.length).toBe(7 * 5)
  })

  it("every emitted action is in the closed ACTIONS enum", () => {
    const sql = generate({ days: 30, rowsPerDay: 10, seed: 1 })
    const actionMatches = Array.from(
      sql.matchAll(/'([a-z_]+)', '[a-z_]+', '[0-9a-f-]{36}', '\{/g),
    ).map((m: RegExpMatchArray) => m[1])
    expect(actionMatches.length).toBeGreaterThan(0)
    for (const a of actionMatches) {
      expect(ACTIONS as readonly string[]).toContain(a)
    }
  })

  it("every emitted entity is in the closed ENTITIES enum", () => {
    const sql = generate({ days: 30, rowsPerDay: 10, seed: 1 })
    const entityMatches = Array.from(
      sql.matchAll(/'[a-z_]+', '([a-z_]+)', '[0-9a-f-]{36}', '\{/g),
    ).map((m: RegExpMatchArray) => m[1])
    expect(entityMatches.length).toBeGreaterThan(0)
    for (const e of entityMatches) {
      expect(ENTITIES as readonly string[]).toContain(e)
    }
  })

  it("output exercises both null and string ip values (tri-state invariant)", () => {
    const sql = generate({ days: 30, rowsPerDay: 10, seed: 1 })
    expect(sql).toMatch(/, NULL, /)
    expect(sql).toMatch(/, '10\.0\.0\.\d+', /)
  })

  it("header carries the canonical contract block", () => {
    const sql = generate({ days: 1, rowsPerDay: 1, seed: 1 })
    expect(sql).toMatch(/^-- File: scale-12-months\.sql/)
    expect(sql).toMatch(/-- Purpose: 12-month audit_log volume fixture/)
    expect(sql).toMatch(/-- Schema version: local-migration-009/)
    expect(sql).toMatch(/-- Regen script: scripts\/regen-scale-12-months\.ts/)
    expect(sql).toMatch(/-- DO NOT EDIT BY HAND/)
    expect(sql).toMatch(/^BEGIN;$/m)
    expect(sql).toMatch(/^COMMIT;$/m)
  })

  it("committed scale-12-months.sql matches the generator output for the smoke size", () => {
    const committed = readFileSync(
      "docs/idc-system/testing/fixtures/scale-12-months.sql",
      "utf8",
    )
    // The committed file uses the smoke knobs: days=30, rows_per_day=10,
    // seed=1. A drift here means someone hand-edited the SQL OR
    // changed the generator without re-running it -- either way the
    // committed artifact has fallen out of sync with the source of
    // truth.
    const fresh = generate({ days: 30, rowsPerDay: 10, seed: 1 })
    expect(committed).toBe(fresh)
  })

  it("generator output is line-stable (no trailing whitespace, LF line endings)", () => {
    const sql = generate({ days: 1, rowsPerDay: 1, seed: 1 })
    // No CR characters (LF-only).
    expect(sql).not.toMatch(/\r/)
    // No trailing whitespace on any line.
    for (const line of sql.split("\n")) {
      expect(line).toBe(line.trimEnd())
    }
  })
})
