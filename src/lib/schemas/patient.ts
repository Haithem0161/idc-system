import { z } from "zod"

export const PatientCreateSchema = z.object({
  name: z.string().trim().min(1, "name_required"),
})

export type PatientCreateInput = z.infer<typeof PatientCreateSchema>

export const PatientUpdateSchema = PatientCreateSchema.extend({
  id: z.string().uuid(),
})

export type PatientUpdateInput = z.infer<typeof PatientUpdateSchema>
