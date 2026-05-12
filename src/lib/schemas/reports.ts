import { z } from "zod"

import type { VisitsReportGroupByLiteral } from "@/lib/ipc"

export const dateRangeSchema = z.object({
  from_utc: z.string(),
  to_utc: z.string(),
  include_voided: z.boolean().optional(),
})

export const visitsReportGroupByValues = [
  "none",
  "by_date",
  "by_doctor",
  "by_operator",
  "by_check_type",
  "by_subtype",
  "by_status",
] as const satisfies readonly VisitsReportGroupByLiteral[]

export const visitsReportFiltersSchema = z.object({
  from_utc: z.string(),
  to_utc: z.string(),
  include_voided: z.boolean().optional(),
  statuses: z.array(z.string()).optional(),
  check_type_ids: z.array(z.string().uuid()).optional(),
  subtype_ids: z.array(z.string().uuid()).optional(),
  doctor_ids: z.array(z.string().uuid()).optional(),
  operator_ids: z.array(z.string().uuid()).optional(),
  include_house: z.boolean().optional(),
  dye: z.boolean().nullable().optional(),
  report: z.boolean().nullable().optional(),
  group_by: z.enum(visitsReportGroupByValues).optional(),
  limit: z.number().int().positive().max(10_000).optional(),
})

export type VisitsReportFiltersInput = z.infer<typeof visitsReportFiltersSchema>

export const dailyCloseInputSchema = z.object({
  date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/, "YYYY-MM-DD"),
})
