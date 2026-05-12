import { useTranslation } from "react-i18next"

import { cn } from "@/lib/utils"
import {
  useAccountingFiltersStore,
  type AccountingRangePreset,
} from "@/stores/accounting-filters-store"

const PRESETS: AccountingRangePreset[] = [
  "today",
  "yesterday",
  "last_7d",
  "month",
  "last_month",
  "custom",
]

/**
 * Editorial filter-pill segmented control. Plays the role of the
 * `<DateRangePicker>` declared in phase-07 §3 (today / yesterday / 7d /
 * month / last month / custom).
 */
export function DateRangePicker () {
  const { t } = useTranslation()
  const preset = useAccountingFiltersStore((s) => s.preset)
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const setPreset = useAccountingFiltersStore((s) => s.setPreset)
  const setCustomRange = useAccountingFiltersStore((s) => s.setCustomRange)

  return (
    <div className="flex flex-wrap items-center gap-3">
      <div
        role="tablist"
        aria-label={t("accounting.range.aria_label", { defaultValue: "Date range" })}
        className="inline-flex gap-1 rounded-md border border-line bg-paper-2 p-1"
      >
        {PRESETS.map((p) => (
          <button
            key={p}
            type="button"
            role="tab"
            aria-selected={preset === p}
            onClick={() => setPreset(p)}
            className={cn(
              "rounded px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] transition-colors",
              preset === p
                ? "bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
                : "text-ink-3 hover:text-ink"
            )}
          >
            {t(`accounting.range.${p}`, { defaultValue: p })}
          </button>
        ))}
      </div>

      {preset === "custom" ? (
        <div className="flex items-center gap-2">
          <input
            type="date"
            value={fromDate}
            onChange={(e) => setCustomRange(e.target.value, toDate)}
            className="input h-9 px-2 py-1 text-[12px]"
            aria-label={t("accounting.range.from", { defaultValue: "From" })}
          />
          <span className="text-[11px] text-ink-3">
            {t("accounting.range.to_separator", { defaultValue: "→" })}
          </span>
          <input
            type="date"
            value={toDate}
            onChange={(e) => setCustomRange(fromDate, e.target.value)}
            className="input h-9 px-2 py-1 text-[12px]"
            aria-label={t("accounting.range.to", { defaultValue: "To" })}
          />
        </div>
      ) : (
        <span className="font-mono text-[11px] text-ink-3">
          {fromDate} → {toDate}
        </span>
      )}
    </div>
  )
}
