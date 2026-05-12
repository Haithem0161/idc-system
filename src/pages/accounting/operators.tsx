import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"
import { save } from "@tauri-apps/plugin-dialog"

import { DateRangePicker } from "@/components/accounting/date-range-picker"
import { IncludeVoidedToggle } from "@/components/accounting/include-voided-toggle"
import { useExportOperatorsCsv, useOperatorEarnings } from "@/features/reports/queries"
import { formatHours, formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

export default function AccountingOperatorsPage () {
  const { t, i18n } = useTranslation()
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const rows = useOperatorEarnings(range)
  const exportMutation = useExportOperatorsCsv()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"

  const onExport = async () => {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19)
    const slug = `operator-earnings_${fromDate}_${toDate}_${stamp}.csv`
    const path = await save({
      defaultPath: slug,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    })
    if (!path) return
    await exportMutation.mutateAsync({
      from_utc: range.from_utc,
      to_utc: range.to_utc,
      include_voided: includeVoided,
      path,
    })
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="eyebrow">
            {t("accounting.operators.eyebrow", { defaultValue: "Earnings" })}
          </div>
          <h1 className="mt-1 text-[28px] font-bold tracking-tight text-ink">
            {t("accounting.operators.title", { defaultValue: "Operator earnings" })}
          </h1>
        </div>
        <button
          type="button"
          onClick={onExport}
          disabled={exportMutation.isPending}
          className="btn btn-ghost btn-sm"
        >
          {exportMutation.isPending
            ? t("accounting.actions.exporting", { defaultValue: "Exporting…" })
            : t("accounting.actions.export_csv", { defaultValue: "Export CSV" })}
        </button>
      </header>

      <div className="flex flex-wrap items-center gap-4">
        <DateRangePicker />
        <IncludeVoidedToggle />
      </div>

      {rows.data ? (
        rows.data.length === 0 ? (
          <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
            {t("accounting.operators.empty", { defaultValue: "No earnings in range." })}
          </div>
        ) : (
          <div className="overflow-hidden rounded-lg border border-line">
            <table className="data-table w-full">
              <thead>
                <tr>
                  <th className="text-start">{t("accounting.operators.col.operator", { defaultValue: "Operator" })}</th>
                  <th className="text-end">{t("accounting.operators.col.visits", { defaultValue: "Visits" })}</th>
                  <th className="text-end">{t("accounting.operators.col.dye_visits", { defaultValue: "Dye" })}</th>
                  <th className="text-end">{t("accounting.operators.col.cut_total", { defaultValue: "Cut total" })}</th>
                  <th className="text-end">{t("accounting.operators.col.hours", { defaultValue: "Hours" })}</th>
                  <th className="text-end">{t("accounting.operators.col.avg_per_hour", { defaultValue: "Avg/hr" })}</th>
                </tr>
              </thead>
              <tbody>
                {rows.data.map((o) => (
                  <tr key={o.operator_id}>
                    <td>
                      <NavLink
                        to={`/accounting/operators/${o.operator_id}`}
                        className="text-ink hover:underline"
                      >
                        {o.name || o.operator_id}
                      </NavLink>
                    </td>
                    <td className="text-end font-mono tabular-nums">{o.visits}</td>
                    <td className="text-end font-mono tabular-nums">{o.visits_with_dye}</td>
                    <td className="text-end font-mono tabular-nums">
                      {formatIqd(o.operator_cut_total_iqd, { locale })}
                    </td>
                    <td className="text-end font-mono tabular-nums">
                      {formatHours(o.hours_on_shift_milli)}
                    </td>
                    <td className="text-end font-mono tabular-nums">
                      {formatIqd(o.avg_cut_per_hour_iqd, { locale })}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      ) : (
        <div className="h-[200px] animate-pulse rounded-lg bg-paper-2" />
      )}
    </div>
  )
}
