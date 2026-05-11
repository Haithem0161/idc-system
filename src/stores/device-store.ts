import { create } from "zustand"

import type { DeviceContext } from "@/lib/schemas/device"

interface DeviceState {
  device: DeviceContext | null
  setDevice: (device: DeviceContext) => void
}

export const useDeviceStore = create<DeviceState>((set) => ({
  device: null,
  setDevice: (device) => set({ device }),
}))
