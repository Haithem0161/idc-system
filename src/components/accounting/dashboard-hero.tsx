import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"
import { ArrowRight } from "lucide-react"

import { formatIqd, formatPermille } from "@/lib/format/money"
import type { DashboardKpisRecord, TrendMatrixRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

interface HeroTile {
  key: keyof TrendMatrixRecord
  labelKey: string
  fallback: string
  amount: number
  href: string
  ink?: boolean
}

/**
 * The dashboard KPI band. Five tiles -- revenue, doctor cuts, operator cuts,
 * inventory value, net -- each a drill link into the relevant explorer view
 * (or daily close for net). The trend delta shown is the week-over-week
 * comparison from `trend_week_vs_last_week`, matching the dashboard's default
 * "this week" framing. The net tile flips to the dark ink scheme.
 */
export function DashboardHero ({ kpis }: { kpis: DashboardKpisRecord }) {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const trend = kpis.trend_week_vs_last_week

  const tiles: HeroTile[] = [
    {
      key: "revenue",
      labelKey: "accounting.kpi.revenue",
      fallback: "Revenue",
      amount: kpis.revenue_iqd,
      href: "/accounting/explore/visits",
    },
    {
      key: "doctor_cuts",
      labelKey: "accounting.kpi.doctor_cuts",
      fallback: "Doctor cuts",
      amount: kpis.doctor_cuts_iqd,
      href: "/accounting/explore/doctors",
    },
    {
      key: "operator_cuts",
      labelKey: "accounting.kpi.operator_cuts",
      fallback: "Operator cuts",
      amount: kpis.operator_cuts_iqd,
      href: "/accounting/explore/operators",
    },
    {
      key: "inventory_value",
      labelKey: "accounting.kpi.inventory_value",
      fallback: "Inventory value",
      amount: kpis.inventory_consumption_value_iqd,
      href: "/accounting/explore/checks",
    },
    {
      key: "net",
      labelKey: "accounting.kpi.net",
      fallback: "Net",
      amount: kpis.net_iqd,
      href: "/accounting/daily-close",
      ink: true,
    },
  ]

  return (
    <div className="grid grid-cols-1 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-2 lg:grid-cols-5">
      {tiles.map((tile) => {
        const delta = trend[tile.key].delta_permille
        return (
          <button
            key={tile.key}
            type="button"
            onClick={() => navigate(tile.href)}
            className={cn(
              "group relative cursor-pointer p-5 text-start transition-colors",
              tile.ink ? "bg-ink hover:bg-ink-2" : "bg-surface hover:bg-paper"
            )}
          >
            <ArrowRight
              aria-hidden
              strokeWidth={2}
              className={cn(
                "absolute end-4 top-4 h-3.5 w-3.5 opacity-0 transition-opacity group-hover:opacity-100 rtl:rotate-180",
                tile.ink ? "text-paper/60" : "text-ink-4"
              )}
            />
            <div
              className={cn(
                "text-[10px] font-semibold uppercase tracking-[0.1em]",
                tile.ink ? "text-paper/60" : "text-ink-3"
              )}
            >
              {t(tile.labelKey, { defaultValue: tile.fallback })}
            </div>
            <div
              className={cn(
                "mt-2 font-mono text-[28px] font-bold tracking-tight tabular-nums",
                tile.ink ? "text-paper" : "text-ink"
              )}
            >
              {formatIqd(tile.amount, { locale })}
              <span
                className={cn(
                  "ms-1 text-[13px] font-medium",
                  tile.ink ? "text-paper/50" : "text-ink-3"
                )}
              >
                {t("accounting.currency_suffix", { defaultValue: "IQD" })}
              </span>
            </div>
            <div
              className={cn(
                "mt-1.5 text-[11px] font-semibold tabular-nums",
                delta > 0
                  ? tile.ink
                    ? "text-[#34D399]"
                    : "text-success"
                  : delta < 0
                    ? tile.ink
                      ? "text-[#FCA5A5]"
                      : "text-crimson"
                    : tile.ink
                      ? "text-paper/50"
                      : "text-ink-3"
              )}
            >
              {formatPermille(delta)}{" "}
              <span className="font-medium opacity-70">
                {t("accounting.kpi.vs_last_week", { defaultValue: "vs last week" })}
              </span>
            </div>
          </button>
        )
      })}
    </div>
  )
}
