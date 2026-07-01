import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  RotateCw,
  RefreshCw,
  Server,
  MonitorSmartphone,
  AlertTriangle,
  ChevronRight,
} from "lucide-react"

import {
  useSyncStatus,
  useOutboxCount,
  useStuckOps,
  useLastSynced,
  useTriggerPush,
  useTriggerPull,
  useRequeueOp,
  useResyncLocal,
} from "@/features/sync/queries"
import { useAuthStore } from "@/stores/auth-store"
import { formatIpcError } from "@/lib/errors"
import { emitToast } from "@/lib/toast"
import { cn } from "@/lib/utils"

/**
 * /sync -- the sync status dashboard.
 *
 * A calm, at-a-glance answer to "is my data safe on the server?": the live
 * engine status, when this device last pushed and pulled, how many ops are
 * pending or stuck, and the device/server identity. Manual controls let the
 * user force a round-trip, requeue a stranded op, or (superadmin) run a full
 * re-push. Open to every role -- viewing sync health is not privileged; the
 * destructive resync stays superadmin-only.
 */
export default function SyncDashboardPage() {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()

  const status = useSyncStatus()
  const pending = useOutboxCount().data ?? 0
  const stuck = useStuckOps()
  const timing = useLastSynced()

  const triggerPush = useTriggerPush()
  const triggerPull = useTriggerPull()
  const requeue = useRequeueOp()
  const resync = useResyncLocal()

  const role = useAuthStore((s) =>
    s.state.kind === "authenticated" ? s.state.role : null
  )
  const isSuperadmin = role === "superadmin"

  const liveStatus = status.data?.status ?? "offline"
  const stuckOps = stuck.data ?? []
  const syncing = triggerPush.isPending || triggerPull.isPending

  const statusTone = useMemo(() => {
    switch (liveStatus) {
      case "idle":
        return "is-success"
      case "pushing":
        return "is-warn"
      case "pulling":
        return "is-info"
      case "error":
        return "is-danger"
      default:
        return ""
    }
  }, [liveStatus])
  const statusLive = liveStatus === "pushing" || liveStatus === "pulling"

  const rel = (iso: string | null): string => formatRelative(iso, i18n.language, t)

  const syncNow = async () => {
    try {
      await triggerPush.mutateAsync()
      await triggerPull.mutateAsync()
      void status.refetch()
      void stuck.refetch()
      void timing.refetch()
    } catch (e) {
      emitToast("error", formatIpcError(e, t))
    }
  }

  const handleRequeue = async (opId: string) => {
    try {
      await requeue.mutateAsync(opId)
      void stuck.refetch()
    } catch (e) {
      emitToast("error", formatIpcError(e, t))
    }
  }

  const handleResync = async () => {
    try {
      const r = await resync.mutateAsync()
      emitToast(
        "success",
        t("sync.dashboard.resync_done", {
          defaultValue: "Re-queued {{count}} rows for push.",
          count: r.total,
        })
      )
      void status.refetch()
      void stuck.refetch()
    } catch (e) {
      emitToast("error", formatIpcError(e, t))
    }
  }

  return (
    <div className="space-y-6">
      <header>
        <div className="eyebrow text-crimson">
          {t("sync.dashboard.eyebrow", { defaultValue: "SYNC" })}
        </div>
        <div className="flex items-center justify-between gap-3">
          <h1 className="text-[30px] font-bold tracking-[-0.026em] text-ink">
            {t("sync.dashboard.title", { defaultValue: "Sync status" })}
          </h1>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={() => void syncNow()}
              disabled={syncing}
              aria-label={t("sync.sync_now_aria", { defaultValue: "Sync now" })}
            >
              <RotateCw className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              <span>
                {syncing
                  ? t("sync.dashboard.syncing", { defaultValue: "Syncing" })
                  : t("sync.sync_now", { defaultValue: "Sync now" })}
              </span>
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => {
                void status.refetch()
                void stuck.refetch()
                void timing.refetch()
              }}
              aria-label={t("a11y.icons.refresh", { defaultValue: "Refresh" })}
            >
              <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              <span>{t("common.refresh", { defaultValue: "Refresh" })}</span>
            </button>
          </div>
        </div>
        <p className="mt-1 text-[12px] text-ink-3">
          {t("sync.dashboard.subtitle", {
            defaultValue:
              "How this device is syncing with the server -- what has shipped, what is waiting, and what needs attention.",
          })}
        </p>
      </header>

      {/* Status row: live pill + pending + stuck at a glance */}
      <div className="grid gap-4 sm:grid-cols-3">
        <div className="panel">
          <div className="panel-body space-y-2">
            <div className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-ink-3">
              {t("sync.dashboard.state", { defaultValue: "State" })}
            </div>
            <span className={cn("status-pill", statusTone, statusLive && "is-live")}>
              {t(`sync.status.${liveStatus}`, { defaultValue: liveStatus })}
            </span>
          </div>
        </div>
        <div className="panel">
          <div className="panel-body space-y-2">
            <div className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-ink-3">
              {t("sync.dashboard.pending", { defaultValue: "Pending" })}
            </div>
            <div className="font-mono text-[30px] font-bold tabular-nums text-ink">
              {pending}
            </div>
            <div className="text-[11px] text-ink-3">
              {t("sync.dashboard.pending_hint", {
                defaultValue: "Local changes waiting to reach the server.",
              })}
            </div>
          </div>
        </div>
        <div className="panel">
          <div className="panel-body space-y-2">
            <div className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-ink-3">
              {t("sync.dashboard.stuck", { defaultValue: "Stuck" })}
            </div>
            <div
              className={cn(
                "font-mono text-[30px] font-bold tabular-nums",
                stuckOps.length > 0 ? "text-crimson" : "text-ink"
              )}
            >
              {stuckOps.length}
            </div>
            <div className="text-[11px] text-ink-3">
              {t("sync.dashboard.stuck_hint", {
                defaultValue: "Ops that cannot progress without help.",
              })}
            </div>
          </div>
        </div>
      </div>

      {/* Last activity + device/server identity */}
      <div className="grid gap-4 lg:grid-cols-2">
        <div className="panel">
          <div className="panel-head">
            <div className="panel-title">
              {t("sync.dashboard.activity_title", { defaultValue: "Last activity" })}
            </div>
          </div>
          <div className="panel-body space-y-3">
            <ActivityRow
              icon={<ArrowUpFromLine className="h-4 w-4 text-ink-3" strokeWidth={1.8} />}
              label={t("sync.dashboard.last_pushed", { defaultValue: "Last pushed" })}
              value={rel(timing.data?.lastPushedAt ?? null)}
            />
            <ActivityRow
              icon={<ArrowDownToLine className="h-4 w-4 text-ink-3" strokeWidth={1.8} />}
              label={t("sync.dashboard.last_pulled", { defaultValue: "Last pulled" })}
              value={rel(timing.data?.lastPulledAt ?? null)}
            />
          </div>
        </div>

        <div className="panel">
          <div className="panel-head">
            <div className="panel-title">
              {t("sync.dashboard.identity_title", { defaultValue: "Device & server" })}
            </div>
          </div>
          <div className="panel-body space-y-3">
            <ActivityRow
              icon={<Server className="h-4 w-4 text-ink-3" strokeWidth={1.8} />}
              label={t("sync.dashboard.server", { defaultValue: "Server" })}
              value={timing.data?.serverUrl ?? "--"}
              mono
            />
            <ActivityRow
              icon={<MonitorSmartphone className="h-4 w-4 text-ink-3" strokeWidth={1.8} />}
              label={t("sync.dashboard.device", { defaultValue: "Device" })}
              value={timing.data?.deviceId ?? "--"}
              mono
            />
            <ActivityRow
              label={t("sync.dashboard.app_version", { defaultValue: "App version" })}
              value={timing.data?.appVersion ?? "--"}
              mono
            />
          </div>
        </div>
      </div>

      {/* Stuck operations */}
      <div className="panel">
        <div className="panel-head">
          <div className="panel-title flex items-center gap-2">
            {stuckOps.length > 0 ? (
              <AlertTriangle className="h-4 w-4 text-crimson" strokeWidth={1.8} />
            ) : null}
            {t("sync.dashboard.stuck_title", { defaultValue: "Stuck operations" })}
          </div>
          {isSuperadmin ? (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => void handleResync()}
              disabled={resync.isPending}
              title={t("sync.dashboard.resync_title", {
                defaultValue:
                  "Re-queue every local row for a full re-push (recovers a server that lost synced rows).",
              })}
            >
              <RotateCw className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              <span>
                {resync.isPending
                  ? t("sync.dashboard.resyncing", { defaultValue: "Re-pushing" })
                  : t("sync.dashboard.resync", { defaultValue: "Full re-push" })}
              </span>
            </button>
          ) : null}
        </div>
        <div className="panel-body">
          {stuckOps.length === 0 ? (
            <div className="py-6 text-center text-[13px] text-ink-3">
              {t("sync.dashboard.stuck_empty", {
                defaultValue: "Nothing stuck. Every change is flowing.",
              })}
            </div>
          ) : (
            <div className="space-y-2">
              {stuckOps.map((op) => (
                <div
                  key={op.opId}
                  className="flex items-start justify-between gap-3 rounded-md border border-line bg-paper-2 px-3 py-2"
                >
                  <div className="min-w-0 space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-[11px] font-semibold text-ink-2">
                        {op.entity}
                      </span>
                      <span className="font-mono text-[10px] text-ink-4">
                        {op.entityId.slice(0, 8)}
                      </span>
                      {op.parked ? (
                        <span className="count-badge is-alert">
                          {t("sync.dashboard.parked", { defaultValue: "parked" })}
                        </span>
                      ) : null}
                    </div>
                    {op.lastError ? (
                      <div className="truncate text-[11px] text-crimson" title={op.lastError}>
                        {op.lastError}
                      </div>
                    ) : null}
                  </div>
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm shrink-0"
                    onClick={() => void handleRequeue(op.opId)}
                    disabled={requeue.isPending}
                  >
                    {t("sync.dashboard.requeue", { defaultValue: "Retry" })}
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Link to conflict resolver (superadmin) */}
      {isSuperadmin ? (
        <button
          type="button"
          onClick={() => navigate("/sync/conflicts")}
          className="flex w-full items-center justify-between rounded-lg border border-line bg-surface px-5 py-4 text-start transition-colors duration-150 hover:bg-paper-2"
        >
          <div>
            <div className="text-[14px] font-semibold text-ink">
              {t("sync.dashboard.conflicts_link", { defaultValue: "Conflict resolver" })}
            </div>
            <div className="text-[12px] text-ink-3">
              {t("sync.dashboard.conflicts_hint", {
                defaultValue: "Review and resolve changes the server could not merge.",
              })}
            </div>
          </div>
          <ChevronRight className="h-4 w-4 text-ink-3 rtl:rotate-180" strokeWidth={1.8} aria-hidden />
        </button>
      ) : null}
    </div>
  )
}

function ActivityRow({
  icon,
  label,
  value,
  mono,
}: {
  icon?: React.ReactNode
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2 text-[12px] text-ink-3">
        {icon}
        <span>{label}</span>
      </div>
      <span
        className={cn(
          "truncate text-[13px] text-ink-2",
          mono && "font-mono text-[12px] tabular-nums"
        )}
        title={value}
      >
        {value}
      </span>
    </div>
  )
}

/**
 * Relative-time formatter using the platform Intl.RelativeTimeFormat -- no new
 * dependency. Falls back to "never" when the timestamp is null (this device
 * has not pushed/pulled yet). Uses whole-unit steps (seconds -> minutes ->
 * hours -> days) which is precise enough for a sync dashboard.
 */
function formatRelative(
  iso: string | null,
  lang: string,
  t: (k: string, o?: Record<string, unknown>) => string
): string {
  if (!iso) return t("sync.dashboard.never", { defaultValue: "Never" })
  const then = new Date(iso).getTime()
  if (Number.isNaN(then)) return "--"
  const diffMs = then - Date.now()
  const abs = Math.abs(diffMs)
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: "auto" })
  const sec = Math.round(diffMs / 1000)
  if (abs < 60_000) return rtf.format(sec, "second")
  const min = Math.round(diffMs / 60_000)
  if (abs < 3_600_000) return rtf.format(min, "minute")
  const hr = Math.round(diffMs / 3_600_000)
  if (abs < 86_400_000) return rtf.format(hr, "hour")
  const day = Math.round(diffMs / 86_400_000)
  return rtf.format(day, "day")
}
