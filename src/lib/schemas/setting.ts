import { z } from "zod"

export const SettingValueSchema = z.discriminatedUnion("valueType", [
  z.object({ valueType: z.literal("int"), value: z.number().int() }),
  z.object({ valueType: z.literal("decimal"), value: z.string() }),
  z.object({ valueType: z.literal("text"), value: z.string() }),
  z.object({ valueType: z.literal("bool"), value: z.boolean() }),
])
export type SettingValue = z.infer<typeof SettingValueSchema>

export const SettingSchema = z.object({
  id: z.string(),
  key: z.string(),
  value: SettingValueSchema,
  updated_at: z.string(),
  version: z.number().int(),
  entity_id: z.string(),
})
export type Setting = z.infer<typeof SettingSchema>
