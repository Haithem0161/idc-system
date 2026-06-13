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

    const applyUser = async (user: Awaited<ReturnType<typeof invoke<"auth_current_user">>>) => {
      if (cancelled) return
      if (!user) {
        // A genuine null is the only signal for "no signed-in user" -> sign
        // out. An IPC rejection is handled separately below.
        setAnonymous()
        return
      }
      const role = (user.role as UserRoleLiteral) ?? "receptionist"
      setAuthenticated({ user, role, mode: "online" })
      // Restore the lock state on reload, otherwise reloading the webview
      // (or a crash-restart) silently bypasses the lock screen.
      try {
        const locked = await invoke("auth_is_locked")
        if (!cancelled && locked) setLocked(true)
      } catch {
        // a failed lock probe must not block auth restore
      }
    }

    invoke("auth_current_user")
      .then(applyUser)
      .catch((err) => {
        // A failed `auth_current_user` is NOT the same as "no user": the Rust
        // state may not be ready yet during bootstrap. Retry once before
        // falling back to anonymous so a transient race doesn't silently sign
        // the user out into the login screen.
        console.warn("auth_current_user failed; retrying once", err)
        if (cancelled) return
        invoke("auth_current_user")
          .then(applyUser)
          .catch((err2) => {
            console.error("auth_current_user failed after retry; treating as anonymous", err2)
            if (!cancelled) setAnonymous()
          })
      })

    listenEvent<string>("auth:changed", (payload) => {
      // Rust emits the real LoginMode ("online" | "offline"); honour it
      // instead of hardcoding "online", which clobbered offline-login state.
      const mode: "online" | "offline" = payload === "offline" ? "offline" : "online"
      void invoke("auth_current_user")
        .then((user) => {
          if (!user) {
            setAnonymous()
            return
          }
          const role = (user.role as UserRoleLiteral) ?? "receptionist"
          setAuthenticated({ user, role, mode })
        })
        .catch((err) => {
          // Don't leave an unhandled rejection or a stale auth state when the
          // post-`auth:changed` refetch fails; log and keep the prior state.
          console.error("auth_current_user (auth:changed) failed", err)
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
