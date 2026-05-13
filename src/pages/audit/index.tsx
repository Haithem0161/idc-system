import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { RefreshCw, Eraser } from "lucide-react"

import { AuditFilters } from "@/components/audit/audit-filters"
import { AuditTable } from "@/components/audit/audit-table"
import { useAuditQuery, useAuditVacuum } from "@/features/audit/queries"
import type { AuditFilter } from "@/lib/schemas/audit"
import { emitToast } from "@/lib/toast"

/**
 * Audit log page (phase-08 §3 Frontend).
 *
 * Superadmin-only (wrapped in `<RequireRole>` at the route level, §7.23).
 * Filter chips + result table with expandable JSON delta. Shows the
 * server-backed pill when the query crosses the 90-day cliff.
 *
 * Manual "Vacuum now" button: runs `audit::vacuum_now` immediately. The
 * underlying scheduler also fires daily at 03:00 UTC (lib.rs::bootstrap).
 */
export default function AuditPage() {
  const { t } = useTranslation()
  const [filter, setFilter] = useState<AuditFilter>({
    limit: 50,
    offset: 0,
  })
  const query = useAuditQuery(filter)
  const vacuum = useAuditVacuum()

  const summary = useMemo(() => {
    if (!query.data) return null
    return t("audit.summary", {
      defaultValue: "{{count}} results · mode: {{mode}}",
      count: query.data.rows.length,
      mode: query.data.mode,
    })
  }, [query.data, t])

  return (
    <div className="space-y-6">
      <header>
        <div className="eyebrow text-crimson">
          {t("audit.eyebrow", { defaultValue: "AUDIT LOG" })}
        </div>
        <div className="flex items-center justify-between gap-3">
          <h1 className="text-[30px] font-bold tracking-[-0.026em] text-ink">
            {t("audit.title", { defaultValue: "Audit log" })}
          </h1>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => query.refetch()}
              disabled={query.isFetching}
              aria-label={t("a11y.icons.refresh", { defaultValue: "Refresh" })}
            >
              <RefreshCw
                className="h-3.5 w-3.5"
                strokeWidth={1.8}
                aria-hidden
              />
              <span>{t("common.refresh", { defaultValue: "Refresh" })}</span>
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => {
                vacuum.mutate(undefined, {
                  onSuccess: (data) =>
                    emitToast(
                      "success",
                      t("audit.vacuum.success", {
                        defaultValue:
                          "Vacuum done: {{audit}} audit, {{metrics}} metrics rows purged",
                        audit: data.audit_purged,
                        metrics: data.metrics_purged,
                      })
                    ),
                  onError: (e) =>
                    emitToast(
                      "error",
                      t("audit.vacuum.failure", {
                        defaultValue: "Vacuum failed: {{msg}}",
                        msg: String(e),
                      })
                    ),
                })
              }}
              disabled={vacuum.isPending}
              aria-label={t("audit.vacuum.aria", {
                defaultValue: "Run audit vacuum now",
              })}
            >
              <Eraser className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              <span>
                {t("audit.vacuum.button", { defaultValue: "Vacuum now" })}
              </span>
            </button>
          </div>
        </div>
        {summary ? (
          <p className="mt-1 text-[12px] text-ink-3">{summary}</p>
        ) : null}
      </header>

      <AuditFilters value={filter} onChange={setFilter} />

      {query.isError ? (
        <div className="rounded-md border border-crimson/30 bg-crimson-soft px-4 py-3 text-[13px] text-crimson">
          {t("audit.error", {
            defaultValue: "Failed to load audit rows: {{msg}}",
            msg: String(query.error),
          })}
        </div>
      ) : (
        <AuditTable page={query.data} />
      )}

      {query.data?.next_offset != null ? (
        <div className="flex justify-center pt-2">
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() =>
              setFilter((cur) => ({
                ...cur,
                offset: query.data?.next_offset ?? 0,
              }))
            }
          >
            {t("audit.load_more", { defaultValue: "Load next page" })}
          </button>
        </div>
      ) : null}
    </div>
  )
}
