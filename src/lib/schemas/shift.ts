import { z } from "zod"

export const ShiftSchema = z.object({
  id: z.string().uuid(),
  operator_id: z.string().uuid(),
  check_in_at: z.string(),
  check_out_at: z.string().nullable(),
  check_in_by_user_id: z.string().uuid(),
  check_out_by_user_id: z.string().uuid().nullable(),
  note: z.string().nullable(),
  created_at: z.string(),
  updated_at: z.string(),
  deleted_at: z.string().nullable(),
  version: z.number().int(),
  entity_id: z.string(),
})
export type ShiftSchemaType = z.infer<typeof ShiftSchema>

export const ClockInInputSchema = z.object({
  operator_id: z.string().uuid(),
  note: z.string().max(1024).nullable().optional(),
})
export type ClockInInput = z.infer<typeof ClockInInputSchema>

export const ClockOutInputSchema = z.object({
  shift_id: z.string().uuid(),
})
export type ClockOutInput = z.infer<typeof ClockOutInputSchema>

export const ShiftEditSchema = z
  .object({
    shift_id: z.string().uuid(),
    check_in_at: z.string().datetime({ offset: true }),
    check_out_at: z.string().datetime({ offset: true }).nullable().optional(),
    note: z.object({ value: z.string().nullable() }).nullable().optional(),
  })
  .refine(
    (input) =>
      input.check_out_at == null
      || input.check_out_at >= input.check_in_at,
    {
      message: "check_out_at must be >= check_in_at",
      path: ["check_out_at"],
    }
  )
export type ShiftEditInput = z.infer<typeof ShiftEditSchema>

export const SoftDeleteShiftSchema = z.object({
  shift_id: z.string().uuid(),
  reason: z.string().min(1).max(512),
})
export type SoftDeleteShiftInput = z.infer<typeof SoftDeleteShiftSchema>
