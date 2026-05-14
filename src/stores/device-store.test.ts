import { beforeEach, describe, expect, it } from "vitest"

import { useDeviceStore } from "@/stores/device-store"

describe("useDeviceStore", () => {
  beforeEach(() => {
    useDeviceStore.setState({ device: null })
  })

  it("starts with a null device", () => {
    expect(useDeviceStore.getState().device).toBeNull()
  })

  it("setDevice persists the context", () => {
    useDeviceStore
      .getState()
      .setDevice({ deviceId: "device-abc", appVersion: "0.1.0" })
    expect(useDeviceStore.getState().device).toEqual({
      deviceId: "device-abc",
      appVersion: "0.1.0",
    })
  })

  it("setDevice replaces the previous device wholesale", () => {
    useDeviceStore.getState().setDevice({ deviceId: "a", appVersion: "1" })
    useDeviceStore.getState().setDevice({ deviceId: "b", appVersion: "2" })
    expect(useDeviceStore.getState().device).toEqual({
      deviceId: "b",
      appVersion: "2",
    })
  })
})
