import { describe, expect, it } from "vitest"

import { DeviceContextSchema } from "@/lib/schemas/device"

describe("DeviceContextSchema", () => {
  it("parses a minimal valid device context", () => {
    const parsed = DeviceContextSchema.parse({
      deviceId: "device-abc",
      appVersion: "0.1.0",
    })
    expect(parsed.deviceId).toBe("device-abc")
    expect(parsed.appVersion).toBe("0.1.0")
  })

  it("rejects empty deviceId", () => {
    expect(
      DeviceContextSchema.safeParse({ deviceId: "", appVersion: "0.1.0" })
        .success,
    ).toBe(false)
  })

  it("rejects empty appVersion", () => {
    expect(
      DeviceContextSchema.safeParse({ deviceId: "device-abc", appVersion: "" })
        .success,
    ).toBe(false)
  })

  it("rejects missing fields", () => {
    expect(DeviceContextSchema.safeParse({ deviceId: "x" }).success).toBe(false)
    expect(DeviceContextSchema.safeParse({ appVersion: "1" }).success).toBe(
      false,
    )
    expect(DeviceContextSchema.safeParse({}).success).toBe(false)
  })
})
