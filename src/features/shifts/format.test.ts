import { describe, expect, it } from "vitest"

import { formatShiftDuration } from "./format"

describe("formatShiftDuration", () => {
  it("returns Xh Ym for a closed shift", () => {
    const out = formatShiftDuration({
      check_in_at: "2026-05-14T10:00:00Z",
      check_out_at: "2026-05-14T12:14:00Z",
    })
    expect(out).toBe("2h 14m")
  })

  it("returns -- while the shift is still open", () => {
    expect(
      formatShiftDuration({
        check_in_at: "2026-05-14T10:00:00Z",
        check_out_at: null,
      })
    ).toBe("--")
  })

  it("truncates seconds to whole minutes", () => {
    expect(
      formatShiftDuration({
        check_in_at: "2026-05-14T10:00:00Z",
        check_out_at: "2026-05-14T10:00:59Z",
      })
    ).toBe("0h 0m")
  })

  it("supports multi-hour spans crossing the day boundary", () => {
    expect(
      formatShiftDuration({
        check_in_at: "2026-05-14T22:30:00Z",
        check_out_at: "2026-05-15T01:45:00Z",
      })
    ).toBe("3h 15m")
  })

  it("throws when check_out_at is earlier than check_in_at", () => {
    expect(() =>
      formatShiftDuration({
        check_in_at: "2026-05-14T12:00:00Z",
        check_out_at: "2026-05-14T11:00:00Z",
      })
    ).toThrow(/check_out_at before check_in_at/)
  })

  it("throws on invalid ISO strings", () => {
    expect(() =>
      formatShiftDuration({
        check_in_at: "not-a-date",
        check_out_at: "2026-05-14T12:00:00Z",
      })
    ).toThrow(/invalid timestamp/)
  })

  it("renders Arabic-Indic digits when arabicNumerals is true", () => {
    const out = formatShiftDuration(
      {
        check_in_at: "2026-05-14T10:00:00Z",
        check_out_at: "2026-05-14T12:14:00Z",
      },
      { arabicNumerals: true }
    )
    expect(out).toBe("٢h ١٤m")
  })

  it("leaves letters untouched when switching digit shape", () => {
    const out = formatShiftDuration(
      {
        check_in_at: "2026-05-14T10:00:00Z",
        check_out_at: "2026-05-14T10:30:00Z",
      },
      { arabicNumerals: true }
    )
    expect(out).toContain("h ")
    expect(out).toContain("m")
  })
})
