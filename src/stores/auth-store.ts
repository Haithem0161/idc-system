import { create } from "zustand"

import type { AuthUserContext, UserRoleLiteral } from "@/lib/ipc"

export type AuthState =
  | { kind: "loading" }
  | { kind: "anonymous" }
  | {
      kind: "authenticated"
      user: AuthUserContext
      role: UserRoleLiteral
      mode: "online" | "offline"
      locked: boolean
    }
  | { kind: "expired" }

interface AuthStore {
  state: AuthState
  setLoading: () => void
  setAnonymous: () => void
  setAuthenticated: (input: { user: AuthUserContext; role: UserRoleLiteral; mode: "online" | "offline" }) => void
  setLocked: (locked: boolean) => void
  setExpired: () => void
}

export const useAuthStore = create<AuthStore>((set) => ({
  state: { kind: "loading" },
  setLoading: () => set({ state: { kind: "loading" } }),
  setAnonymous: () => set({ state: { kind: "anonymous" } }),
  setAuthenticated: ({ user, role, mode }) =>
    set({ state: { kind: "authenticated", user, role, mode, locked: false } }),
  setLocked: (locked) =>
    set((s) =>
      s.state.kind === "authenticated"
        ? { state: { ...s.state, locked } }
        : s
    ),
  setExpired: () => set({ state: { kind: "expired" } }),
}))

export function selectCurrentUser (s: AuthStore): AuthUserContext | null {
  return s.state.kind === "authenticated" ? s.state.user : null
}

export function selectCurrentRole (s: AuthStore): UserRoleLiteral | null {
  return s.state.kind === "authenticated" ? s.state.role : null
}
