import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { NavLink, useParams } from "react-router"

import { DateRangePicker } from "@/components/accounting/date-range-picker"
import { IncludeVoidedToggle } from "@/components/accounting/include-voided-toggle"
import { useDoctorDrilldown } from "@/features/reports/queries"
import { formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

export default function AccountingDoctorDetailPage () {
  const { t, i18n } = useTranslation()
  const params = useParams<{ id: string }>()
  const id = params.id === "house" ? null : params.id ?? null
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const detail = useDoctorDrilldown(id, range)
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"

  if (!detail.data) {
    return <div className="h-[200px] animate-pulse rounded-lg bg-paper-2" />
  }
  const d = detail.data

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="eyebrow">
            <NavLink to="/accounting/doctors" className="hover:underline">
              {t("accounting.doctors.title", { defaultValue: "Doctor earnings" })}
            </NavLink>{" "}
            ·{" "}
            <span>{d.specialty ?? t("accounting.doctors.no_specialty", { defaultValue: "No specialty" })}</span>
          </div>
          <h1 className="mt-1 text-[28px] font-bold tracking-tight text-ink">{d.name}</h1>
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-4">
        <DateRangePicker />
        <IncludeVoidedToggle />
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-4">
        <Stat label={t("accounting.doctors.col.visits", { defaultValue: "Visits" })} value={String(d.totals.visits)} />
        <Stat
          label={t("accounting.doctors.col.revenue", { defaultValue: "Revenue" })}
          value={formatIqd(d.totals.revenue_iqd, { locale, withSuffix: true })}
        />
        <Stat
          label={t("accounting.doctors.col.cut_total", { defaultValue: "Cut total" })}
          value={formatIqd(d.totals.doctor_cut_iqd, { locale, withSuffix: true })}
        />
        <Stat
          label={t("accounting.doctors.col.net", { defaultValue: "Net" })}
          value={formatIqd(d.totals.net_iqd, { locale, withSuffix: true })}
        />
      </div>

      <section className="space-y-3">
        <h2 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
          {t("accounting.doctors.breakdown.title", { defaultValue: "Per-check breakdown" })}
        </h2>
        {d.per_check.length === 0 ? (
          <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
            {t("accounting.doctors.breakdown.empty", { defaultValue: "No checks in range." })}
          </div>
        ) : (
          <div className="overflow-hidden rounded-lg border border-line">
            <table className="data-table w-full">
              <thead>
                <tr>
                  <th className="text-start">{t("accounting.doctors.breakdown.columns.check", { defaultValue: "Check" })}</th>
                  <th className="text-start">{t("accounting.doctors.breakdown.columns.subtype", { defaultValue: "Subtype" })}</th>
                  <th className="text-end">{t("accounting.doctors.breakdown.columns.visits", { defaultValue: "Visits" })}</th>
                  <th className="text-end">{t("accounting.doctors.breakdown.columns.revenue", { defaultValue: "Revenue" })}</th>
                  <th className="text-end">{t("accounting.doctors.breakdown.columns.cut", { defaultValue: "Cut" })}</th>
                  <th className="text-end">{t("accounting.doctors.breakdown.columns.avg_cut", { defaultValue: "Avg cut" })}</th>
                </tr>
              </thead>
              <tbody>
                {d.per_check.map((row) => (
                  <tr key={`${row.check_type_id}:${row.check_subtype_id ?? ""}`}>
                    <td>{row.check_type_name_en ?? row.check_type_name_ar}</td>
                    <td className="text-ink-3">
                      {row.check_subtype_name_en ?? row.check_subtype_name_ar ?? "—"}
                    </td>
                    <td className="text-end font-mono tabular-nums">{row.visits}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(row.revenue_iqd, { locale })}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(row.doctor_cut_iqd, { locale })}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(row.avg_cut_iqd, { locale })}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="space-y-3">
        <h2 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
          {t("accounting.doctors.source_visits.title", { defaultValue: "Source visits" })}
        </h2>
        {d.source_visits.length === 0 ? (
          <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
            {t("accounting.doctors.source_visits.empty", { defaultValue: "No visits in range." })}
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
                  <th className="text-start">{t("accounting.visits.col.operator", { defaultValue: "Operator" })}</th>
                  <th className="text-end">{t("accounting.visits.col.price", { defaultValue: "Price" })}</th>
                  <th className="text-end">{t("accounting.visits.col.doctor_cut", { defaultValue: "Doc cut" })}</th>
                </tr>
              </thead>
              <tbody>
                {d.source_visits.map((v) => (
                  <tr key={v.visit_id}>
                    <td className="font-mono">{v.locked_at?.slice(0, 10) ?? "—"}</td>
                    <td className="font-mono">{v.visit_id.slice(-6)}</td>
                    <td>
                      <NavLink
                        to={`/accounting/visits/${v.visit_id}`}
                        className="text-ink hover:underline"
                      >
                        {v.patient_name}
                      </NavLink>
                    </td>
                    <td>{v.check_type_name_en ?? v.check_type_name_ar}</td>
                    <td>{v.operator_name}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(v.price_iqd, { locale })}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(v.doctor_cut_iqd, { locale })}</td>
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
      <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-ink-3">
        {label}
      </div>
      <div className="mt-1 font-mono text-[20px] tabular-nums text-ink">{value}</div>
    </div>
  )
}
