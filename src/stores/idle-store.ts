import { create } from "zustand"

interface IdleState {
  lastActivityAt: number
  idleLockMinutes: number
  bump: () => void
  setIdleLockMinutes: (minutes: number) => void
}

/// Minimum gap between two `lastActivityAt` writes. Mouse-move fires dozens
/// of events per second; coalescing them to at most one store write per
/// second keeps the idle timer accurate without re-rendering subscribers on
/// every pixel of movement.
const BUMP_THROTTLE_MS = 1_000

export const useIdleStore = create<IdleState>((set, get) => ({
  lastActivityAt: Date.now(),
  idleLockMinutes: 10,
  bump: () => {
    const now = Date.now()
    // Throttle: only commit a new activity timestamp once per second. The
    // timer's resolution (15s poll, minutes-long threshold) is unaffected.
    if (now - get().lastActivityAt < BUMP_THROTTLE_MS) return
    set({ lastActivityAt: now })
  },
  setIdleLockMinutes: (minutes) => set({ idleLockMinutes: minutes }),
}))
