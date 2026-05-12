import { z } from "zod"

/**
 * Reason for an inventory adjustment. `consume_visit` is excluded here
 * because it is only emitted by the visit-lock workflow (phase-05),
 * never by the operational form.
 */
export const adjustmentReasonSchema = z.enum([
  "receive",
  "writeoff",
  "count_correction",
])

export type AdjustmentReasonInput = z.infer<typeof adjustmentReasonSchema>

/**
 * Refined schema for the `<AdjustForm>`. The UI submits an `inputDelta` field
 * which is interpreted per-reason:
 *
 * - `receive`: positive integer, the quantity to add.
 * - `writeoff`: positive integer, the quantity to remove (service negates it).
 * - `count_correction`: signed integer, non-zero.
 *
 * Note may be empty; trimmed and length-capped at 500 chars to match the
 * server `validateAdjustment` (phase-06 §7.6).
 */
export const adjustmentInputSchema = z
  .object({
    item_id: z.string().uuid("item_required"),
    reason: adjustmentReasonSchema,
    input_delta: z
      .number({ message: "delta_required" })
      .int("delta_integer")
      .finite("delta_finite"),
    note: z
      .string()
      .trim()
      .max(500, "note_too_long")
      .optional()
      .or(z.literal("")),
  })
  .superRefine((data, ctx) => {
    if (data.reason === "receive" && data.input_delta <= 0) {
      ctx.addIssue({
        code: "custom",
        message: "delta_must_be_positive",
        path: ["input_delta"],
      })
    }
    if (data.reason === "writeoff" && data.input_delta <= 0) {
      ctx.addIssue({
        code: "custom",
        message: "delta_must_be_positive",
        path: ["input_delta"],
      })
    }
    if (data.reason === "count_correction" && data.input_delta === 0) {
      ctx.addIssue({
        code: "custom",
        message: "delta_must_be_nonzero",
        path: ["input_delta"],
      })
    }
  })

export type AdjustmentFormInput = z.infer<typeof adjustmentInputSchema>

/**
 * Convert form input to the IPC payload. `writeoff` flips sign so the stored
 * delta is negative (matches the SQLite CHECK + the Rust constructor).
 */
export function toIpcDelta (reason: AdjustmentReasonInput, inputDelta: number): number {
  switch (reason) {
    case "receive":
      return inputDelta
    case "writeoff":
      return -inputDelta
    case "count_correction":
      return inputDelta
  }
}
