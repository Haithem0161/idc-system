import { Navigate } from "react-router"

import { useAuthStore } from "@/stores/auth-store"

/**
 * Role-based root redirect. Until the per-role landing screens ship in later
 * phases, every authenticated role lands on the editorial home page.
 */
export default function RootRedirect () {
  const state = useAuthStore((s) => s.state)
  if (state.kind === "loading") return null
  if (state.kind !== "authenticated") return <Navigate to="/login" replace />
  return <Navigate to="/home" replace />
}
