import { useEffect } from "react"
import { useNavigate } from "react-router"

import { invoke, isTauri } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"
import { useIdleStore } from "@/stores/idle-store"
import { useSettings, settingValueAsNumber, getSettingByKey } from "@/features/settings/queries"

const ACTIVITY_EVENTS: Array<keyof DocumentEventMap> = [
  "mousemove",
  "keydown",
  "click",
  "touchstart",
]

const POLL_MS = 15_000

/**
 * Mounts inside `<AppShell>` once authenticated. Resets the last-activity
 * timer on user interaction; when the timer exceeds `settings.idle_lock_minutes`,
 * dispatches `auth::lock` and routes to `/lock`.
 */
export function IdleWatcher () {
  const state = useAuthStore((s) => s.state)
  const lastActivityAt = useIdleStore((s) => s.lastActivityAt)
  const bump = useIdleStore((s) => s.bump)
  const setIdleLockMinutes = useIdleStore((s) => s.setIdleLockMinutes)
  const navigate = useNavigate()

  const { data: settings } = useSettings()

  useEffect(() => {
    const setting = getSettingByKey(settings, "idle_lock_minutes")
    const minutes = settingValueAsNumber(setting, 10)
    setIdleLockMinutes(minutes)
  }, [settings, setIdleLockMinutes])

  useEffect(() => {
    if (state.kind !== "authenticated" || state.locked) return
    const onActivity = () => bump()
    for (const evt of ACTIVITY_EVENTS) {
      document.addEventListener(evt, onActivity, { passive: true })
    }
    return () => {
      for (const evt of ACTIVITY_EVENTS) {
        document.removeEventListener(evt, onActivity)
      }
    }
  }, [state, bump])

  useEffect(() => {
    if (state.kind !== "authenticated" || state.locked) return
    const idleLockMinutes = useIdleStore.getState().idleLockMinutes
    const threshold = Math.max(1, idleLockMinutes) * 60_000
    const interval = window.setInterval(() => {
      const idleFor = Date.now() - useIdleStore.getState().lastActivityAt
      if (idleFor >= threshold) {
        if (isTauri()) {
          void invoke("auth_lock").catch(() => undefined)
        }
        navigate("/lock", { replace: true })
      }
    }, POLL_MS)
    return () => window.clearInterval(interval)
  }, [state, lastActivityAt, navigate])

  return null
}
