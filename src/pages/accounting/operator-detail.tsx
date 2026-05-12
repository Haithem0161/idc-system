import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { NavLink, useParams } from "react-router"

import { DateRangePicker } from "@/components/accounting/date-range-picker"
import { IncludeVoidedToggle } from "@/components/accounting/include-voided-toggle"
import { useOperatorDrilldown } from "@/features/reports/queries"
import { formatHours, formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

export default function AccountingOperatorDetailPage () {
  const { t, i18n } = useTranslation()
  const params = useParams<{ id: string }>()
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const detail = useOperatorDrilldown(params.id ?? null, range)
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"

  if (!detail.data) {
    return <div className="h-[200px] animate-pulse rounded-lg bg-paper-2" />
  }
  const o = detail.data

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="eyebrow">
            <NavLink to="/accounting/operators" className="hover:underline">
              {t("accounting.operators.title", { defaultValue: "Operator earnings" })}
            </NavLink>
          </div>
          <h1 className="mt-1 text-[28px] font-bold tracking-tight text-ink">{o.name}</h1>
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-4">
        <DateRangePicker />
        <IncludeVoidedToggle />
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-4">
        <Stat label={t("accounting.operators.col.visits", { defaultValue: "Visits" })} value={String(o.totals.visits)} />
        <Stat
          label={t("accounting.operators.col.cut_total", { defaultValue: "Cut total" })}
          value={formatIqd(o.totals.operator_cut_iqd, { locale, withSuffix: true })}
        />
        <Stat
          label={t("accounting.operators.col.hours", { defaultValue: "Hours" })}
          value={formatHours(o.total_hours_milli)}
        />
        <Stat
          label={t("accounting.operators.col.net", { defaultValue: "Net" })}
          value={formatIqd(o.totals.net_iqd, { locale, withSuffix: true })}
        />
      </div>

      <section className="space-y-3">
        <h2 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
          {t("accounting.operators.shifts.title", { defaultValue: "Shifts in window" })}
        </h2>
        {o.shifts.length === 0 ? (
          <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
            {t("accounting.operators.shifts.empty", { defaultValue: "No shifts in range." })}
          </div>
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
                    <td className="font-mono">
                      {s.check_out_at ? s.check_out_at.slice(11, 16) : "—"}
                    </td>
                    <td className="text-end font-mono tabular-nums">
                      {s.duration_milli != null ? formatHours(s.duration_milli) : "—"}
                    </td>
                    <td className="text-end font-mono tabular-nums">{s.lines_run}</td>
                    <td className="text-end font-mono tabular-nums">
                      {formatIqd(s.cut_earned_iqd, { locale })}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="space-y-3">
        <h2 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
          {t("accounting.operators.visits.title", { defaultValue: "Attributed visits" })}
        </h2>
        {o.attributed_visits.length === 0 ? (
          <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
            {t("accounting.operators.visits.empty", { defaultValue: "No visits in range." })}
          </div>
        ) : (
          <div className="overflow-hidden rounded-lg border border-line">
            <table className="data-table w-full">
              <thead>
                <tr>
                  <th className="text-start">{t("accounting.visits.col.date", { defaultValue: "Date" })}</th>
                  <th className="text-start">{t("accounting.visits.col.visit_no", { defaultValue: "Visit #" })}</th>
                  <th className="text-start">{t("accounting.visits.col.patient", { defaultValue: "Patient" })}</th>
                  <th className="text-start">{t("accounting.visits.col.check", { defaultValue: "Check" })}</th>
                  <th className="text-start">{t("accounting.visits.col.doctor", { defaultValue: "Doctor" })}</th>
                  <th className="text-center">{t("accounting.visits.col.dye", { defaultValue: "Dye" })}</th>
                  <th className="text-end">{t("accounting.visits.col.operator_cut", { defaultValue: "Op cut" })}</th>
                </tr>
              </thead>
              <tbody>
                {o.attributed_visits.map((v) => (
                  <tr key={v.visit_id}>
                    <td className="font-mono">{v.locked_at?.slice(0, 10) ?? "—"}</td>
                    <td className="font-mono">{v.visit_id.slice(-6)}</td>
                    <td>
                      <NavLink to={`/accounting/visits/${v.visit_id}`} className="text-ink hover:underline">
                        {v.patient_name}
                      </NavLink>
                    </td>
                    <td>{v.check_type_name_en ?? v.check_type_name_ar}</td>
                    <td>{v.doctor_name ?? <span className="text-ink-3">(house)</span>}</td>
                    <td className="text-center text-ink-3">{v.dye ? "Y" : "—"}</td>
                    <td className="text-end font-mono tabular-nums">
                      {formatIqd(v.operator_cut_iqd, { locale })}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  )
}

function Stat ({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-surface p-4">
      <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-ink-3">{label}</div>
      <div className="mt-1 font-mono text-[20px] tabular-nums text-ink">{value}</div>
    </div>
  )
}
