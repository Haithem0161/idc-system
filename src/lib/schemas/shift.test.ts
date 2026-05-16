import { describe, expect, it } from "vitest"

import {
  ClockInInputSchema,
  ClockOutInputSchema,
  ShiftEditSchema,
  ShiftSchema,
  SoftDeleteShiftSchema,
} from "./shift"

const UUID_A = "0190f3a0-f1c0-7000-8000-000000000001"
const UUID_B = "0190f3a0-f1c0-7000-8000-000000000002"
const UUID_USER = "0190f3a0-f1c0-7000-8000-00000000abcd"

describe("ShiftSchema", () => {
  const baseRow = {
    id: UUID_A,
    operator_id: UUID_B,
    check_in_at: "2026-05-14T10:00:00Z",
    check_out_at: null,
    check_in_by_user_id: UUID_USER,
    check_out_by_user_id: null,
    note: null,
    created_at: "2026-05-14T10:00:00Z",
    updated_at: "2026-05-14T10:00:00Z",
    deleted_at: null,
    version: 1,
    entity_id: "tenant-x",
  }

  it("parses a minimal open-shift row", () => {
    const parsed = ShiftSchema.parse(baseRow)
    expect(parsed.id).toBe(UUID_A)
    expect(parsed.check_out_at).toBeNull()
  })

  it("parses a closed-shift row with note", () => {
    const closed = {
      ...baseRow,
      check_out_at: "2026-05-14T12:00:00Z",
      check_out_by_user_id: UUID_USER,
      note: "morning",
      version: 2,
    }
    const parsed = ShiftSchema.parse(closed)
    expect(parsed.check_out_at).toBe("2026-05-14T12:00:00Z")
    expect(parsed.note).toBe("morning")
  })

  it("rejects a non-UUID id with a path-typed error", () => {
    const result = ShiftSchema.safeParse({ ...baseRow, id: "not-a-uuid" })
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues.some((i) => i.path.includes("id"))).toBe(true)
    }
  })

  it("rejects a non-integer version", () => {
    const result = ShiftSchema.safeParse({ ...baseRow, version: 1.5 })
    expect(result.success).toBe(false)
  })
})

describe("ClockInInputSchema", () => {
  it("accepts the minimal { operator_id } shape", () => {
    const parsed = ClockInInputSchema.parse({ operator_id: UUID_A })
    expect(parsed.operator_id).toBe(UUID_A)
    expect(parsed.note).toBeUndefined()
  })

  it("accepts null note", () => {
    const parsed = ClockInInputSchema.parse({ operator_id: UUID_A, note: null })
    expect(parsed.note).toBeNull()
  })

  it("accepts a non-empty note up to 1024 chars", () => {
    const note = "x".repeat(1024)
    const parsed = ClockInInputSchema.parse({ operator_id: UUID_A, note })
    expect(parsed.note?.length).toBe(1024)
  })

  it("rejects note over 1024 chars", () => {
    const result = ClockInInputSchema.safeParse({
      operator_id: UUID_A,
      note: "x".repeat(1025),
    })
    expect(result.success).toBe(false)
  })

  it("rejects malformed operator_id", () => {
    const result = ClockInInputSchema.safeParse({ operator_id: "not-a-uuid" })
    expect(result.success).toBe(false)
  })
})

describe("ClockOutInputSchema", () => {
  it("accepts a valid uuid shift_id", () => {
    expect(ClockOutInputSchema.parse({ shift_id: UUID_A }).shift_id).toBe(UUID_A)
  })

  it("rejects a malformed shift_id", () => {
    const result = ClockOutInputSchema.safeParse({ shift_id: "bad" })
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues.some((i) => i.path.includes("shift_id"))).toBe(true)
    }
  })
})

describe("ShiftEditSchema", () => {
  const base = {
    shift_id: UUID_A,
    check_in_at: "2026-05-14T10:00:00+00:00",
    check_out_at: "2026-05-14T12:00:00+00:00",
  }

  it("parses a valid edit payload", () => {
    expect(ShiftEditSchema.parse(base).shift_id).toBe(UUID_A)
  })

  it("rejects when check_out_at is earlier than check_in_at", () => {
    const result = ShiftEditSchema.safeParse({
      ...base,
      check_in_at: "2026-05-14T12:00:00+00:00",
      check_out_at: "2026-05-14T10:00:00+00:00",
    })
    expect(result.success).toBe(false)
    if (!result.success) {
      const issue = result.error.issues[0]
      expect(issue.path).toContain("check_out_at")
      expect(issue.message).toMatch(/check_out_at must be >= check_in_at/)
    }
  })

  it("allows check_out_at to be omitted (reopen edit)", () => {
    const parsed = ShiftEditSchema.parse({
      shift_id: UUID_A,
      check_in_at: "2026-05-14T10:00:00+00:00",
    })
    expect(parsed.check_out_at).toBeUndefined()
  })

  it("accepts note: { value: null } to clear", () => {
    const parsed = ShiftEditSchema.parse({
      ...base,
      note: { value: null },
    })
    expect(parsed.note).toEqual({ value: null })
  })

  it("accepts note: { value: 'x' } to replace", () => {
    const parsed = ShiftEditSchema.parse({
      ...base,
      note: { value: "evening" },
    })
    expect(parsed.note).toEqual({ value: "evening" })
  })
})

describe("SoftDeleteShiftSchema", () => {
  it("accepts a non-empty reason", () => {
    expect(
      SoftDeleteShiftSchema.parse({
        shift_id: UUID_A,
        reason: "duplicate shift",
      }).reason
    ).toBe("duplicate shift")
  })

  it("rejects an empty reason", () => {
    const result = SoftDeleteShiftSchema.safeParse({
      shift_id: UUID_A,
      reason: "",
    })
    expect(result.success).toBe(false)
  })

  it("rejects a reason over 512 chars", () => {
    const result = SoftDeleteShiftSchema.safeParse({
      shift_id: UUID_A,
      reason: "x".repeat(513),
    })
    expect(result.success).toBe(false)
  })
})
