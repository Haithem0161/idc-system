import { z } from "zod"

export const DeviceContextSchema = z.object({
  deviceId: z.string().min(1),
  appVersion: z.string().min(1),
})
export type DeviceContext = z.infer<typeof DeviceContextSchema>
