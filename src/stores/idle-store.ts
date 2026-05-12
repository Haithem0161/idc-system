import { create } from "zustand"

interface IdleState {
  lastActivityAt: number
  idleLockMinutes: number
  bump: () => void
  setIdleLockMinutes: (minutes: number) => void
}

export const useIdleStore = create<IdleState>((set) => ({
  lastActivityAt: Date.now(),
  idleLockMinutes: 10,
  bump: () => set({ lastActivityAt: Date.now() }),
  setIdleLockMinutes: (minutes) => set({ idleLockMinutes: minutes }),
}))
