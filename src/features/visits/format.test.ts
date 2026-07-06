import { describe, expect, it } from "vitest"

import { computeRunningTotal, formatVisitTotal } from "./format"

describe("formatVisitTotal", () => {
  it("renders ASCII digits by default", () => {
    expect(formatVisitTotal(1234)).toBe("1234")
  })

  it("renders Arabic-Indic digits when arabicNumerals is true", () => {
    expect(formatVisitTotal(1234, { arabicNumerals: true })).toBe("١٢٣٤")
  })

  it("renders zero", () => {
    expect(formatVisitTotal(0)).toBe("0")
    expect(formatVisitTotal(0, { arabicNumerals: true })).toBe("٠")
  })

  it("throws on non-integer amounts", () => {
    expect(() => formatVisitTotal(1.5)).toThrow(/integer/)
  })

  it("throws on non-finite amounts", () => {
    expect(() => formatVisitTotal(Number.NaN)).toThrow(/finite/)
    expect(() => formatVisitTotal(Number.POSITIVE_INFINITY)).toThrow(/finite/)
  })

  it("renders negative integers correctly", () => {
    expect(formatVisitTotal(-3)).toBe("-3")
    expect(formatVisitTotal(-3, { arabicNumerals: true })).toBe("-٣")
  })
})

describe("computeRunningTotal", () => {
  const baseInputs = {
    base_price_iqd: 50_000,
    operator_base_cut_iqd: 5_000,
    dye: false,
    dye_price_iqd: 2_000,
    report: false,
    report_pct: 20,
    internal_doctor_pct: 40,
    dalal: false,
  }

  it("flat house mode with no dye or report", () => {
    const snap = computeRunningTotal(baseInputs)
    expect(snap.price_iqd).toBe(50_000)
    expect(snap.dye_cost_iqd).toBe(0)
    expect(snap.report_amount_iqd).toBe(0)
    expect(snap.doctor_cut_iqd).toBe(20_000)
    expect(snap.operator_cut_iqd).toBe(5_000)
    expect(snap.internal_pct).toBe(40)
    expect(snap.patient_total_iqd).toBe(50_000)
  })

  it("adds dye cost only when dye is true and supported", () => {
    expect(computeRunningTotal({ ...baseInputs, dye: true }).dye_cost_iqd).toBe(
      2_000
    )
    expect(computeRunningTotal({ ...baseInputs, dye: false }).dye_cost_iqd).toBe(
      0
    )
  })

  it("house dye visit: cuts run on price (collected price+dye, dye subtracted)", () => {
    // collected defaults to price + dye = 50_000 + 2_000 = 52_000.
    // cutBase = 52_000 - 2_000 = 50_000. Doctor 40% = 20_000.
    const snap = computeRunningTotal({ ...baseInputs, dye: true })
    expect(snap.dye_cost_iqd).toBe(2_000)
    expect(snap.doctor_cut_iqd).toBe(20_000)
    expect(snap.patient_total_iqd).toBe(52_000)
  })

  it("underpayment: dye taken first, cuts scale off the remaining service", () => {
    // price 50_000, dye 2_000, patient pays 30_000 total.
    // cutBase = max(0, 30_000 - 2_000) = 28_000. Doctor 40% = 11_200.
    const snap = computeRunningTotal({
      ...baseInputs,
      dye: true,
      amount_paid_override_iqd: 30_000,
    })
    expect(snap.doctor_cut_iqd).toBe(11_200)
  })

  it("underpayment below the dye price zeroes every cut", () => {
    // pays 1_500 < dye 2_000 -> cutBase = 0 -> all cuts zero.
    const snap = computeRunningTotal({
      ...baseInputs,
      dye: true,
      amount_paid_override_iqd: 1_500,
    })
    expect(snap.doctor_cut_iqd).toBe(0)
    expect(snap.operator_cut_iqd).toBe(0)
    expect(snap.report_amount_iqd).toBe(0)
  })

  it("report floors at zero when a fixed doctor cut exceeds the cut base", () => {
    // Fixed cut 60_000 > cutBase 50_000. Report base = max(0, 50_000 - 60_000)
    // = 0, so report is 0 (never negative). Mirrors the Rust engine.
    const snap = computeRunningTotal({
      ...baseInputs,
      report: true,
      doctor_pricing: { cut_kind: "fixed", cut_value: 60_000 },
    })
    expect(snap.doctor_cut_iqd).toBe(60_000)
    expect(snap.report_amount_iqd).toBe(0)
  })

  it("throws when dye is requested but no dye price is resolved", () => {
    expect(() =>
      computeRunningTotal({
        ...baseInputs,
        dye: true,
        dye_price_iqd: null,
      })
    ).toThrow(/dye/)
  })

  it("report is internal-only: a pct of (price - doctor cut), never in the patient total", () => {
    // House mode: doctor cut = 50_000 * 40% = 20_000.
    // Report = (50_000 - 20_000) * 20% = 6_000. Patient total stays price + dye.
    const snap = computeRunningTotal({ ...baseInputs, report: true })
    expect(snap.report_amount_iqd).toBe(6_000)
    expect(snap.patient_total_iqd).toBe(50_000)
  })

  it("report is zero when the report flag is off", () => {
    expect(computeRunningTotal(baseInputs).report_amount_iqd).toBe(0)
  })

  it("dalal applies a flat 10,000 IQD doctor cut and skips house pct", () => {
    const snap = computeRunningTotal({ ...baseInputs, dalal: true })
    expect(snap.doctor_cut_iqd).toBe(10_000)
    expect(snap.internal_pct).toBeNull()
    // Report carves out of (price - dalal cut): (50_000 - 10_000) * 20% = 8_000.
    const withReport = computeRunningTotal({
      ...baseInputs,
      dalal: true,
      report: true,
    })
    expect(withReport.report_amount_iqd).toBe(8_000)
    expect(withReport.patient_total_iqd).toBe(50_000)
  })

  it("uses subtype price when provided", () => {
    const snap = computeRunningTotal({
      ...baseInputs,
      subtype_price_iqd: 70_000,
    })
    expect(snap.price_iqd).toBe(70_000)
    expect(snap.patient_total_iqd).toBe(70_000)
  })

  it("doctor flat-cut overrides internal_pct and pricing kind", () => {
    const snap = computeRunningTotal({
      ...baseInputs,
      doctor_pricing: {
        cut_kind: "fixed",
        cut_value: 12_500,
      },
    })
    expect(snap.doctor_cut_iqd).toBe(12_500)
    expect(snap.internal_pct).toBeNull()
  })

  it("doctor percentage cut rounds with Math.floor", () => {
    const snap = computeRunningTotal({
      ...baseInputs,
      base_price_iqd: 1_000_037,
      doctor_pricing: {
        cut_kind: "pct",
        cut_value: 25,
      },
    })
    expect(snap.doctor_cut_iqd).toBe(250_009)
    expect(snap.internal_pct).toBeNull()
  })

  it("doctor price_override replaces base price", () => {
    const snap = computeRunningTotal({
      ...baseInputs,
      doctor_pricing: {
        cut_kind: "pct",
        cut_value: 10,
        price_override_iqd: 200_000,
      },
    })
    expect(snap.price_iqd).toBe(200_000)
    expect(snap.doctor_cut_iqd).toBe(20_000)
  })

  it("patient total equals price + dye and excludes the report", () => {
    const snap = computeRunningTotal({
      ...baseInputs,
      dye: true,
      report: true,
    })
    expect(snap.patient_total_iqd).toBe(snap.price_iqd + snap.dye_cost_iqd)
    expect(snap.report_amount_iqd).toBeGreaterThan(0)
  })

  it("throws on doctor percentage out of range", () => {
    expect(() =>
      computeRunningTotal({
        ...baseInputs,
        doctor_pricing: { cut_kind: "pct", cut_value: 150 },
      })
    ).toThrow(/percentage/)
  })

  it("throws on internal_doctor_pct out of range in house mode", () => {
    expect(() =>
      computeRunningTotal({ ...baseInputs, internal_doctor_pct: 250 })
    ).toThrow(/internal_doctor_pct/)
  })

  it("matches Rust port for canonical inputs (parity test)", () => {
    // Mirror of Rust money_math::tests::percentage_rounds_consistently:
    // 1_000_037 * 25 / 100 = 250_009 under integer truncation.
    const snap = computeRunningTotal({
      ...baseInputs,
      base_price_iqd: 1_000_037,
      doctor_pricing: { cut_kind: "pct", cut_value: 25 },
    })
    expect(snap.doctor_cut_iqd).toBe(250_009)
    // House-mode rounding: 75_000 * 30 / 100 = 22_500 (matches persona).
    const house = computeRunningTotal({
      ...baseInputs,
      base_price_iqd: 75_000,
      internal_doctor_pct: 30,
    })
    expect(house.doctor_cut_iqd).toBe(22_500)
  })
})
