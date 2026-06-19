import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Lock, Unlock } from "lucide-react"

import { useFrozenCloseList } from "@/features/reports/queries"
import { formatIqd } from "@/lib/format/money"
import { cn } from "@/lib/utils"

/** Local YYYY-MM-DD for a Date. */
function ymd (d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

/**
 * Compact overview of the last `days` calendar days: which are frozen (signed),
 * which were reopened, and which are still open. Clicking a day selects it. Lets
 * the accountant see gaps at a glance instead of stepping day by day.
 */
export function CloseMonthOverview ({
  selectedDate,
  onSelect,
  days = 14,
}: {
  selectedDate: string
  onSelect: (date: string) => void
  days?: number
}) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"

  // The calendar window: [today-(days-1) .. today], newest first.
  const dayList = useMemo(() => {
    const out: string[] = []
    const base = new Date()
    for (let i = 0; i < days; i++) {
      const d = new Date(base)
      d.setDate(base.getDate() - i)
      out.push(ymd(d))
    }
    return out
  }, [days])

  const fromDate = dayList[dayList.length - 1]
  const toDate = dayList[0]
  const closes = useFrozenCloseList(fromDate, toDate)

  // Index the in-force close per day (reopened rows are ignored for the badge,
  // but a day that has ONLY reopened rows reads as "reopened").
  const byDay = useMemo(() => {
    const map = new Map<string, { frozen: boolean; reopened: boolean; net: number }>()
    for (const c of closes.data ?? []) {
      const existing = map.get(c.target_date)
      const inForce = c.reopened_at === null
      map.set(c.target_date, {
        frozen: inForce || (existing?.frozen ?? false),
        reopened: (existing?.reopened ?? false) || c.reopened_at !== null,
        net: inForce ? c.net_iqd : (existing?.net ?? c.net_iqd),
      })
    }
    return map
  }, [closes.data])

  return (
    <div className="panel">
      <div className="panel-head flex items-center justify-between">
        <span className="panel-title">
          {t("accounting.daily_close.overview.title", { defaultValue: "Recent days" })}
        </span>
        <span className="text-[11px] text-ink-3">
          {t("accounting.daily_close.overview.legend", {
            defaultValue: "Frozen · Open",
          })}
        </span>
      </div>
      <div className="max-h-[320px] divide-y divide-line overflow-y-auto">
        {dayList.map((day) => {
          const info = byDay.get(day)
          const isFrozen = info?.frozen ?? false
          const wasReopened = (info?.reopened ?? false) && !isFrozen
          const selected = day === selectedDate
          return (
            <button
              key={day}
              type="button"
              onClick={() => onSelect(day)}
              className={cn(
                "flex w-full items-center justify-between gap-3 border-s-[3px] px-4 py-2.5 text-start transition-colors",
                selected
                  ? "border-s-crimson bg-surface"
                  : "border-s-transparent hover:bg-paper-2"
              )}
            >
              <span className="flex items-center gap-2">
                {isFrozen ? (
                  <Lock className="h-3.5 w-3.5 text-success" strokeWidth={2} aria-hidden />
                ) : (
                  <Unlock className="h-3.5 w-3.5 text-ink-4" strokeWidth={2} aria-hidden />
                )}
                <span className="font-mono text-[12px] tabular-nums text-ink-2">{day}</span>
              </span>
              <span className="flex items-center gap-2">
                {info ? (
                  <span className="font-mono text-[11px] tabular-nums text-ink-3">
                    {formatIqd(info.net, { locale })}
                  </span>
                ) : null}
                <StatusTag isFrozen={isFrozen} wasReopened={wasReopened} />
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}

function StatusTag ({ isFrozen, wasReopened }: { isFrozen: boolean; wasReopened: boolean }) {
  const { t } = useTranslation()
  if (isFrozen) {
    return (
      <span className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.04em] text-success">
        {t("accounting.daily_close.overview.frozen", { defaultValue: "Frozen" })}
      </span>
    )
  }
  if (wasReopened) {
    return (
      <span className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.04em] text-gold">
        {t("accounting.daily_close.overview.reopened", { defaultValue: "Reopened" })}
      </span>
    )
  }
  return (
    <span className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.04em] text-ink-4">
      {t("accounting.daily_close.overview.open", { defaultValue: "Open" })}
    </span>
  )
}
