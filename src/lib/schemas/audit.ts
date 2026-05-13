import { z } from "zod"

export const AUDIT_ACTIONS = [
  "create",
  "update",
  "soft_delete",
  "lock",
  "void",
  "discard",
  "clock_in",
  "clock_out",
  "password_change",
  "login",
  "logout",
  "conflict_resolve",
  "vacuum",
  "daily_close_run",
] as const

export const AUDIT_ENTITIES = [
  "users",
  "settings",
  "check_types",
  "check_subtypes",
  "doctors",
  "doctor_check_pricing",
  "operators",
  "operator_specialties",
  "operator_shifts",
  "patients",
  "visits",
  "inventory_items",
  "inventory_consumption_map",
  "inventory_adjustments",
  "audit_log",
] as const

export const AuditFilterSchema = z.object({
  actor_user_id: z.string().uuid().optional(),
  action: z.enum(AUDIT_ACTIONS).optional(),
  entity: z.enum(AUDIT_ENTITIES).optional(),
  entity_id_prefix: z.string().min(4).max(36).optional(),
  from_utc: z.string().datetime({ offset: true }).optional(),
  to_utc: z.string().datetime({ offset: true }).optional(),
  text: z.string().min(2).max(100).optional(),
  limit: z.number().int().min(1).max(100).optional(),
  offset: z.number().int().min(0).optional(),
})
export type AuditFilter = z.infer<typeof AuditFilterSchema>

export const AuditSourceSchema = z.enum(["local", "server"])
export type AuditSource = z.infer<typeof AuditSourceSchema>

export const AuditRowSchema = z.object({
  id: z.string(),
  at: z.string(),
  actor_user_id: z.string(),
  action: z.string(),
  entity: z.string(),
  entity_id: z.string(),
  delta: z.unknown(),
  device_id: z.string(),
  version: z.number().int(),
  dirty: z.boolean(),
  source: AuditSourceSchema,
})
export type AuditRow = z.infer<typeof AuditRowSchema>

export const AuditQueryModeSchema = z.enum(["local", "server", "merged"])
export type AuditQueryMode = z.infer<typeof AuditQueryModeSchema>

export const AuditPageSchema = z.object({
  rows: z.array(AuditRowSchema),
  mode: AuditQueryModeSchema,
  next_offset: z.number().int().nullable(),
})
export type AuditPage = z.infer<typeof AuditPageSchema>

export const DiagnosticsSummarySchema = z.object({
  lock_latency_p95_ms: z.number().int().nullable(),
  outbox_depth: z.number().int().nonnegative(),
  last_sync_at: z.string().nullable(),
  conflict_count_7d: z.number().int().nonnegative(),
  receipt_print_success_rate_30d: z.number().nullable(),
})
export type DiagnosticsSummary = z.infer<typeof DiagnosticsSummarySchema>
