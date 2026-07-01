import { useState } from "react"
import { useNavigate } from "react-router"
import { FlaskConical, Sprout, RefreshCw } from "lucide-react"
import { useQueryClient } from "@tanstack/react-query"

import { cn } from "@/lib/utils"
import type { UserRoleLiteral } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"
import {
  DEV_ACCOUNTS,
  ensureDevAccounts,
  switchToRole,
  homeForRole,
} from "@/features/dev/dev-accounts"
import { seedCatalog } from "@/features/dev/dev-seed"
import { useResyncLocal } from "@/features/sync/queries"

// Dev-only role-account switcher. Lets a developer flip between the three role
// surfaces by performing a REAL re-login as a per-role dev account (see
// features/dev/dev-accounts.ts). Rendered only in dev builds, for ANY signed-in
// role -- a developer logged in as reception or accounting can flip back just as
// easily as a superadmin. Account provisioning (ensureDevAccounts) is
// superadmin-only, so we only run it while we are superadmin and otherwise rely
// on the accounts already existing. It is intentionally NOT internationalized --
// it never ships to a clinic.
//
// Role dot colors mirror the design system's role coding
// (.claude/rules/design-system.md §1.5): receptionist=info, accountant=gold,
// superadmin=crimson.
const ROLE_DOT: Record<UserRoleLiteral, string> = {
  superadmin: "bg-crimson",
  receptionist: "bg-info",
  accountant: "bg-gold",
}

const ROLE_LABEL: Record<UserRoleLiteral, string> = {
  superadmin: "Admin",
  receptionist: "Reception",
  accountant: "Accounting",
}

export function DevViewSwitcher () {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const state = useAuthStore((s) => s.state)
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated)
  const [busy, setBusy] = useState<UserRoleLiteral | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [seeding, setSeeding] = useState(false)
  const [seedNote, setSeedNote] = useState<string | null>(null)
  const resync = useResyncLocal()
  const [resyncNote, setResyncNote] = useState<string | null>(null)

  // Only in dev, and only once a session exists. Visible to every role so a
  // developer can flip between surfaces from any account, not just superadmin.
  if (!import.meta.env.DEV) return null
  if (state.kind !== "authenticated") return null

  const currentRole = state.role

  const handleSwitch = async (role: UserRoleLiteral) => {
    if (busy) return
    setError(null)
    setBusy(role)
    try {
      // Provision any missing dev accounts WHILE we are still the superadmin
      // (users_create is superadmin-only). From a non-superadmin session we
      // can't provision, so we rely on the accounts already existing -- a clean
      // 401/credential error surfaces below if they don't.
      if (currentRole === "superadmin") {
        await ensureDevAccounts()
      }
      const result = await switchToRole(role)
      // Reflect the new session in the auth store (mirrors useLogin's onSuccess).
      setAuthenticated({
        user: {
          user_id: result.user.id,
          entity_id: result.user.entity_id,
          email: result.user.email,
          name: result.user.name,
          role: result.user.role,
        },
        role: result.user.role,
        mode: result.mode,
      })
      navigate(homeForRole(role), { replace: true })
    } catch (e) {
      setError((e as { message?: string }).message ?? "Switch failed")
    } finally {
      setBusy(null)
    }
  }

  const handleResync = async () => {
    if (resync.isPending) return
    setResyncNote(null)
    try {
      const r = await resync.mutateAsync()
      setResyncNote(`resynced ${r.total} rows`)
    } catch (e) {
      setResyncNote((e as { message?: string }).message ?? "Resync failed")
    }
  }

  const handleSeed = async () => {
    if (seeding) return
    setSeedNote(null)
    setSeeding(true)
    try {
      const r = await seedCatalog()
      // Refresh every catalog query so the seeded rows appear without a reload.
      await queryClient.invalidateQueries()
      setSeedNote(
        `+${r.checkTypes}ct ${r.doctors}dr ${r.operators}op ${r.inventory}inv`,
      )
    } catch (e) {
      setSeedNote((e as { message?: string }).message ?? "Seed failed")
    } finally {
      setSeeding(false)
    }
  }

  return (
    <div
      className="flex items-center gap-1 rounded-md border border-line-2 bg-paper-2 px-1 py-1"
      title="Dev only: switch role account (real re-login)"
    >
      <FlaskConical className="ms-1 h-3.5 w-3.5 text-ink-4" strokeWidth={1.8} aria-hidden />
      {DEV_ACCOUNTS.map((acct) => {
        const active = acct.role === currentRole
        const isBusy = busy === acct.role
        return (
          <button
            key={acct.role}
            type="button"
            onClick={() => void handleSwitch(acct.role)}
            disabled={!!busy}
            aria-pressed={active}
            className={cn(
              "inline-flex items-center gap-1.5 rounded px-2 py-1 text-[11px] font-semibold transition-colors duration-150 disabled:opacity-60",
              active
                ? "bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
                : "text-ink-3 hover:bg-surface/60 hover:text-ink",
            )}
          >
            <span className={cn("h-1.5 w-1.5 rounded-full", ROLE_DOT[acct.role], isBusy && "animate-pulse")} />
            {ROLE_LABEL[acct.role]}
          </button>
        )
      })}
      {/* Seeding drives superadmin-only create commands, so the control is only
          offered while signed in as superadmin. Other roles keep the switcher. */}
      {currentRole === "superadmin" ? (
        <>
          <span className="mx-0.5 h-4 w-px bg-line-2" aria-hidden />
          <button
            type="button"
            onClick={() => void handleSeed()}
            disabled={seeding}
            title="Dev only: seed catalog (check types, doctors, operators, inventory) via the real IPC"
            className="inline-flex items-center gap-1.5 rounded px-2 py-1 text-[11px] font-semibold text-ink-3 transition-colors duration-150 hover:bg-surface/60 hover:text-ink disabled:opacity-60"
          >
            <Sprout className={cn("h-3.5 w-3.5", seeding && "animate-pulse")} strokeWidth={1.8} />
            {seeding ? "Seeding" : "Seed"}
          </button>
        </>
      ) : null}
      <span className="mx-0.5 h-4 w-px bg-line-2" aria-hidden />
      <button
        type="button"
        onClick={() => void handleResync()}
        disabled={resync.isPending}
        title="Dev only: re-enqueue every local row for a full re-push (recovers a server that lost synced rows)"
        className="inline-flex items-center gap-1.5 rounded px-2 py-1 text-[11px] font-semibold text-ink-3 transition-colors duration-150 hover:bg-surface/60 hover:text-ink disabled:opacity-60"
      >
        <RefreshCw className={cn("h-3.5 w-3.5", resync.isPending && "animate-spin")} strokeWidth={1.8} />
        {resync.isPending ? "Resyncing" : "Resync"}
      </button>
      {resyncNote ? (
        <span className="ms-1 max-w-[160px] truncate text-[10px] text-ink-4" title={resyncNote}>
          {resyncNote}
        </span>
      ) : null}
      {seedNote ? (
        <span className="ms-1 max-w-[160px] truncate text-[10px] text-ink-4" title={seedNote}>
          {seedNote}
        </span>
      ) : null}
      {error ? (
        <span className="ms-1 max-w-[160px] truncate text-[10px] text-crimson" title={error}>
          {error}
        </span>
      ) : null}
    </div>
  )
}
