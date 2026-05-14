import { describe, it, expect } from "vitest"

import { formatHours, formatIqd, formatPermille } from "./money"

describe("formatIqd", () => {
  it("groups thousands using the en-GB locale by default", () => {
    expect(formatIqd(1_234)).toBe("1,234")
    expect(formatIqd(0)).toBe("0")
    expect(formatIqd(10_000)).toBe("10,000")
  })

  it("renders Arabic-Indic digits when arabicNumerals is true", () => {
    expect(formatIqd(10_000, { arabicNumerals: true })).toBe("١٠,٠٠٠")
    expect(formatIqd(1_234, { arabicNumerals: true })).toBe("١,٢٣٤")
  })

  it("appends the `IQD` suffix when withSuffix is true", () => {
    expect(formatIqd(1_234, { withSuffix: true })).toBe("1,234 IQD")
    expect(formatIqd(1_234, { withSuffix: true, arabicNumerals: true })).toBe("١,٢٣٤ IQD")
  })

  it("truncates fractional input (IQD is integer in v1)", () => {
    expect(formatIqd(1_234.99)).toBe("1,234")
    expect(formatIqd(-1_234.5)).toBe("-1,234")
  })

  it("honours an explicit locale override", () => {
    expect(formatIqd(1_234_567, { locale: "en-US" })).toBe("1,234,567")
  })
})

describe("formatPermille", () => {
  it("renders zero exactly as `0.0%` (or Arabic-Indic equivalent)", () => {
    expect(formatPermille(0)).toBe("0.0%")
    expect(formatPermille(0, { arabicNumerals: true })).toBe("٠.٠٪")
  })

  it("renders positive permille with `+` prefix and one fractional digit", () => {
    expect(formatPermille(14)).toBe("+1.4%")
    expect(formatPermille(125)).toBe("+12.5%")
  })

  it("renders negative permille with `-` prefix", () => {
    expect(formatPermille(-14)).toBe("-1.4%")
  })

  it("renders Arabic-Indic digits when arabicNumerals is true", () => {
    expect(formatPermille(14, { arabicNumerals: true })).toBe("+١.٤%")
  })
})

describe("formatHours", () => {
  it("renders milliseconds as fractional hours rounded to one digit", () => {
    expect(formatHours(3_600_000)).toBe("1.0h")
    expect(formatHours(5_400_000)).toBe("1.5h")
    expect(formatHours(900_000)).toBe("0.3h")
  })

  it("collapses non-positive durations to a literal zero", () => {
    expect(formatHours(0)).toBe("0.0h")
    expect(formatHours(-1)).toBe("0.0h")
  })

  it("renders Arabic-Indic digits when arabicNumerals is true", () => {
    expect(formatHours(3_600_000, { arabicNumerals: true })).toBe("١.٠h")
    expect(formatHours(0, { arabicNumerals: true })).toBe("٠.٠h")
  })
})
