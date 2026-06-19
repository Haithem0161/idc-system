import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Stethoscope, Users, Boxes } from "lucide-react"

import { AccountingToolbar } from "@/components/accounting/accounting-toolbar"
import { DashboardHero } from "@/components/accounting/dashboard-hero"
import {
  LeaderboardCard,
  type LeaderboardRow,
} from "@/components/accounting/leaderboard-card"
import { abbreviateIqd, thousandsK } from "@/components/accounting/format-abbrev"
import { TrendMatrix } from "@/components/accounting/trend-matrix"
import { useDashboardKpis, useDashboardTops } from "@/features/reports/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { doctorIdToSegment } from "@/components/accounting/entity-link"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

export default function AccountingDashboardPage () {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar" : "en"
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

  const doctorRows: LeaderboardRow[] = (tops.data?.top_doctors ?? []).map((d) => ({
    id: doctorIdToSegment(d.doctor_id),
    name: d.doctor_id
      ? d.name
      : t("accounting.house.label", { defaultValue: "Internal" }),
    sub: [
      d.specialty ?? t("accounting.doctors.no_specialty", { defaultValue: "No specialty" }),
      t("accounting.tops.visits_count", { defaultValue: "{{count}} visits", count: d.visits }),
    ].join(" · "),
    primary: abbreviateIqd(d.doctor_cut_total_iqd),
    secondary: t("accounting.tops.per_visit", {
      defaultValue: "{{value}} / visit",
      value: thousandsK(d.avg_cut_per_visit_iqd),
    }),
    href: `/accounting/explore/doctors/${doctorIdToSegment(d.doctor_id)}`,
    house: !d.doctor_id,
  }))

  const operatorRows: LeaderboardRow[] = (tops.data?.top_operators ?? []).map((o) => ({
    id: o.operator_id,
    name: o.name || o.operator_id,
    sub: [
      t("accounting.tops.visits_count", { defaultValue: "{{count}} visits", count: o.visits }),
      t("accounting.tops.dye_count", { defaultValue: "{{count}} dye", count: o.visits_with_dye }),
    ].join(" · "),
    primary: abbreviateIqd(o.operator_cut_total_iqd),
    secondary: t("accounting.tops.per_hour", {
      defaultValue: "{{value}} / hr",
      value: thousandsK(o.avg_cut_per_hour_iqd),
    }),
    href: `/accounting/explore/operators/${o.operator_id}`,
  }))

  const checkRows: LeaderboardRow[] = (tops.data?.top_check_types ?? []).map((c) => ({
    id: c.check_type_id,
    name: resolveLocaleName(c, locale),
    sub: t("accounting.tops.visits_count", { defaultValue: "{{count}} visits", count: c.visits }),
    primary: abbreviateIqd(c.revenue_iqd),
    secondary: t("accounting.tops.doc_cut_per_visit", {
      defaultValue: "{{value}} doc/v",
      value: thousandsK(c.visits > 0 ? c.doctor_cut_iqd / c.visits : 0),
    }),
    href: `/accounting/explore/checks/${c.check_type_id}`,
  }))

  return (
    <div className="space-y-6">
      <header>
        <div className="eyebrow">
          {localeDate.toUpperCase()} ·{" "}
          {t("accounting.dashboard.eyebrow", { defaultValue: "Accounting" })}
        </div>
        <h1 className="mt-1 text-[30px] font-bold tracking-tight text-ink">
          {t("accounting.dashboard.title", { defaultValue: "Dashboard" })}
        </h1>
        <p className="mt-1 text-[13px] text-ink-3">
          {t("accounting.dashboard.subtitle", {
            defaultValue: "Every tile and row drills in. Click a KPI or a leaderboard row to open the explorer.",
          })}
        </p>
      </header>

      <AccountingToolbar />

      {kpis.error ? (
        <div className="rounded-md border border-crimson/30 bg-crimson-soft px-4 py-3 text-[12px] text-crimson">
          {t("accounting.errors.kpis_failed", { defaultValue: "Could not load accounting KPIs." })}
        </div>
      ) : null}

      {kpis.data ? (
        <DashboardHero kpis={kpis.data} />
      ) : (
        <div className="grid grid-cols-1 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-2 lg:grid-cols-5">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="h-[112px] animate-pulse bg-paper-2" />
          ))}
        </div>
      )}

      {tops.data ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <LeaderboardCard
            title={t("accounting.tops.doctors", { defaultValue: "Top doctors" })}
            icon={<Stethoscope className="h-4 w-4" strokeWidth={1.8} />}
            allHref="/accounting/explore/doctors"
            allLabel={t("accounting.tops.all", { defaultValue: "All" })}
            rows={doctorRows}
            emptyLabel={t("accounting.doctors.empty", { defaultValue: "No earnings in range." })}
          />
          <LeaderboardCard
            title={t("accounting.tops.operators", { defaultValue: "Top operators" })}
            icon={<Users className="h-4 w-4" strokeWidth={1.8} />}
            allHref="/accounting/explore/operators"
            allLabel={t("accounting.tops.all", { defaultValue: "All" })}
            rows={operatorRows}
            emptyLabel={t("accounting.operators.empty", { defaultValue: "No earnings in range." })}
          />
          <LeaderboardCard
            title={t("accounting.tops.check_types", { defaultValue: "Top check types" })}
            icon={<Boxes className="h-4 w-4" strokeWidth={1.8} />}
            allHref="/accounting/explore/checks"
            allLabel={t("accounting.tops.all", { defaultValue: "All" })}
            rows={checkRows}
            emptyLabel={t("accounting.checks.empty", { defaultValue: "No checks in range." })}
          />
        </div>
      ) : null}

      {kpis.data ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <TrendMatrix
            title={t("accounting.trends.today_vs_yesterday", { defaultValue: "Today vs Yesterday" })}
            matrix={kpis.data.trend_today_vs_yesterday}
          />
          <TrendMatrix
            title={t("accounting.trends.this_week_vs_last", { defaultValue: "This Week vs Last" })}
            matrix={kpis.data.trend_week_vs_last_week}
          />
          <TrendMatrix
            title={t("accounting.trends.this_month_vs_last", { defaultValue: "This Month vs Last" })}
            matrix={kpis.data.trend_month_vs_last_month}
          />
        </div>
      ) : null}
    </div>
  )
}
