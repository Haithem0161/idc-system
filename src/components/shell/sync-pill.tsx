import { useEffect, useMemo } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"
import { Cloud, CloudOff, RotateCw, ArrowDownToLine, AlertCircle } from "lucide-react"
import type { LucideIcon } from "lucide-react"

import { useOutboxCount, useSyncStatus } from "@/features/sync/queries"
import { useSyncStatusStore } from "@/stores/sync-status-store"
import { cn } from "@/lib/utils"

interface PillVariant {
  Icon: LucideIcon
  tone: "success" | "warn" | "info" | "danger" | "muted"
  /** Live states blink the leading dot (design-system §4.3); no icon spin. */
  live?: boolean
}

const variants: Record<string, PillVariant> = {
  idle: { Icon: Cloud, tone: "success" },
  pushing: { Icon: RotateCw, tone: "warn", live: true },
  pulling: { Icon: ArrowDownToLine, tone: "info", live: true },
  offline: { Icon: CloudOff, tone: "muted" },
  error: { Icon: AlertCircle, tone: "danger" },
}

/**
 * Editorial sync pill -- 11px uppercase, dot blinks live, mono count badge
 * suffix when ops are pending. (.claude/rules/design-system.md §5.2)
 *
 * Always a button: clicking it (in any state, for any role) opens the /sync
 * dashboard. Activity is signalled by the blinking `is-live` dot, not a
 * spinning icon. Keyboard: Enter/Space activate the same nav.
 */
export function SyncPill() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryStatus = useSyncStatus().data?.status ?? "offline"
  const queuedFromStore = useSyncStatusStore((s) => s.status)
  const setStoreStatus = useSyncStatusStore((s) => s.setStatus)
  const pendingFromQuery = useOutboxCount().data ?? 0
  const liveStatus = queuedFromStore !== "idle" ? queuedFromStore : queryStatus

  // Seed the event-driven store from the polled snapshot. The engine emits its
  // first `sync:status` during Tauri setup, before any listener exists, so a
  // freshly mounted shell would otherwise show the store's default "idle"
  // until the next event. Whenever the store is at its default and the polled
  // status disagrees, adopt the canonical poll result.
  useEffect(() => {
    if (queuedFromStore === "idle" && queryStatus !== "idle") {
      setStoreStatus(queryStatus)
    }
  }, [queuedFromStore, queryStatus, setStoreStatus])

  const variant = useMemo(() => variants[liveStatus] ?? variants.idle, [liveStatus])

  const sharedClass = cn(
    "status-pill",
    variant.tone === "success" && "is-success",
    variant.tone === "warn" && "is-warn",
    variant.tone === "info" && "is-info",
    variant.tone === "danger" && "is-danger",
    variant.live && "is-live",
    "cursor-pointer hover:bg-paper-2 transition-colors"
  )

  return (
    <button
      type="button"
      onClick={() => navigate("/sync")}
      title={t("sync.pill.tooltip_open_dashboard", {
        defaultValue: "Open sync status",
      })}
      aria-label={t("sync.pill.aria_open_dashboard", {
        defaultValue: "Open sync status",
      })}
      data-state={liveStatus}
      className={sharedClass + " border-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"}
    >
      <variant.Icon className="h-3 w-3 -ms-1" strokeWidth={2} aria-hidden />
      <span>{t(`sync.status.${liveStatus}`, { defaultValue: liveStatus })}</span>
      {pendingFromQuery > 0 ? (
        <span
          className="count-badge ms-1"
          aria-label={t("sync.pending_aria", { defaultValue: "Pending operations" })}
        >
          {pendingFromQuery}
        </span>
      ) : null}
    </button>
  )
}
