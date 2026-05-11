import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Cloud, CloudOff, RotateCw, ArrowDownToLine, AlertCircle } from "lucide-react"

import { useOutboxCount, useSyncStatus } from "@/features/sync/queries"
import { useSyncStatusStore } from "@/stores/sync-status-store"
import { cn } from "@/lib/utils"

/**
 * Five-state sync pill (PRD §10.8 / phase-01 §7.4).
 *
 * The pill is a passive indicator in Phase-1; clicking it in `error` will
 * route to the conflict resolver once Phase-8 ships.
 */
export function SyncPill() {
  const { t } = useTranslation()
  const queryStatus = useSyncStatus().data?.status ?? "offline"
  const queuedFromStore = useSyncStatusStore((s) => s.status)
  const pendingFromQuery = useOutboxCount().data ?? 0
  const liveStatus = queuedFromStore !== "idle" ? queuedFromStore : queryStatus

  const config = useMemo(() => statusVariants[liveStatus] ?? statusVariants.idle, [liveStatus])

  const Icon = config.Icon
  return (
    <span
      role="status"
      aria-live="polite"
      data-state={liveStatus}
      className={cn(
        "inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-medium transition-colors",
        config.className
      )}
    >
      <Icon className={cn("h-3.5 w-3.5", config.spin && "animate-spin")} />
      <span>{t(`sync.status.${liveStatus}`, { defaultValue: liveStatus })}</span>
      {pendingFromQuery > 0 ? (
        <span
          className="ml-1 inline-flex h-5 min-w-[1.25rem] items-center justify-center rounded-full bg-foreground/10 px-1 text-[10px] font-semibold tabular-nums"
          aria-label={t("sync.pending_aria", {
            defaultValue: "Pending operations",
          })}
        >
          {pendingFromQuery}
        </span>
      ) : null}
    </span>
  )
}

const statusVariants: Record<
  string,
  { Icon: typeof Cloud; className: string; spin?: boolean }
> = {
  idle: {
    Icon: Cloud,
    className: "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300",
  },
  pushing: {
    Icon: RotateCw,
    className: "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-300",
    spin: true,
  },
  pulling: {
    Icon: ArrowDownToLine,
    className: "border-sky-500/30 bg-sky-500/10 text-sky-600 dark:text-sky-300",
    spin: true,
  },
  offline: {
    Icon: CloudOff,
    className: "border-muted-foreground/30 bg-muted text-muted-foreground",
  },
  error: {
    Icon: AlertCircle,
    className: "border-destructive/40 bg-destructive/10 text-destructive",
  },
}
