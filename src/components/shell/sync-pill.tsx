import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"
import { Cloud, CloudOff, RotateCw, ArrowDownToLine, AlertCircle } from "lucide-react"
import type { LucideIcon } from "lucide-react"

import { useOutboxCount, useSyncStatus } from "@/features/sync/queries"
import { useSyncStatusStore } from "@/stores/sync-status-store"
import { useAuthStore } from "@/stores/auth-store"
import { cn } from "@/lib/utils"

interface PillVariant {
  Icon: LucideIcon
  tone: "success" | "warn" | "info" | "danger" | "muted"
  spin?: boolean
  live?: boolean
}

const variants: Record<string, PillVariant> = {
  idle: { Icon: Cloud, tone: "success" },
  pushing: { Icon: RotateCw, tone: "warn", spin: true, live: true },
  pulling: { Icon: ArrowDownToLine, tone: "info", spin: true, live: true },
  offline: { Icon: CloudOff, tone: "muted" },
  error: { Icon: AlertCircle, tone: "danger" },
}

/**
 * Editorial sync pill -- 11px uppercase, dot blinks live, mono count badge
 * suffix when ops are pending. (.claude/rules/design-system.md §5.2)
 *
 * Phase-08 §7.14: when status === error OR outboxCount > 0, the pill is
 * a button that navigates to /sync/conflicts (superadmin only). Keyboard:
 * Enter/Space activate the same nav.
 */
export function SyncPill() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryStatus = useSyncStatus().data?.status ?? "offline"
  const queuedFromStore = useSyncStatusStore((s) => s.status)
  const pendingFromQuery = useOutboxCount().data ?? 0
  const liveStatus = queuedFromStore !== "idle" ? queuedFromStore : queryStatus
  const role = useAuthStore((s) =>
    s.state.kind === "authenticated" ? s.state.role : null
  )

  const variant = useMemo(() => variants[liveStatus] ?? variants.idle, [liveStatus])
  const interactive =
    role === "superadmin" && (liveStatus === "error" || pendingFromQuery > 0)

  const sharedClass = cn(
    "status-pill",
    variant.tone === "success" && "is-success",
    variant.tone === "warn" && "is-warn",
    variant.tone === "info" && "is-info",
    variant.tone === "danger" && "is-danger",
    variant.live && "is-live",
    interactive && "cursor-pointer hover:bg-paper-2 transition-colors"
  )

  const inner = (
    <>
      <variant.Icon
        className={cn("h-3 w-3 -ms-1", variant.spin && "animate-spin")}
        strokeWidth={2}
        aria-hidden
      />
      <span>{t(`sync.status.${liveStatus}`, { defaultValue: liveStatus })}</span>
      {pendingFromQuery > 0 ? (
        <span
          className="count-badge ms-1"
          aria-label={t("sync.pending_aria", { defaultValue: "Pending operations" })}
        >
          {pendingFromQuery}
        </span>
      ) : null}
    </>
  )

  if (interactive) {
    return (
      <button
        type="button"
        onClick={() => navigate("/sync/conflicts")}
        title={t("sync.pill.tooltip_view_conflicts", {
          defaultValue: "View parked conflicts",
        })}
        aria-label={t("sync.pill.aria_view_conflicts", {
          defaultValue: "Open conflict resolver",
        })}
        data-state={liveStatus}
        className={sharedClass + " border-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"}
      >
        {inner}
      </button>
    )
  }

  return (
    <span
      role="status"
      aria-live="polite"
      data-state={liveStatus}
      className={sharedClass}
    >
      {inner}
    </span>
  )
}
