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
  check_subtype_id: z.string().uuid().nullable().optional(),
  doctor_id: z.string().uuid().nullable().optional(),
  dye: z.boolean().optional(),
  report: z.boolean().optional(),
})
export type VisitUpdateDraftInput = z.infer<typeof VisitUpdateDraftSchema>

export const VisitLockSchema = z.object({
  visit_id: z.string().uuid(),
  operator_id: z.string().uuid(),
})
export type VisitLockInput = z.infer<typeof VisitLockSchema>

export const VisitVoidSchema = z.object({
  visit_id: z.string().uuid(),
  reason: z.string().trim().min(5, "void_reason_too_short"),
})
export type VisitVoidInput = z.infer<typeof VisitVoidSchema>
