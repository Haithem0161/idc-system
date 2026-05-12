import { useEffect } from "react"

import { invoke, isTauri, listenEvent } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"
import type { UserRoleLiteral } from "@/lib/ipc"

/**
 * One-shot bootstrap of the Tauri-side auth state into the Zustand store.
 * Mounted once near the top of the tree (inside `<App>`); rest of the app
 * reads the store, never the IPC directly.
 */
export function AuthBootstrap() {
  const setLoading = useAuthStore((s) => s.setLoading)
  const setAnonymous = useAuthStore((s) => s.setAnonymous)
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated)
  const setLocked = useAuthStore((s) => s.setLocked)
  const setExpired = useAuthStore((s) => s.setExpired)

  useEffect(() => {
    if (!isTauri()) {
      setAnonymous()
      return
    }
    setLoading()

    const unsubs: Array<() => void> = []
    let cancelled = false

    invoke("auth_current_user")
      .then((user) => {
        if (cancelled) return
        if (!user) {
          setAnonymous()
          return
        }
        const role = (user.role as UserRoleLiteral) ?? "receptionist"
        setAuthenticated({
          user,
          role,
          mode: "online",
        })
      })
      .catch(() => {
        if (!cancelled) setAnonymous()
      })

    listenEvent<string>("auth:changed", () => {
      void invoke("auth_current_user").then((user) => {
        if (!user) {
          setAnonymous()
          return
        }
        const role = (user.role as UserRoleLiteral) ?? "receptionist"
        setAuthenticated({ user, role, mode: "online" })
      })
    }).then((u) => unsubs.push(u))

    listenEvent<unknown>("auth:lock", () => {
      setLocked(true)
    }).then((u) => unsubs.push(u))

    listenEvent<unknown>("auth:unlock", () => {
      setLocked(false)
    }).then((u) => unsubs.push(u))

    listenEvent<unknown>("auth:session_expired", () => {
      setExpired()
    }).then((u) => unsubs.push(u))

    return () => {
      cancelled = true
      for (const u of unsubs) {
        try {
          u()
        } catch {
          // ignore
        }
      }
    }
  }, [setLoading, setAnonymous, setAuthenticated, setLocked, setExpired])

  return null
}
