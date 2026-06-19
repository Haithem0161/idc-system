import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import { SourceVisitsTable } from "@/components/accounting/source-visits-table"
import { StatStrip } from "@/components/accounting/stat-strip"
import { DetailHeader, DetailSection } from "@/components/accounting/detail-chrome"
import { useOperatorDrilldown } from "@/features/reports/queries"
import { formatHours, formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

/**
 * Operator detail pane: stat strip (visits / dye / cut total / hours), the
 * shifts worked in the window, and the visits attributed to the operator.
 * Driven by `reports_operator_drilldown`.
 */
export function OperatorDetailPane ({ operatorId }: { operatorId: string }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const detail = useOperatorDrilldown(operatorId, range)

  if (detail.isLoading || !detail.data) {
    return <DetailSkeleton />
  }
  const o = detail.data

  return (
    <div className="space-y-4">
      <DetailHeader
        eyebrow={[t("accounting.explorer.entity.operators_singular", { defaultValue: "Operator" })]}
        title={o.name}
      />

      <StatStrip
        items={[
          {
            label: t("accounting.operators.col.visits", { defaultValue: "Visits" }),
            value: String(o.totals.visits),
          },
          {
            label: t("accounting.operators.col.hours", { defaultValue: "Hours" }),
            value: formatHours(o.total_hours_milli),
          },
          {
            label: t("accounting.operators.col.cut_total", { defaultValue: "Cut total" }),
            value: formatIqd(o.totals.operator_cut_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.operators.col.net", { defaultValue: "Net" }),
            value: formatIqd(o.totals.net_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
        ]}
      />

      <DetailSection
        title={t("accounting.operators.shifts.title", { defaultValue: "Shifts in window" })}
        meta={formatHours(o.total_hours_milli)}
      >
        {o.shifts.length === 0 ? (
          <Empty label={t("accounting.operators.shifts.empty", { defaultValue: "No shifts in range." })} />
        ) : (
          <div className="overflow-hidden rounded-lg border border-line">
            <table className="data-table w-full">
              <thead>
                <tr>
                  <th className="text-start">{t("accounting.operators.shifts.columns.date", { defaultValue: "Date" })}</th>
                  <th className="text-start">{t("accounting.operators.shifts.columns.check_in", { defaultValue: "Check-in" })}</th>
                  <th className="text-start">{t("accounting.operators.shifts.columns.check_out", { defaultValue: "Check-out" })}</th>
                  <th className="text-end">{t("accounting.operators.shifts.columns.duration", { defaultValue: "Duration" })}</th>
                  <th className="text-end">{t("accounting.operators.shifts.columns.lines_run", { defaultValue: "Lines" })}</th>
                  <th className="text-end">{t("accounting.operators.shifts.columns.cut", { defaultValue: "Cut earned" })}</th>
                </tr>
              </thead>
              <tbody>
                {o.shifts.map((s) => (
                  <tr key={s.shift_id}>
                    <td className="font-mono">{s.check_in_at.slice(0, 10)}</td>
                    <td className="font-mono">{s.check_in_at.slice(11, 16)}</td>
                    <td className="font-mono">{s.check_out_at ? s.check_out_at.slice(11, 16) : "—"}</td>
                    <td className="text-end font-mono tabular-nums">
                      {s.duration_milli != null ? formatHours(s.duration_milli) : "—"}
                    </td>
                    <td className="text-end font-mono tabular-nums">{s.lines_run}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(s.cut_earned_iqd, { locale })}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </DetailSection>

      <DetailSection
        title={t("accounting.operators.visits.title", { defaultValue: "Attributed visits" })}
        meta={t("accounting.tops.visits_count", {
          defaultValue: "{{count}} visits",
          count: o.totals.visits,
        })}
      >
        <SourceVisitsTable
          rows={o.attributed_visits}
          locale={locale}
          emptyLabel={t("accounting.operators.visits.empty", { defaultValue: "No visits in range." })}
        />
      </DetailSection>
    </div>
  )
}

function Empty ({ label }: { label: string }) {
  return (
    <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
      {label}
    </div>
  )
}

function DetailSkeleton () {
  return (
    <div className="space-y-4">
      <div className="h-8 w-48 animate-pulse rounded bg-paper-2" />
      <div className="h-[88px] animate-pulse rounded-lg bg-paper-2" />
      <div className="h-[200px] animate-pulse rounded-lg bg-paper-2" />
    </div>
  )
}
