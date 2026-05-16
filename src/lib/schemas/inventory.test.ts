/**
 * Phase-06 §1.2 unit tests for the inventory operations Zod schemas.
 *
 * Locks the user-input branch of `<AdjustForm>` against PRD §6.1.14 invariants
 * + phase-06 §4: receive/writeoff require positive qty, count_correction
 * requires non-zero signed delta, note is optional but capped at 500 chars,
 * `consume_visit` is NOT a user-selectable reason.
 */

import { describe, expect, it } from "vitest"

import {
  adjustmentInputSchema,
  adjustmentReasonSchema,
  toIpcDelta,
} from "./inventory"

const ITEM = "0190f3a0-f1c0-7000-8000-0000000a0001"

describe("adjustmentReasonSchema", () => {
  it.each(["receive", "writeoff", "count_correction"])(
    "accepts the user-selectable reason %s",
    (reason) => {
      expect(adjustmentReasonSchema.parse(reason)).toBe(reason)
    }
  )

  it("rejects consume_visit (lock-workflow reserved)", () => {
    expect(() => adjustmentReasonSchema.parse("consume_visit")).toThrow()
  })

  it("rejects unknown literals", () => {
    expect(() => adjustmentReasonSchema.parse("destroy")).toThrow()
    expect(() => adjustmentReasonSchema.parse("")).toThrow()
    expect(() => adjustmentReasonSchema.parse(null)).toThrow()
  })
})

describe("adjustmentInputSchema -- receive branch", () => {
  it("parses positive qty + reason=receive", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 5,
    })
    expect(parsed.reason).toBe("receive")
    expect(parsed.input_delta).toBe(5)
  })

  it("rejects qty=0 on the input_delta path", () => {
    const result = adjustmentInputSchema.safeParse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 0,
    })
    expect(result.success).toBe(false)
    if (!result.success) {
      const issue = result.error.issues.find(
        (i) => i.path[0] === "input_delta"
      )
      expect(issue?.message).toBe("delta_must_be_positive")
    }
  })

  it("rejects negative qty on the input_delta path", () => {
    const result = adjustmentInputSchema.safeParse({
      item_id: ITEM,
      reason: "receive",
      input_delta: -3,
    })
    expect(result.success).toBe(false)
  })
})

describe("adjustmentInputSchema -- writeoff branch", () => {
  it("accepts positive qty (UI submits positive; Rust negates)", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "writeoff",
      input_delta: 4,
    })
    expect(parsed.input_delta).toBe(4)
  })

  it("rejects qty=0", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: ITEM,
        reason: "writeoff",
        input_delta: 0,
      }).success
    ).toBe(false)
  })

  it("rejects negative qty (the form clamps to positive integers)", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: ITEM,
        reason: "writeoff",
        input_delta: -1,
      }).success
    ).toBe(false)
  })
})

describe("adjustmentInputSchema -- count_correction branch", () => {
  it("accepts positive signed delta", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "count_correction",
      input_delta: 7,
    })
    expect(parsed.input_delta).toBe(7)
  })

  it("accepts negative signed delta", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "count_correction",
      input_delta: -7,
    })
    expect(parsed.input_delta).toBe(-7)
  })

  it("rejects delta=0", () => {
    const result = adjustmentInputSchema.safeParse({
      item_id: ITEM,
      reason: "count_correction",
      input_delta: 0,
    })
    expect(result.success).toBe(false)
    if (!result.success) {
      const issue = result.error.issues.find(
        (i) => i.path[0] === "input_delta"
      )
      expect(issue?.message).toBe("delta_must_be_nonzero")
    }
  })
})

describe("adjustmentInputSchema -- item_id", () => {
  it("rejects non-UUID item_id", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: "not-a-uuid",
        reason: "receive",
        input_delta: 1,
      }).success
    ).toBe(false)
  })

  it("rejects missing item_id", () => {
    expect(
      adjustmentInputSchema.safeParse({
        reason: "receive",
        input_delta: 1,
      }).success
    ).toBe(false)
  })
})

describe("adjustmentInputSchema -- delta primitive guards", () => {
  it("rejects non-integer delta", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: ITEM,
        reason: "receive",
        input_delta: 1.5,
      }).success
    ).toBe(false)
  })

  it("rejects non-finite delta", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: ITEM,
        reason: "receive",
        input_delta: Number.POSITIVE_INFINITY,
      }).success
    ).toBe(false)
  })

  it("rejects NaN delta", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: ITEM,
        reason: "receive",
        input_delta: Number.NaN,
      }).success
    ).toBe(false)
  })

  it("rejects missing delta", () => {
    expect(
      adjustmentInputSchema.safeParse({
        item_id: ITEM,
        reason: "receive",
      }).success
    ).toBe(false)
  })
})

describe("adjustmentInputSchema -- note", () => {
  it("accepts an absent note", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 1,
    })
    expect(parsed.note).toBeUndefined()
  })

  it("accepts an empty-string note (z.literal('') escape)", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 1,
      note: "",
    })
    expect(parsed.note).toBe("")
  })

  it("trims whitespace from the note", () => {
    const parsed = adjustmentInputSchema.parse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 1,
      note: "   from supplier   ",
    })
    expect(parsed.note).toBe("from supplier")
  })

  it("rejects a 501-char note", () => {
    const result = adjustmentInputSchema.safeParse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 1,
      note: "x".repeat(501),
    })
    expect(result.success).toBe(false)
  })

  it("accepts a 500-char note (boundary inclusive)", () => {
    const result = adjustmentInputSchema.safeParse({
      item_id: ITEM,
      reason: "receive",
      input_delta: 1,
      note: "x".repeat(500),
    })
    expect(result.success).toBe(true)
  })
})

describe("toIpcDelta", () => {
  it("passes receive qty unchanged (positive add)", () => {
    expect(toIpcDelta("receive", 5)).toBe(5)
  })

  it("flips writeoff sign (UI positive -> stored negative)", () => {
    expect(toIpcDelta("writeoff", 5)).toBe(-5)
  })

  it("passes count_correction signed delta unchanged", () => {
    expect(toIpcDelta("count_correction", 7)).toBe(7)
    expect(toIpcDelta("count_correction", -7)).toBe(-7)
  })
})
