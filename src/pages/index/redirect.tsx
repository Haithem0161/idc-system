import { Navigate } from "react-router"

import { useAuthStore } from "@/stores/auth-store"

/**
 * Role-based root redirect. Each role lands directly on its primary surface;
 * the shell has no neutral "home" screen.
 */
export default function RootRedirect () {
  const state = useAuthStore((s) => s.state)
  if (state.kind === "loading") return null
  if (state.kind !== "authenticated") return <Navigate to="/login" replace />
  switch (state.role) {
    case "accountant":
      return <Navigate to="/accounting" replace />
    case "receptionist":
    case "superadmin":
    default:
      return <Navigate to="/reception" replace />
  }
}
