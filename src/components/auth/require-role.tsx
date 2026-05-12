import type { ReactNode } from "react"
import { Navigate } from "react-router"

import type { UserRoleLiteral } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"

/**
 * Route guard. Renders `children` when the current user's role is in
 * `roles`; otherwise navigates to `/no-access`. Anonymous users go to
 * `/login`.
 */
export function RequireRole ({ roles, children }: { roles: UserRoleLiteral[]; children: ReactNode }) {
  const state = useAuthStore((s) => s.state)
  if (state.kind === "loading") return null
  if (state.kind === "anonymous" || state.kind === "expired") {
    return <Navigate to="/login" replace />
  }
  if (!roles.includes(state.role)) {
    return <Navigate to="/no-access" replace />
  }
  return <>{children}</>
}

export function RequireAuth ({ children }: { children: ReactNode }) {
  const state = useAuthStore((s) => s.state)
  if (state.kind === "loading") return null
  if (state.kind !== "authenticated") {
    return <Navigate to="/login" replace />
  }
  return <>{children}</>
}
