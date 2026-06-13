import { useState } from "react"
import { useTranslation } from "react-i18next"
import { RefreshCw, RotateCw } from "lucide-react"

import { ConflictList } from "@/components/sync/conflict-list"
import { ConflictResolverPanel } from "@/components/sync/conflict-resolver-panel"
import { useSyncConflicts, useTriggerPull, useTriggerPush } from "@/features/sync/queries"
import { formatIpcError } from "@/lib/errors"
import type { Conflict } from "@/lib/schemas/sync"

/**
 * /sync/conflicts -- the resolver UI (phase-08 §3 Frontend, §7.11).
 *
 * Lists parked conflicts (server-canonical) on the left; selecting one
 * opens the side-by-side resolver on the right. Mid-flight 409s are
 * surfaced via the panel's toast and trigger a refetch.
 */
export default function SyncConflictsPage() {
  const { t } = useTranslation()
  const list = useSyncConflicts()
  const triggerPush = useTriggerPush()
  const triggerPull = useTriggerPull()
  // Force a push then a pull so a superadmin can drain the outbox and pull the
  // server's resolution without waiting for the background loop, then refresh
  // the parked-conflict list.
  const syncNow = async () => {
    await triggerPush.mutateAsync()
    await triggerPull.mutateAsync()
    void list.refetch()
  }
  const syncing = triggerPush.isPending || triggerPull.isPending
  // User selection sticks until they pick another row OR the row vanishes
  // from the queue (resolved on this device or another). Default to the
  // first row when nothing is selected -- derived, not effect-driven.
  const [userSelectedOpId, setUserSelectedOpId] = useState<string | null>(null)
  const conflicts = list.data ?? []
  const userPick =
    userSelectedOpId && conflicts.some((c) => c.opId === userSelectedOpId)
      ? userSelectedOpId
      : null
  const selectedOpId: string | null =
    userPick ?? (conflicts.length > 0 ? conflicts[0].opId : null)
  const selected: Conflict | null =
    conflicts.find((c) => c.opId === selectedOpId) ?? null

  return (
    <div className="space-y-6">
      <header>
        <div className="eyebrow text-crimson">
          {t("sync_conflicts.eyebrow", { defaultValue: "SYNC CONFLICTS" })}
        </div>
        <div className="flex items-center justify-between gap-3">
          <h1 className="text-[30px] font-bold tracking-[-0.026em] text-ink">
            {t("sync_conflicts.title", { defaultValue: "Conflict resolver" })}
          </h1>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => void syncNow()}
              disabled={syncing}
              aria-label={t("sync.sync_now_aria", { defaultValue: "Sync now" })}
            >
              <RotateCw
                className={"h-3.5 w-3.5" + (syncing ? " animate-spin" : "")}
                strokeWidth={1.8}
                aria-hidden
              />
              <span>{t("sync.sync_now", { defaultValue: "Sync now" })}</span>
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => list.refetch()}
              disabled={list.isFetching}
              aria-label={t("a11y.icons.refresh", { defaultValue: "Refresh" })}
            >
              <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              <span>{t("common.refresh", { defaultValue: "Refresh" })}</span>
            </button>
          </div>
        </div>
        <p className="mt-1 text-[12px] text-ink-3">
          {t("sync_conflicts.subtitle", {
            defaultValue:
              "Server-parked conflicts are listed here. Pick a row to inspect and resolve.",
          })}
        </p>
      </header>

      {list.isError ? (
        <div className="rounded-md border border-crimson/30 bg-crimson-soft px-4 py-3 text-[13px] text-crimson">
          {t("sync_conflicts.load_error", {
            defaultValue: "Failed to load conflicts: {{msg}}",
            msg: formatIpcError(list.error, t),
          })}
        </div>
      ) : null}

      <div className="grid gap-6 lg:grid-cols-[320px_1fr]">
        <div>
          <ConflictList
            conflicts={conflicts}
            selectedOpId={selectedOpId}
            onSelect={(c) => setUserSelectedOpId(c.opId)}
          />
        </div>
        <div>
          {selected ? (
            <ConflictResolverPanel
              conflict={selected}
              onResolved={() => {
                void list.refetch()
              }}
            />
          ) : (
            <div className="rounded-md border border-line bg-surface px-6 py-12 text-center text-[13px] text-ink-3">
              {t("sync_conflicts.select_prompt", {
                defaultValue:
                  "Select a conflict from the list to inspect and resolve.",
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
