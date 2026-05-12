import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import { DateRangePicker } from "@/components/accounting/date-range-picker"
import { IncludeVoidedToggle } from "@/components/accounting/include-voided-toggle"
import { KpiCard } from "@/components/accounting/kpi-card"
import { TrendMatrix } from "@/components/accounting/trend-matrix"
import { useDashboardKpis, useDashboardTops } from "@/features/reports/queries"
import { formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

export default function AccountingDashboardPage () {
  const { t, i18n } = useTranslation()
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const kpis = useDashboardKpis(range)
  const tops = useDashboardTops(range)

  const today = new Date()
  const localeDate = today.toLocaleDateString(
    i18n.language === "ar" ? "ar-IQ" : "en-GB",
    { weekday: "long", day: "numeric", month: "long", year: "numeric" }
  )

  return (
    <div className="space-y-6">
      <header>
        <div className="eyebrow">
          {localeDate.toUpperCase()} ·{" "}
          {t("accounting.dashboard.eyebrow", { defaultValue: "Accounting" })}
        </div>
        <h1 className="mt-1 text-[28px] font-bold tracking-tight text-ink">
          {t("accounting.dashboard.title", { defaultValue: "Dashboard" })}
        </h1>
      </header>

      <div className="flex flex-wrap items-center gap-4">
        <DateRangePicker />
        <IncludeVoidedToggle />
      </div>

      {kpis.error ? (
        <div className="rounded-md border border-crimson/30 bg-crimson-soft px-4 py-3 text-[12px] text-crimson">
          {t("accounting.errors.kpis_failed", {
            defaultValue: "Could not load accounting KPIs.",
          })}
        </div>
      ) : null}

      {kpis.data ? (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5">
            <KpiCard
              label={t("accounting.kpi.revenue", { defaultValue: "Revenue" })}
              amount={kpis.data.revenue_iqd}
              deltaPermille={kpis.data.trend_today_vs_yesterday.revenue.delta_permille}
            />
            <KpiCard
              label={t("accounting.kpi.doctor_cuts", { defaultValue: "Doctor cuts" })}
              amount={kpis.data.doctor_cuts_iqd}
              deltaPermille={kpis.data.trend_today_vs_yesterday.doctor_cuts.delta_permille}
            />
            <KpiCard
              label={t("accounting.kpi.operator_cuts", { defaultValue: "Operator cuts" })}
              amount={kpis.data.operator_cuts_iqd}
              deltaPermille={kpis.data.trend_today_vs_yesterday.operator_cuts.delta_permille}
            />
            <KpiCard
              label={t("accounting.kpi.inventory_value", { defaultValue: "Inventory value" })}
              amount={kpis.data.inventory_consumption_value_iqd}
              deltaPermille={kpis.data.trend_today_vs_yesterday.inventory_value.delta_permille}
            />
            <KpiCard
              label={t("accounting.kpi.net", { defaultValue: "Net" })}
              amount={kpis.data.net_iqd}
              deltaPermille={kpis.data.trend_today_vs_yesterday.net.delta_permille}
              tone="ink"
            />
          </div>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
            <TrendMatrix
              title={t("accounting.trends.today_vs_yesterday", {
                defaultValue: "Today vs Yesterday",
              })}
              matrix={kpis.data.trend_today_vs_yesterday}
            />
            <TrendMatrix
              title={t("accounting.trends.this_week_vs_last", {
                defaultValue: "This Week vs Last",
              })}
              matrix={kpis.data.trend_week_vs_last_week}
            />
            <TrendMatrix
              title={t("accounting.trends.this_month_vs_last", {
                defaultValue: "This Month vs Last",
              })}
              matrix={kpis.data.trend_month_vs_last_month}
            />
          </div>
        </>
      ) : (
        <SkeletonGrid />
      )}

      {tops.data ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <TopList
            title={t("accounting.tops.doctors", { defaultValue: "Top Doctors" })}
            rows={tops.data.top_doctors.map((d) => ({
              label: d.name,
              value: formatIqd(d.revenue_iqd, { withSuffix: true }),
              href: `/accounting/doctors/${d.doctor_id ?? "house"}`,
            }))}
          />
          <TopList
            title={t("accounting.tops.operators", { defaultValue: "Top Operators" })}
            rows={tops.data.top_operators.map((o) => ({
              label: o.name || o.operator_id,
              value: `${o.visits} visits`,
              href: `/accounting/operators/${o.operator_id}`,
            }))}
          />
          <TopList
            title={t("accounting.tops.check_types", { defaultValue: "Top Check Types" })}
            rows={tops.data.top_check_types.map((c) => ({
              label: c.name_en ?? c.name_ar,
              value: formatIqd(c.revenue_iqd, { withSuffix: true }),
              href: `/accounting/visits?check_type_id=${c.check_type_id}`,
            }))}
          />
        </div>
      ) : null}
    </div>
  )
}

function SkeletonGrid () {
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-5">
      {Array.from({ length: 5 }).map((_, i) => (
        <div key={i} className="h-[110px] animate-pulse rounded-lg bg-paper-2" />
      ))}
    </div>
  )
}

function TopList ({
  title,
  rows,
}: {
  title: string
  rows: Array<{ label: string; value: string; href: string }>
}) {
  return (
    <div className="rounded-lg border border-line bg-surface p-5">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
        {title}
      </div>
      {rows.length === 0 ? (
        <div className="mt-3 text-[12px] text-ink-3">
          <span>—</span>
        </div>
      ) : (
        <ul className="mt-3 space-y-1.5">
          {rows.map((r, i) => (
            <li key={i}>
              <NavLink
                to={r.href}
                className="flex items-center justify-between rounded p-1.5 text-[13px] text-ink-2 hover:bg-paper"
              >
                <span className="truncate">{r.label}</span>
                <span className="ms-2 font-mono tabular-nums text-ink-3">{r.value}</span>
              </NavLink>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
