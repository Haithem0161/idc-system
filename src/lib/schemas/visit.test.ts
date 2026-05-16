import { describe, expect, it } from "vitest"

import {
  VisitCreateDraftSchema,
  VisitLockSchema,
  VisitStatusSchema,
  VisitUpdateDraftSchema,
  VisitVoidSchema,
} from "./visit"

describe("VisitStatusSchema", () => {
  it.each(["draft", "locked", "voided"])(
    "accepts canonical literal %s",
    (value) => {
      expect(VisitStatusSchema.parse(value)).toBe(value)
    }
  )

  it("rejects unknown literal", () => {
    expect(() => VisitStatusSchema.parse("destroyed")).toThrow()
  })
})

describe("VisitCreateDraftSchema", () => {
  const base = {
    patient_id: "01913d3a-7c70-7c00-a000-000000000001",
    check_type_id: "01913d3a-7c70-7c00-a000-000000000002",
  }

  it("parses minimal payload with default dye and report flags", () => {
    const parsed = VisitCreateDraftSchema.parse(base)
    expect(parsed.dye).toBe(false)
    expect(parsed.report).toBe(false)
  })

  it("allows nullable check_subtype_id and doctor_id", () => {
    const parsed = VisitCreateDraftSchema.parse({
      ...base,
      check_subtype_id: null,
      doctor_id: null,
    })
    expect(parsed.check_subtype_id).toBeNull()
    expect(parsed.doctor_id).toBeNull()
  })

  it("rejects non-UUID patient_id", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, patient_id: "not-a-uuid" })
    ).toThrow()
  })

  it("rejects non-UUID check_type_id", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, check_type_id: "bad" })
    ).toThrow()
  })

  it("rejects non-boolean dye flag", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, dye: "yes" })
    ).toThrow()
  })
})

describe("VisitUpdateDraftSchema", () => {
  const visit_id = "01913d3a-7c70-7c00-a000-000000000003"

  it("accepts a partial patch with only one field", () => {
    const parsed = VisitUpdateDraftSchema.parse({ visit_id, dye: true })
    expect(parsed.dye).toBe(true)
    expect(parsed.report).toBeUndefined()
  })

  it("accepts explicit nulls for clearable references", () => {
    const parsed = VisitUpdateDraftSchema.parse({
      visit_id,
      doctor_id: null,
      check_subtype_id: null,
    })
    expect(parsed.doctor_id).toBeNull()
    expect(parsed.check_subtype_id).toBeNull()
  })

  it("rejects missing visit_id", () => {
    expect(() => VisitUpdateDraftSchema.parse({ dye: true })).toThrow()
  })
})

describe("VisitLockSchema", () => {
  const visit_id = "01913d3a-7c70-7c00-a000-000000000003"
  const operator_id = "01913d3a-7c70-7c00-a000-000000000004"

  it("parses canonical payload", () => {
    expect(VisitLockSchema.parse({ visit_id, operator_id })).toEqual({
      visit_id,
      operator_id,
    })
  })

  it("rejects missing operator_id", () => {
    expect(() => VisitLockSchema.parse({ visit_id })).toThrow()
  })

  it("rejects non-UUID operator_id", () => {
    expect(() =>
      VisitLockSchema.parse({ visit_id, operator_id: "x" })
    ).toThrow()
  })
})

describe("VisitVoidSchema", () => {
  const visit_id = "01913d3a-7c70-7c00-a000-000000000003"

  it("accepts a 5+ character reason and trims whitespace", () => {
    const parsed = VisitVoidSchema.parse({
      visit_id,
      reason: "  patient walked  ",
    })
    expect(parsed.reason).toBe("patient walked")
  })

  it("rejects a 4-char reason after trim", () => {
    expect(() =>
      VisitVoidSchema.parse({ visit_id, reason: "  oops " })
    ).toThrow(/void_reason_too_short/)
  })

  it("rejects a whitespace-only reason", () => {
    expect(() =>
      VisitVoidSchema.parse({ visit_id, reason: "        " })
    ).toThrow(/void_reason_too_short/)
  })
})
