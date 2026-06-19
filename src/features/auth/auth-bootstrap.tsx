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
      const role = (user!.role as UserRoleLiteral) ?? "receptionist"
      setAuthenticated({ user: user!, role, mode: "online" })
      // Restore the lock state on reload, otherwise reloading the webview
      // (or a crash-restart) silently bypasses the lock screen.
      try {
        const locked = await invoke("auth_is_locked")
        if (!cancelled && locked) setLocked(true)
      } catch {
        // a failed lock probe must not block auth restore
      }
    }

    // The Rust `bootstrap()` runs concurrently with the webview load and
    // restores a persisted session near its end. So a `null` from the very
    // first probe may just mean "restore hasn't run yet", not "no user". Poll a
    // few times over a short window before settling anonymous; the Rust
    // `auth:changed` emit (below) is the other half -- whichever fires first
    // wins, and `cancelled` guards against a late poll clobbering a real login.
    const PROBE_ATTEMPTS = 8
    const PROBE_INTERVAL_MS = 250
    const probeOnce = (attempt: number): void => {
      if (cancelled) return
      invoke("auth_current_user")
        .then((user) => {
          if (cancelled) return
          if (user) {
            void applyUser(user)
            return
          }
          // No user yet. Keep probing while bootstrap may still be restoring.
          if (attempt + 1 < PROBE_ATTEMPTS) {
            setTimeout(() => probeOnce(attempt + 1), PROBE_INTERVAL_MS)
          } else {
            setAnonymous()
          }
        })
        .catch((err) => {
          // An IPC rejection means the Rust state is not ready -- retry within
          // the same bounded window rather than signing the user out.
          if (cancelled) return
          if (attempt + 1 < PROBE_ATTEMPTS) {
            setTimeout(() => probeOnce(attempt + 1), PROBE_INTERVAL_MS)
          } else {
            console.error("auth_current_user failed after retries; treating as anonymous", err)
            setAnonymous()
          }
        })
    }
    probeOnce(0)

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
      // The refresh token is dead. Wipe the persisted session so the next
      // launch starts clean instead of re-restoring a doomed session and
      // flashing an authenticated state before the refresh fails again. This
      // is NOT a user-initiated logout, so it writes no logout audit row.
      void invoke("auth_clear_session").catch((err) => {
        console.warn("auth_clear_session after expiry failed", err)
      })
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
