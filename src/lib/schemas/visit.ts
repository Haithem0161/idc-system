import { z } from "zod"

export const VisitStatusSchema = z.enum(["draft", "locked", "voided"])
export type VisitStatusLiteral = z.infer<typeof VisitStatusSchema>

export const VisitCreateDraftSchema = z.object({
  patient_id: z.string().uuid(),
  check_type_id: z.string().uuid(),
  check_subtype_id: z.string().uuid().nullable().optional(),
  doctor_id: z.string().uuid().nullable().optional(),
  dye: z.boolean().default(false),
  report: z.boolean().default(false),
})
export type VisitCreateDraftInput = z.infer<typeof VisitCreateDraftSchema>

export const VisitUpdateDraftSchema = z.object({
  visit_id: z.string().uuid(),
  // Reassign the draft to a corrected patient. Omitted = unchanged.
  patient_id: z.string().uuid().optional(),
  check_subtype_id: z.string().uuid().nullable().optional(),
  doctor_id: z.string().uuid().nullable().optional(),
  dye: z.boolean().optional(),
  report: z.boolean().optional(),
})
export type VisitUpdateDraftInput = z.infer<typeof VisitUpdateDraftSchema>

export const VisitLockSchema = z.object({
  visit_id: z.string().uuid(),
  operator_id: z.string().uuid(),
  // Cash actually collected when the receptionist overrides the billed total
  // (patient could not pay in full). Omitted/null = paid in full. Zero is a
  // valid collected amount (waived). Must be a non-negative integer.
  amount_paid_override_iqd: z
    .number()
    .int()
    .min(0, "amount_paid_override_negative")
    .nullable()
    .optional(),
})
export type VisitLockInput = z.infer<typeof VisitLockSchema>

export const VisitVoidSchema = z.object({
  visit_id: z.string().uuid(),
  reason: z.string().trim().min(5, "void_reason_too_short"),
})
export type VisitVoidInput = z.infer<typeof VisitVoidSchema>
