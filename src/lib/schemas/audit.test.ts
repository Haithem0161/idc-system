// Phase-08 §1.2 schema unit tests for the audit + diagnostics surface.
// Pure Zod parsing: enum exhaustiveness, defaults, validation errors.

import { describe, expect, it } from "vitest"

import {
  AUDIT_ACTIONS,
  AUDIT_ENTITIES,
  AuditFilterSchema,
  AuditPageSchema,
  AuditQueryModeSchema,
  AuditRowSchema,
  AuditSourceSchema,
  DiagnosticsSummarySchema,
} from "@/lib/schemas/audit"

describe("Phase-08 §1.2 AUDIT_ACTIONS", () => {
  it("enumerates all actions incl. the signed-close actions", () => {
    expect(AUDIT_ACTIONS).toEqual([
      "create",
      "update",
      "soft_delete",
      "lock",
      "void",
      "discard",
      "clock_in",
      "clock_out",
      "password_change",
      "login",
      "logout",
      "conflict_resolve",
      "vacuum",
      "daily_close_run",
      "daily_close_sign",
      "daily_close_reopen",
    ])
  })

  it("has exactly 16 entries", () => {
    expect(AUDIT_ACTIONS).toHaveLength(16)
  })

  it("includes vacuum and the daily-close actions", () => {
    expect(AUDIT_ACTIONS).toContain("vacuum")
    expect(AUDIT_ACTIONS).toContain("daily_close_run")
    expect(AUDIT_ACTIONS).toContain("daily_close_sign")
    expect(AUDIT_ACTIONS).toContain("daily_close_reopen")
  })

  it("includes conflict_resolve for phase-08 resolver round-trip", () => {
    expect(AUDIT_ACTIONS).toContain("conflict_resolve")
  })
})

describe("Phase-08 §1.2 AUDIT_ENTITIES", () => {
  it("enumerates all entity tables incl. daily_close", () => {
    expect(AUDIT_ENTITIES).toEqual([
      "users",
      "settings",
      "check_types",
      "check_subtypes",
      "doctors",
      "doctor_check_pricing",
      "operators",
      "operator_specialties",
      "operator_shifts",
      "mandoubs",
      "patients",
      "visits",
      "inventory_items",
      "inventory_consumption_map",
      "inventory_adjustments",
      "audit_log",
      "daily_close",
    ])
  })

  it("has exactly 17 entries", () => {
    expect(AUDIT_ENTITIES).toHaveLength(17)
  })

  it("includes audit_log so the vacuum self-row can drill down", () => {
    expect(AUDIT_ENTITIES).toContain("audit_log")
  })
})

describe("Phase-08 §1.2 AuditFilterSchema", () => {
  it("accepts an empty payload (all fields optional)", () => {
    const parsed = AuditFilterSchema.parse({})
    expect(parsed).toEqual({})
  })

  it("parses a full filter shape", () => {
    const parsed = AuditFilterSchema.parse({
      actor_user_id: "0190f3a0-f1c0-7000-8000-000000000abc",
      action: "lock",
      entity: "visits",
      entity_id_prefix: "abcd",
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T23:59:59Z",
      text: "duplicate",
      limit: 50,
      offset: 0,
    })
    expect(parsed.action).toBe("lock")
    expect(parsed.entity).toBe("visits")
    expect(parsed.entity_id_prefix).toBe("abcd")
  })

  it("rejects entity_id_prefix shorter than 4 chars", () => {
    expect(() =>
      AuditFilterSchema.parse({ entity_id_prefix: "abc" }),
    ).toThrow()
  })

  it("accepts entity_id_prefix of exactly 4 chars", () => {
    const parsed = AuditFilterSchema.parse({ entity_id_prefix: "abcd" })
    expect(parsed.entity_id_prefix).toBe("abcd")
  })

  it("rejects entity_id_prefix longer than 36 chars", () => {
    expect(() =>
      AuditFilterSchema.parse({
        entity_id_prefix: "0".repeat(37),
      }),
    ).toThrow()
  })

  it("accepts entity_id_prefix of exactly 36 chars (full UUID)", () => {
    const parsed = AuditFilterSchema.parse({
      entity_id_prefix: "abcdefab-1234-5678-9abc-def012345678",
    })
    expect(parsed.entity_id_prefix).toHaveLength(36)
  })

  it("rejects free text shorter than 2 chars", () => {
    expect(() => AuditFilterSchema.parse({ text: "a" })).toThrow()
  })

  it("accepts free text of exactly 2 chars", () => {
    const parsed = AuditFilterSchema.parse({ text: "ab" })
    expect(parsed.text).toBe("ab")
  })

  it("rejects free text longer than 100 chars", () => {
    expect(() => AuditFilterSchema.parse({ text: "x".repeat(101) })).toThrow()
  })

  it("rejects an unknown action enum value", () => {
    expect(() =>
      AuditFilterSchema.parse({ action: "renamed_action" }),
    ).toThrow()
  })

  it("rejects an unknown entity enum value", () => {
    expect(() =>
      AuditFilterSchema.parse({ entity: "renamed_entity" }),
    ).toThrow()
  })

  it("rejects limit below 1", () => {
    expect(() => AuditFilterSchema.parse({ limit: 0 })).toThrow()
  })

  it("rejects limit above 100 (§11.4 cap)", () => {
    expect(() => AuditFilterSchema.parse({ limit: 101 })).toThrow()
  })

  it("rejects negative offset", () => {
    expect(() => AuditFilterSchema.parse({ offset: -1 })).toThrow()
  })

  it("accepts limit at the boundary values 1 and 100", () => {
    expect(AuditFilterSchema.parse({ limit: 1 }).limit).toBe(1)
    expect(AuditFilterSchema.parse({ limit: 100 }).limit).toBe(100)
  })

  it("requires actor_user_id to be a UUID", () => {
    expect(() => AuditFilterSchema.parse({ actor_user_id: "not-a-uuid" })).toThrow()
  })

  it("requires from_utc/to_utc to be ISO 8601 with offset", () => {
    expect(() => AuditFilterSchema.parse({ from_utc: "2026-05-01" })).toThrow()
    expect(() => AuditFilterSchema.parse({ to_utc: "2026-05-01 00:00:00" })).toThrow()
  })
})

describe("Phase-08 §1.2 AuditSourceSchema", () => {
  it("accepts local and server", () => {
    expect(AuditSourceSchema.parse("local")).toBe("local")
    expect(AuditSourceSchema.parse("server")).toBe("server")
  })

  it("rejects mixed-case typos", () => {
    expect(() => AuditSourceSchema.parse("LOCAL")).toThrow()
    expect(() => AuditSourceSchema.parse("Server")).toThrow()
  })

  it("rejects renamed variants", () => {
    expect(() => AuditSourceSchema.parse("remote")).toThrow()
  })
})

describe("Phase-08 §1.2 AuditQueryModeSchema", () => {
  it("accepts all 3 modes", () => {
    expect(AuditQueryModeSchema.parse("local")).toBe("local")
    expect(AuditQueryModeSchema.parse("server")).toBe("server")
    expect(AuditQueryModeSchema.parse("merged")).toBe("merged")
  })

  it("rejects renamed variants", () => {
    expect(() => AuditQueryModeSchema.parse("hybrid")).toThrow()
  })
})

describe("Phase-08 §1.2 AuditRowSchema", () => {
  const valid = {
    id: "row-1",
    at: "2026-05-13T12:00:00Z",
    actor_user_id: "0190f3a0-f1c0-7000-8000-000000000abc",
    action: "create",
    entity: "doctors",
    entity_id: "ent-1",
    delta: { k: "v" },
    device_id: "device-1",
    version: 1,
    dirty: false,
    source: "local",
  }

  it("parses a row with the full set of keys", () => {
    const parsed = AuditRowSchema.parse(valid)
    expect(parsed.dirty).toBe(false)
    expect(parsed.source).toBe("local")
  })

  it("preserves the dirty flag from the Rust serialisation per §7.15", () => {
    const parsed = AuditRowSchema.parse({ ...valid, dirty: true })
    expect(parsed.dirty).toBe(true)
  })

  it("accepts arbitrary delta shapes (unknown)", () => {
    const parsed = AuditRowSchema.parse({
      ...valid,
      delta: { nested: { a: 1, b: [true, false] } },
    })
    expect(parsed.delta).toEqual({ nested: { a: 1, b: [true, false] } })
  })

  it("rejects missing source", () => {
    const { source: _omit, ...rest } = valid
    void _omit
    expect(() => AuditRowSchema.parse(rest)).toThrow()
  })

  it("rejects non-integer version", () => {
    expect(() => AuditRowSchema.parse({ ...valid, version: 1.5 })).toThrow()
  })

  it("rejects non-boolean dirty", () => {
    expect(() => AuditRowSchema.parse({ ...valid, dirty: 1 })).toThrow()
  })
})

describe("Phase-08 §1.2 AuditPageSchema", () => {
  const row = {
    id: "row-1",
    at: "2026-05-13T12:00:00Z",
    actor_user_id: "0190f3a0-f1c0-7000-8000-000000000abc",
    action: "create",
    entity: "doctors",
    entity_id: "ent-1",
    delta: { k: "v" },
    device_id: "device-1",
    version: 1,
    dirty: false,
    source: "local" as const,
  }

  it("parses an empty page", () => {
    const parsed = AuditPageSchema.parse({
      rows: [],
      mode: "local",
      next_offset: null,
    })
    expect(parsed.rows).toHaveLength(0)
    expect(parsed.next_offset).toBeNull()
  })

  it("parses a page with rows and a numeric next_offset", () => {
    const parsed = AuditPageSchema.parse({
      rows: [row],
      mode: "merged",
      next_offset: 50,
    })
    expect(parsed.rows).toHaveLength(1)
    expect(parsed.mode).toBe("merged")
    expect(parsed.next_offset).toBe(50)
  })

  it("rejects non-null and non-number next_offset values", () => {
    expect(() =>
      AuditPageSchema.parse({
        rows: [],
        mode: "local",
        next_offset: "not-a-number" as unknown as number,
      }),
    ).toThrow()
  })

  it("rejects rows with invalid sub-shape", () => {
    expect(() =>
      AuditPageSchema.parse({
        rows: [{ ...row, source: "bogus" }],
        mode: "local",
        next_offset: null,
      }),
    ).toThrow()
  })
})

describe("Phase-08 §1.2 DiagnosticsSummarySchema", () => {
  it("parses a fresh-install all-null summary", () => {
    const parsed = DiagnosticsSummarySchema.parse({
      lock_latency_p95_ms: null,
      outbox_depth: 0,
      last_sync_at: null,
      conflict_count_7d: 0,
      receipt_print_success_rate_30d: null,
    })
    expect(parsed.outbox_depth).toBe(0)
    expect(parsed.conflict_count_7d).toBe(0)
  })

  it("parses a populated summary", () => {
    const parsed = DiagnosticsSummarySchema.parse({
      lock_latency_p95_ms: 95,
      outbox_depth: 12,
      last_sync_at: "2026-05-13T12:00:00Z",
      conflict_count_7d: 3,
      receipt_print_success_rate_30d: 0.987,
    })
    expect(parsed.lock_latency_p95_ms).toBe(95)
    expect(parsed.receipt_print_success_rate_30d).toBe(0.987)
  })

  it("rejects negative outbox_depth", () => {
    expect(() =>
      DiagnosticsSummarySchema.parse({
        lock_latency_p95_ms: null,
        outbox_depth: -1,
        last_sync_at: null,
        conflict_count_7d: 0,
        receipt_print_success_rate_30d: null,
      }),
    ).toThrow()
  })

  it("rejects negative conflict_count_7d", () => {
    expect(() =>
      DiagnosticsSummarySchema.parse({
        lock_latency_p95_ms: null,
        outbox_depth: 0,
        last_sync_at: null,
        conflict_count_7d: -1,
        receipt_print_success_rate_30d: null,
      }),
    ).toThrow()
  })

  it("requires lock_latency_p95_ms to be an integer when present", () => {
    expect(() =>
      DiagnosticsSummarySchema.parse({
        lock_latency_p95_ms: 1.5,
        outbox_depth: 0,
        last_sync_at: null,
        conflict_count_7d: 0,
        receipt_print_success_rate_30d: null,
      }),
    ).toThrow()
  })

  it("permits fractional receipt_print_success_rate_30d", () => {
    const parsed = DiagnosticsSummarySchema.parse({
      lock_latency_p95_ms: 80,
      outbox_depth: 0,
      last_sync_at: null,
      conflict_count_7d: 0,
      receipt_print_success_rate_30d: 0.6364,
    })
    expect(parsed.receipt_print_success_rate_30d).toBeCloseTo(0.6364)
  })
})
