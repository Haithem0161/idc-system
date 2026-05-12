import { useEffect, useRef, useState } from "react"
import { useNavigate } from "react-router"
import { useTranslation } from "react-i18next"
import { LogOut, Lock, ChevronDown } from "lucide-react"

import { useAuthStore } from "@/stores/auth-store"
import { useLock, useLogout } from "@/features/auth/queries"
import { cn } from "@/lib/utils"

const roleToneClass: Record<string, string> = {
  superadmin: "bg-crimson text-white",
  accountant: "bg-gold text-white",
  receptionist: "bg-info text-white",
}

/**
 * Compact avatar + dropdown for the 64px header. The richer user card lives
 * in the sidebar; this is the always-visible identity hook.
 */
export function UserMenu() {
  const { t } = useTranslation()
  const state = useAuthStore((s) => s.state)
  const lock = useLock()
  const logout = useLogout()
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!open) return
    const onDocClick = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false)
    }
    document.addEventListener("mousedown", onDocClick)
    document.addEventListener("keydown", onKey)
    return () => {
      document.removeEventListener("mousedown", onDocClick)
      document.removeEventListener("keydown", onKey)
    }
  }, [open])

  if (state.kind !== "authenticated") return null
  const display = state.user.name ?? state.user.email
  const initial = (display.trim()[0] ?? "?").toUpperCase()
  const roleTone = roleToneClass[state.role] ?? "bg-ink text-paper"

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        className="inline-flex h-8 items-center gap-1.5 rounded-full border border-line-2 bg-paper pe-2 ps-0.5 text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
      >
        <span className={cn("flex h-7 w-7 items-center justify-center rounded-full text-[12px] font-semibold", roleTone)}>
          {initial}
        </span>
        <ChevronDown className="h-3 w-3 text-ink-3" strokeWidth={1.8} />
      </button>
      {open ? (
        <div
          role="menu"
          className="absolute end-0 z-30 mt-2 w-60 overflow-hidden rounded-xl border border-line bg-surface text-ink shadow-[0_10px_30px_rgba(10,18,48,0.08)]"
        >
          <div className="border-b border-line px-4 py-3">
            <div className="truncate text-[13px] font-semibold text-ink" title={display}>{display}</div>
            <div className="truncate text-[11px] text-ink-3" title={state.user.email}>{state.user.email}</div>
            <div className="mt-2 flex items-center gap-2">
              <span className={cn("role-pill", `is-${state.role}`)}>
                {t(`auth.role_${state.role}`, { defaultValue: state.role })}
              </span>
              {state.mode === "offline" ? (
                <span className="status-pill is-warn">{t("auth.offline_session", { defaultValue: "Offline" })}</span>
              ) : null}
            </div>
          </div>
          <button
            type="button"
            onClick={() => {
              setOpen(false)
              void lock.mutateAsync().then(() => navigate("/lock"))
            }}
            className="flex w-full items-center gap-2.5 px-4 py-2.5 text-start text-[12.5px] text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink"
          >
            <Lock className="h-3.5 w-3.5" strokeWidth={1.8} />
            <span>{t("auth.lock", { defaultValue: "Lock session" })}</span>
          </button>
          <button
            type="button"
            onClick={() => {
              setOpen(false)
              void logout.mutateAsync().then(() => navigate("/login"))
            }}
            className="flex w-full items-center gap-2.5 border-t border-line px-4 py-2.5 text-start text-[12.5px] text-ink-2 transition-colors hover:bg-crimson-soft hover:text-crimson"
          >
            <LogOut className="h-3.5 w-3.5" strokeWidth={1.8} />
            <span>{t("auth.sign_out", { defaultValue: "Sign out" })}</span>
          </button>
        </div>
      ) : null}
    </div>
  )
}
