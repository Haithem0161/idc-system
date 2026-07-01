import { NavLink, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"
import {
  Users,
  Settings,
  ClipboardList,
  Stethoscope,
  Package,
  ShieldCheck,
  Lock,
  LogOut,
  Microscope,
  HardHat,
  UsersRound,
  Contact,
  LayoutDashboard,
  Activity,
  Boxes,
  CalendarCheck,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"

import { cn } from "@/lib/utils"
import { useAuthStore } from "@/stores/auth-store"
import { useLock, useLogout } from "@/features/auth/queries"
import { Logo } from "./logo"

interface SidebarItem {
  key: string
  to: string
  icon: LucideIcon
  enabled: boolean
  // Match this route EXACTLY (NavLink `end`). Needed for a parent path whose
  // children are separate nav items -- e.g. /reception must not light up when
  // the active route is its child /reception/shifts.
  end?: boolean
}

interface SidebarGroup {
  key: string
  items: SidebarItem[]
}

/**
 * Editorial sidebar (.claude/rules/design-system.md §10).
 * Width 256px, paper-on-paper with a single right border. Brand mark + role
 * pill at the top, grouped nav in the middle, user card pinned at the bottom.
 */
export function Sidebar() {
  const { t } = useTranslation()
  const state = useAuthStore((s) => s.state)
  const lock = useLock()
  const logout = useLogout()
  const navigate = useNavigate()

  const role = state.kind === "authenticated" ? state.role : null

  const groups: SidebarGroup[] = [
    {
      key: "operations",
      items: [
        {
          key: "reception",
          to: "/reception",
          icon: ClipboardList,
          enabled: role === "receptionist" || role === "superadmin",
          // Exact match so /reception/shifts doesn't also light up Reception.
          end: true,
        },
        {
          key: "shifts",
          to: "/reception/shifts",
          icon: HardHat,
          enabled: role === "receptionist" || role === "superadmin",
        },
        {
          key: "patients",
          to: "/reception/patients",
          icon: UsersRound,
          enabled: role === "receptionist" || role === "superadmin",
        },
        // Operations inventory removed from nav: inventory is managed from the
        // Admin > Inventory catalog. The ops /inventory routes still exist but
        // are no longer surfaced here.
      ],
    },
    {
      key: "accounting",
      items: [
        {
          key: "dashboard",
          to: "/accounting",
          icon: LayoutDashboard,
          enabled: role === "accountant" || role === "superadmin",
          // Exact match: the dashboard is the index of /accounting, so it must
          // not stay lit when an explorer child (/accounting/explore/...) is active.
          end: true,
        },
        {
          key: "acc_visits",
          to: "/accounting/explore/visits",
          icon: Activity,
          enabled: role === "accountant" || role === "superadmin",
        },
        {
          key: "acc_doctors",
          to: "/accounting/explore/doctors",
          icon: Stethoscope,
          enabled: role === "accountant" || role === "superadmin",
        },
        {
          key: "acc_operators",
          to: "/accounting/explore/operators",
          icon: Users,
          enabled: role === "accountant" || role === "superadmin",
        },
        {
          key: "acc_mandoubs",
          to: "/accounting/explore/mandoubs",
          icon: Contact,
          enabled: role === "accountant" || role === "superadmin",
        },
        {
          key: "acc_checks",
          to: "/accounting/explore/checks",
          icon: Boxes,
          enabled: role === "accountant" || role === "superadmin",
        },
        {
          key: "daily_close",
          to: "/accounting/daily-close",
          icon: CalendarCheck,
          enabled: role === "accountant" || role === "superadmin",
        },
      ],
    },
    {
      key: "admin",
      items: [
        { key: "users", to: "/admin/users", icon: Users, enabled: role === "superadmin" },
        { key: "check_types", to: "/admin/check-types", icon: Microscope, enabled: role === "superadmin" },
        { key: "doctors", to: "/admin/doctors", icon: Stethoscope, enabled: role === "superadmin" },
        { key: "operators", to: "/admin/operators", icon: HardHat, enabled: role === "superadmin" },
        { key: "mandoubs", to: "/admin/mandoubs", icon: Contact, enabled: role === "superadmin" },
        { key: "inventory", to: "/admin/inventory", icon: Package, enabled: role === "superadmin" },
        { key: "audit", to: "/audit", icon: ShieldCheck, enabled: role === "superadmin" },
        { key: "settings", to: "/admin/settings", icon: Settings, enabled: role === "superadmin" },
      ],
    },
  ]

  return (
    <nav
      aria-label={t("nav.aria_label", { defaultValue: "Primary" })}
      className="hidden h-full w-64 shrink-0 flex-col border-e border-line bg-paper text-ink-2 md:flex"
    >
      <div className="flex flex-col gap-3 px-5 pt-5 pb-3">
        <div className="flex items-center gap-2.5">
          <Logo size={26} />
          <span className="text-[15px] font-semibold tracking-tight text-ink">
            {t("app.title", { defaultValue: "IDC" })}
          </span>
        </div>
        {role ? (
          <span className={cn("role-pill self-start", `is-${role}`)}>
            {t(`auth.role_${role}`, { defaultValue: role })}
          </span>
        ) : null}
      </div>

      <div className="mt-3 flex-1 overflow-y-auto px-3 pb-4">
        {groups.map((group) => {
          const visible = group.items.filter((it) => it.enabled)
          if (visible.length === 0) return null
          return (
            <div key={group.key} className="mb-5 last:mb-0">
              <div className="px-3 pb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-ink-3">
                {t(`nav.group.${group.key}`, { defaultValue: group.key })}
              </div>
              <ul className="space-y-0.5">
                {visible.map((item) => (
                  <li key={item.key}>
                    <NavLink
                      to={item.to}
                      end={item.end}
                      className={({ isActive }) =>
                        cn("nav-item", isActive && "is-active")
                      }
                    >
                      <item.icon className="h-[15px] w-[15px]" strokeWidth={1.8} />
                      <span>{t(`nav.${item.key}`, { defaultValue: item.key })}</span>
                    </NavLink>
                  </li>
                ))}
              </ul>
            </div>
          )
        })}
      </div>

      {state.kind === "authenticated" ? (
        <UserCard
          name={state.user.name ?? state.user.email}
          email={state.user.email}
          offline={state.mode === "offline"}
          onLock={async () => {
            await lock.mutateAsync().catch(() => undefined)
            navigate("/lock")
          }}
          onLogout={async () => {
            await logout.mutateAsync().catch(() => undefined)
            navigate("/login")
          }}
        />
      ) : null}
    </nav>
  )
}

function UserCard({
  name,
  email,
  offline,
  onLock,
  onLogout,
}: {
  name: string
  email: string
  offline: boolean
  onLock: () => void
  onLogout: () => void
}) {
  const { t } = useTranslation()
  const initial = (name.trim()[0] ?? email[0] ?? "?").toUpperCase()
  return (
    <div className="border-t border-line px-4 py-3.5">
      <div className="flex items-center gap-2.5">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-ink text-paper text-[13px] font-semibold">
          {initial}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold text-ink" title={name}>{name}</div>
          <div className="truncate text-[11px] text-ink-3" title={email}>{email}</div>
        </div>
        <button
          type="button"
          onClick={onLock}
          aria-label={t("auth.lock", { defaultValue: "Lock session" })}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-ink-3 transition-colors hover:bg-paper-2 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
        >
          <Lock className="h-4 w-4" strokeWidth={1.8} />
        </button>
      </div>
      {offline ? (
        <div className="mt-2">
          <span className="status-pill is-warn">{t("auth.offline_session", { defaultValue: "Offline" })}</span>
        </div>
      ) : null}
      <button
        type="button"
        onClick={onLogout}
        className="mt-3 inline-flex items-center gap-1.5 text-[11px] font-medium text-ink-3 transition-colors hover:text-crimson focus-visible:outline-none focus-visible:underline"
      >
        <LogOut className="h-3 w-3" strokeWidth={1.8} />
        <span>{t("auth.sign_out", { defaultValue: "Sign out" })}</span>
      </button>
    </div>
  )
}
