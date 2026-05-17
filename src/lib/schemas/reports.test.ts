// Phase-07 §1.2 schema unit tests for the reports surface.
// Pure Zod parsing: enum exhaustiveness, defaults, validation errors.

import { describe, expect, it } from "vitest"

import {
  dailyCloseInputSchema,
  dateRangeSchema,
  visitsReportFiltersSchema,
  visitsReportGroupByValues,
} from "@/lib/schemas/reports"

describe("Phase-07 §1.2 visitsReportGroupByValues", () => {
  it("enumerates all 7 group-by modes from §7.14", () => {
    expect(visitsReportGroupByValues).toEqual([
      "none",
      "by_date",
      "by_doctor",
      "by_operator",
      "by_check_type",
      "by_subtype",
      "by_status",
    ])
  })

  it("has exactly 7 values", () => {
    expect(visitsReportGroupByValues).toHaveLength(7)
  })

  it("includes none as the default mode", () => {
    expect(visitsReportGroupByValues).toContain("none")
  })
})

describe("Phase-07 §1.2 dateRangeSchema", () => {
  it("parses a minimal range with ISO strings", () => {
    const parsed = dateRangeSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
    })
    expect(parsed.from_utc).toBe("2026-05-01T00:00:00Z")
    expect(parsed.to_utc).toBe("2026-05-13T00:00:00Z")
  })

  it("accepts an optional include_voided flag", () => {
    const parsed = dateRangeSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      include_voided: true,
    })
    expect(parsed.include_voided).toBe(true)
  })

  it("rejects missing from_utc", () => {
    const result = dateRangeSchema.safeParse({
      to_utc: "2026-05-13T00:00:00Z",
    })
    expect(result.success).toBe(false)
  })

  it("rejects missing to_utc", () => {
    const result = dateRangeSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
    })
    expect(result.success).toBe(false)
  })
})

describe("Phase-07 §1.2 visitsReportFiltersSchema", () => {
  it("parses with only required fields (from + to)", () => {
    const parsed = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
    })
    expect(parsed.from_utc).toBe("2026-05-01T00:00:00Z")
    expect(parsed.to_utc).toBe("2026-05-13T00:00:00Z")
  })

  it("accepts all 7 group_by modes", () => {
    for (const mode of visitsReportGroupByValues) {
      const parsed = visitsReportFiltersSchema.parse({
        from_utc: "2026-05-01T00:00:00Z",
        to_utc: "2026-05-13T00:00:00Z",
        group_by: mode,
      })
      expect(parsed.group_by).toBe(mode)
    }
  })

  it("rejects an unknown group_by value", () => {
    const result = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      group_by: "by_phase_of_the_moon",
    })
    expect(result.success).toBe(false)
  })

  it("accepts include_voided + include_house toggles", () => {
    const parsed = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      include_voided: true,
      include_house: true,
    })
    expect(parsed.include_voided).toBe(true)
    expect(parsed.include_house).toBe(true)
  })

  it("accepts arrays of UUIDs for catalog filters", () => {
    const ct = "01900000-0000-7000-8000-000000000001"
    const op = "01900000-0000-7000-8000-000000000002"
    const parsed = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      check_type_ids: [ct],
      operator_ids: [op],
    })
    expect(parsed.check_type_ids).toEqual([ct])
    expect(parsed.operator_ids).toEqual([op])
  })

  it("rejects non-UUID strings in id arrays", () => {
    const result = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      doctor_ids: ["not-a-uuid"],
    })
    expect(result.success).toBe(false)
  })

  it("accepts nullable dye / report y|n|all semantics", () => {
    const yes = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      dye: true,
      report: false,
    })
    expect(yes.dye).toBe(true)
    expect(yes.report).toBe(false)
    const both = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      dye: null,
      report: null,
    })
    expect(both.dye).toBe(null)
    expect(both.report).toBe(null)
  })

  it("caps the limit at 10_000 per §7.24", () => {
    const ok = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      limit: 10_000,
    })
    expect(ok.success).toBe(true)
    const too_many = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      limit: 10_001,
    })
    expect(too_many.success).toBe(false)
  })

  it("rejects zero / negative limits", () => {
    const zero = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      limit: 0,
    })
    expect(zero.success).toBe(false)
    const negative = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      limit: -5,
    })
    expect(negative.success).toBe(false)
  })

  it("rejects fractional limits (must be integer)", () => {
    const result = visitsReportFiltersSchema.safeParse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      limit: 1.5,
    })
    expect(result.success).toBe(false)
  })

  it("accepts custom statuses list", () => {
    const parsed = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      statuses: ["locked", "voided"],
    })
    expect(parsed.statuses).toEqual(["locked", "voided"])
  })

  it("preserves subtype_ids when present", () => {
    const stid = "01900000-0000-7000-8000-000000000003"
    const parsed = visitsReportFiltersSchema.parse({
      from_utc: "2026-05-01T00:00:00Z",
      to_utc: "2026-05-13T00:00:00Z",
      subtype_ids: [stid],
    })
    expect(parsed.subtype_ids).toEqual([stid])
  })
})

describe("Phase-07 §1.2 dailyCloseInputSchema", () => {
  it("accepts an ISO YYYY-MM-DD date", () => {
    const parsed = dailyCloseInputSchema.parse({ date: "2026-05-13" })
    expect(parsed.date).toBe("2026-05-13")
  })

  it("rejects a non-ISO date format", () => {
    const result = dailyCloseInputSchema.safeParse({ date: "13/05/2026" })
    expect(result.success).toBe(false)
  })

  it("rejects a partial date (missing day)", () => {
    const result = dailyCloseInputSchema.safeParse({ date: "2026-05" })
    expect(result.success).toBe(false)
  })

  it("rejects an RFC3339 timestamp", () => {
    const result = dailyCloseInputSchema.safeParse({
      date: "2026-05-13T00:00:00Z",
    })
    expect(result.success).toBe(false)
  })

  it("rejects empty string", () => {
    const result = dailyCloseInputSchema.safeParse({ date: "" })
    expect(result.success).toBe(false)
  })

  it("rejects missing date field", () => {
    const result = dailyCloseInputSchema.safeParse({})
    expect(result.success).toBe(false)
  })

  it("accepts the year boundary 2026-12-31", () => {
    const parsed = dailyCloseInputSchema.parse({ date: "2026-12-31" })
    expect(parsed.date).toBe("2026-12-31")
  })

  it("accepts the year start 2026-01-01", () => {
    const parsed = dailyCloseInputSchema.parse({ date: "2026-01-01" })
    expect(parsed.date).toBe("2026-01-01")
  })

  it("accepts a leap-day style date (regex permits any 2-digit M/D)", () => {
    // The schema is regex-only -- it does not validate calendar correctness;
    // calendar checks are enforced server-side by NaiveDate::parse_from_str.
    const parsed = dailyCloseInputSchema.parse({ date: "2024-02-29" })
    expect(parsed.date).toBe("2024-02-29")
  })
})
