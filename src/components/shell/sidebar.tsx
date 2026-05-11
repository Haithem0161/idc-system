import { NavLink } from "react-router"
import { useTranslation } from "react-i18next"
import {
  Home,
  Users,
  Settings,
  ClipboardList,
  Stethoscope,
  Package,
  FileText,
  ShieldCheck,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { Logo } from "./logo"

/**
 * Phase-1 sidebar -- stubs the destination routes for Phases 2-8 so the
 * shell looks complete. Each item is a `NavLink`; routes that do not exist
 * yet render visually disabled and announce "coming soon" via `aria-disabled`.
 */
export function Sidebar() {
  const { t } = useTranslation()

  const items: SidebarItem[] = [
    { key: "home", to: "/", icon: Home, enabled: true },
    { key: "reception", to: "/reception", icon: ClipboardList, enabled: false },
    { key: "doctors", to: "/doctors", icon: Stethoscope, enabled: false },
    { key: "inventory", to: "/inventory", icon: Package, enabled: false },
    { key: "reports", to: "/reports", icon: FileText, enabled: false },
    { key: "users", to: "/users", icon: Users, enabled: false },
    { key: "audit", to: "/audit", icon: ShieldCheck, enabled: false },
    { key: "settings", to: "/settings", icon: Settings, enabled: false },
  ]

  return (
    <nav
      aria-label={t("nav.aria_label", { defaultValue: "Primary" })}
      className="hidden h-full w-56 shrink-0 border-r border-border bg-sidebar text-sidebar-foreground md:flex md:flex-col"
    >
      <div className="flex h-14 items-center gap-2 border-b border-border px-4 text-sm font-semibold tracking-tight">
        <Logo size={24} />
        <span>{t("app.title", { defaultValue: "IDC" })}</span>
      </div>
      <ul className="flex-1 space-y-1 overflow-y-auto p-2 text-sm">
        {items.map((item) => (
          <li key={item.key}>
            {item.enabled ? (
              <NavLink
                to={item.to}
                end={item.to === "/"}
                className={({ isActive }) =>
                  cn(
                    "flex items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                    isActive
                      ? "bg-sidebar-accent text-sidebar-accent-foreground"
                      : "text-sidebar-foreground/80 hover:bg-sidebar-accent/40 hover:text-sidebar-accent-foreground"
                  )
                }
              >
                <item.icon className="h-4 w-4" />
                <span>{t(`nav.${item.key}`, { defaultValue: item.key })}</span>
              </NavLink>
            ) : (
              <span
                aria-disabled="true"
                className="flex cursor-not-allowed items-center gap-2 rounded-md px-3 py-2 text-sm text-sidebar-foreground/40"
                title={t("nav.coming_soon", { defaultValue: "Coming soon" })}
              >
                <item.icon className="h-4 w-4" />
                <span>{t(`nav.${item.key}`, { defaultValue: item.key })}</span>
              </span>
            )}
          </li>
        ))}
      </ul>
    </nav>
  )
}

interface SidebarItem {
  key: string
  to: string
  icon: typeof Home
  enabled: boolean
}
