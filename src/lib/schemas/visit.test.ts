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

  it("parses minimal payload with default dye, report and dalal flags", () => {
    const parsed = VisitCreateDraftSchema.parse(base)
    expect(parsed.dye).toBe(false)
    expect(parsed.report).toBe(false)
    expect(parsed.dalal).toBe(false)
  })

  it("accepts dalal mode without a doctor", () => {
    const parsed = VisitCreateDraftSchema.parse({ ...base, dalal: true })
    expect(parsed.dalal).toBe(true)
  })

  it("rejects dalal combined with a referring doctor", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({
        ...base,
        dalal: true,
        doctor_id: "01913d3a-7c70-7c00-a000-000000000099",
      })
    ).toThrow(/doctor_and_dalal_exclusive/)
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

  it("accepts a mandoub alongside a referring doctor", () => {
    const parsed = VisitCreateDraftSchema.parse({
      ...base,
      doctor_id: "01913d3a-7c70-7c00-a000-000000000099",
      mandoub_id: "01913d3a-7c70-7c00-a000-0000000000aa",
    })
    expect(parsed.mandoub_id).toBe("01913d3a-7c70-7c00-a000-0000000000aa")
  })

  it("rejects a mandoub without a referring doctor (house)", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({
        ...base,
        doctor_id: null,
        mandoub_id: "01913d3a-7c70-7c00-a000-0000000000aa",
      })
    ).toThrow(/mandoub_requires_doctor/)
  })

  it("rejects a mandoub combined with dalal mode", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({
        ...base,
        dalal: true,
        mandoub_id: "01913d3a-7c70-7c00-a000-0000000000aa",
      })
    ).toThrow(/mandoub_requires_doctor/)
  })

  it("defaults discount to false", () => {
    const parsed = VisitCreateDraftSchema.parse(base)
    expect(parsed.discount).toBe(false)
  })

  it("accepts a discount alongside a referring doctor", () => {
    const parsed = VisitCreateDraftSchema.parse({
      ...base,
      doctor_id: "01913d3a-7c70-7c00-a000-000000000099",
      discount: true,
    })
    expect(parsed.discount).toBe(true)
  })

  it("rejects a discount without a referring doctor (house)", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, doctor_id: null, discount: true })
    ).toThrow(/discount_requires_doctor/)
  })

  it("rejects a discount combined with dalal mode", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, dalal: true, discount: true })
    ).toThrow(/discount_requires_doctor/)
  })

  it("omits price_override_iqd when not provided", () => {
    const parsed = VisitCreateDraftSchema.parse(base)
    expect(parsed.price_override_iqd).toBeUndefined()
  })

  it("accepts an explicit null price_override_iqd", () => {
    const parsed = VisitCreateDraftSchema.parse({
      ...base,
      price_override_iqd: null,
    })
    expect(parsed.price_override_iqd).toBeNull()
  })

  it("accepts a non-negative integer price_override_iqd", () => {
    const parsed = VisitCreateDraftSchema.parse({
      ...base,
      price_override_iqd: 15000,
    })
    expect(parsed.price_override_iqd).toBe(15000)
  })

  it("rejects a negative price_override_iqd", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, price_override_iqd: -1 })
    ).toThrow()
  })

  it("rejects a non-integer price_override_iqd", () => {
    expect(() =>
      VisitCreateDraftSchema.parse({ ...base, price_override_iqd: 15000.5 })
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

  it("rejects dalal combined with a referring doctor", () => {
    expect(() =>
      VisitUpdateDraftSchema.parse({
        visit_id,
        dalal: true,
        doctor_id: "01913d3a-7c70-7c00-a000-000000000099",
      })
    ).toThrow(/doctor_and_dalal_exclusive/)
  })

  it("accepts clearing a mandoub with an explicit null", () => {
    const parsed = VisitUpdateDraftSchema.parse({ visit_id, mandoub_id: null })
    expect(parsed.mandoub_id).toBeNull()
  })

  it("rejects a mandoub patched without a referring doctor", () => {
    expect(() =>
      VisitUpdateDraftSchema.parse({
        visit_id,
        doctor_id: null,
        mandoub_id: "01913d3a-7c70-7c00-a000-0000000000aa",
      })
    ).toThrow(/mandoub_requires_doctor/)
  })

  it("accepts clearing price_override_iqd with an explicit null", () => {
    const parsed = VisitUpdateDraftSchema.parse({
      visit_id,
      price_override_iqd: null,
    })
    expect(parsed.price_override_iqd).toBeNull()
  })

  it("accepts patching price_override_iqd to a new value", () => {
    const parsed = VisitUpdateDraftSchema.parse({
      visit_id,
      price_override_iqd: 5000,
    })
    expect(parsed.price_override_iqd).toBe(5000)
  })

  it("rejects a negative price_override_iqd patch", () => {
    expect(() =>
      VisitUpdateDraftSchema.parse({ visit_id, price_override_iqd: -5 })
    ).toThrow()
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

  it.each([500, 1000] as const)("accepts a mandoub_cut of %i", (cut) => {
    const parsed = VisitLockSchema.parse({ visit_id, operator_id, mandoub_cut: cut })
    expect(parsed.mandoub_cut).toBe(cut)
  })

  it("rejects a mandoub_cut that is not 500 or 1000", () => {
    expect(() =>
      VisitLockSchema.parse({ visit_id, operator_id, mandoub_cut: 750 })
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
