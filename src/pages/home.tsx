import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import { KpiCard } from "@/components/accounting/kpi-card"
import { useDashboardKpis, useDashboardTops, useDailyClose } from "@/features/reports/queries"
import { useOpenShifts } from "@/features/shifts/queries"
import { useSyncStatus } from "@/features/sync/queries"
import { useChecksGrid } from "@/features/visits/queries"
import { formatIqd } from "@/lib/format/money"
import type { ChecksGridCardRecord, ShiftWithMetaRecord, UserRoleLiteral } from "@/lib/ipc"
import { cn } from "@/lib/utils"
import { rangeAsUtc } from "@/stores/accounting-filters-store"
import { selectCurrentRole, useAuthStore } from "@/stores/auth-store"

function todayLocalIso (): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

export default function HomePage () {
  const { t, i18n } = useTranslation()
  const state = useAuthStore((s) => s.state)
  const role = useAuthStore(selectCurrentRole)

  const today = new Date()
  const dateStamp = today
    .toLocaleDateString(i18n.language === "ar" ? "ar-IQ" : "en-GB", {
      weekday: "long",
      day: "numeric",
      month: "long",
      year: "numeric",
    })
    .toUpperCase()
  const timeStamp = today.toLocaleTimeString(i18n.language === "ar" ? "ar-IQ" : "en-GB", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  })

  const display = state.kind === "authenticated" ? state.user.name?.trim() || state.user.email : ""
  const firstName = display.split(" ")[0] ?? display

  return (
    <div className="mx-auto max-w-6xl space-y-8">
      <header className="space-y-3 border-b border-line pb-6">
        <span className="eyebrow">
          {dateStamp} · {timeStamp}
        </span>
        <h1 className="text-[30px] font-bold leading-[1.05] tracking-[-0.026em] text-ink">
          {firstName ? t("home.greeting", { name: firstName }) : t("app.title")}
        </h1>
        <p className="max-w-2xl text-[13px] text-ink-3">{t("home.subtitle")}</p>
      </header>

      {role === "accountant" ? <AccountingHome /> : <OperationsHome role={role} />}
    </div>
  )
}

function OperationsHome ({ role }: { role: UserRoleLiteral | null }) {
  const { t, i18n } = useTranslation()
  const sync = useSyncStatus()
  const openShifts = useOpenShifts() as { data?: ShiftWithMetaRecord[]; isLoading: boolean }
  const checks = useChecksGrid()

  const todayVisitsTotal = useMemo(
    () =>
      (checks.data ?? []).reduce(
        (sum: number, c: ChecksGridCardRecord) => sum + (c.todays_visits ?? 0),
        0
      ),
    [checks.data]
  )
  const activeChecks = checks.data?.length ?? 0
  const openShiftCount = openShifts.data?.length ?? 0
  const dayOpen = openShiftCount > 0
  const isRtl = i18n.language === "ar"
  const isAdmin = role === "superadmin"

  return (
    <div className="space-y-8">
      <StatusStrip
        syncStatus={sync.data?.status ?? "offline"}
        pendingOps={sync.data?.pendingOps ?? 0}
        dayLabel={
          dayOpen
            ? t("home.day.open", { count: openShiftCount })
            : t("home.day.no_shifts")
        }
        dayTone={dayOpen ? "is-success" : "is-warn"}
        dayLive={dayOpen}
      />

      <section className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <CountTile label={t("home.tile.visits_today")} value={todayVisitsTotal} isRtl={isRtl} />
        <CountTile label={t("home.tile.open_shifts")} value={openShiftCount} isRtl={isRtl} />
        <CountTile label={t("home.tile.active_check_types")} value={activeChecks} isRtl={isRtl} />
      </section>

      <ChecksTodayPanel
        data={checks.data}
        isLoading={checks.isLoading}
        locale={i18n.language === "ar" ? "ar" : "en"}
      />

      <div className="flex flex-wrap items-center gap-3">
        <NavLink
          to="/reception"
          aria-disabled={!dayOpen}
          onClick={(e) => {
            if (!dayOpen) e.preventDefault()
          }}
          className={cn(
            "btn btn-primary",
            !dayOpen && "pointer-events-none opacity-50"
          )}
          title={dayOpen ? undefined : t("home.action.disabled_no_shift")}
        >
          {t("home.action.new_visit")}
        </NavLink>
        <NavLink to="/reception/shifts" className="btn btn-ghost">
          {t("home.action.open_shifts")}
        </NavLink>
        {isAdmin ? (
          <NavLink to="/admin/users" className="btn btn-ghost">
            {t("home.action.admin")}
          </NavLink>
        ) : null}
      </div>
    </div>
  )
}

function AccountingHome () {
  const { t } = useTranslation()
  const sync = useSyncStatus()
  const todayIso = todayLocalIso()
  const range = useMemo(
    () => ({ ...rangeAsUtc(todayIso, todayIso), include_voided: false }),
    [todayIso]
  )
  const kpis = useDashboardKpis(range)
  const tops = useDashboardTops(range)
  const dailyClose = useDailyClose(todayIso)

  const dayClosed = dailyClose.data ? !dailyClose.data.provisional : false

  return (
    <div className="space-y-8">
      <StatusStrip
        syncStatus={sync.data?.status ?? "offline"}
        pendingOps={sync.data?.pendingOps ?? 0}
        dayLabel={dayClosed ? t("home.day.closed") : t("home.day.open_simple")}
        dayTone={dayClosed ? "is-success" : "is-warn"}
        dayLive={!dayClosed}
      />

      {kpis.data ? (
        <section className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-5">
          <KpiCard
            label={t("home.kpi.revenue")}
            amount={kpis.data.revenue_iqd}
            deltaPermille={kpis.data.trend_today_vs_yesterday.revenue.delta_permille}
          />
          <KpiCard
            label={t("home.kpi.doctor_cuts")}
            amount={kpis.data.doctor_cuts_iqd}
            deltaPermille={kpis.data.trend_today_vs_yesterday.doctor_cuts.delta_permille}
          />
          <KpiCard
            label={t("home.kpi.operator_cuts")}
            amount={kpis.data.operator_cuts_iqd}
            deltaPermille={kpis.data.trend_today_vs_yesterday.operator_cuts.delta_permille}
          />
          <KpiCard
            label={t("home.kpi.inventory_value")}
            amount={kpis.data.inventory_consumption_value_iqd}
            deltaPermille={kpis.data.trend_today_vs_yesterday.inventory_value.delta_permille}
          />
          <KpiCard
            label={t("home.kpi.net")}
            amount={kpis.data.net_iqd}
            deltaPermille={kpis.data.trend_today_vs_yesterday.net.delta_permille}
            tone="ink"
          />
        </section>
      ) : (
        <KpiSkeleton />
      )}

      <TopDoctorsPanel
        rows={tops.data?.top_doctors ?? []}
        isLoading={tops.isLoading}
      />

      <div className="flex flex-wrap items-center gap-3">
        <NavLink to="/accounting/daily-close" className="btn btn-primary">
          {t("home.action.daily_close")}
        </NavLink>
        <NavLink to="/accounting" className="btn btn-ghost">
          {t("home.action.full_dashboard")}
        </NavLink>
      </div>
    </div>
  )
}

function StatusStrip ({
  syncStatus,
  pendingOps,
  dayLabel,
  dayTone,
  dayLive,
}: {
  syncStatus: string
  pendingOps: number
  dayLabel: string
  dayTone: string
  dayLive: boolean
}) {
  const { t } = useTranslation()
  const sync = syncToneLabel(syncStatus, pendingOps, t)
  return (
    <div className="flex flex-wrap items-center gap-3">
      <span className={cn("status-pill", sync.tone, sync.live && "is-live")}>
        {sync.label}
      </span>
      <span className={cn("status-pill", dayTone, dayLive && "is-live")}>{dayLabel}</span>
    </div>
  )
}

function syncToneLabel (
  status: string,
  pending: number,
  t: (k: string, opts?: Record<string, unknown>) => string
): { tone: string; live: boolean; label: string } {
  switch (status) {
    case "pushing":
    case "pulling":
      return {
        tone: "is-info",
        live: true,
        label: t("home.sync.syncing", { count: pending }),
      }
    case "offline":
      return { tone: "is-warn", live: false, label: t("home.sync.offline") }
    case "error":
      return { tone: "is-danger", live: false, label: t("home.sync.error") }
    case "idle":
    default:
      return pending > 0
        ? { tone: "is-warn", live: false, label: t("home.sync.pending", { count: pending }) }
        : { tone: "is-success", live: false, label: t("home.sync.synced") }
  }
}

function CountTile ({
  label,
  value,
  isRtl,
}: {
  label: string
  value: number
  isRtl: boolean
}) {
  const formatted = new Intl.NumberFormat(isRtl ? "ar-IQ" : "en-GB").format(value)
  return (
    <div className="rounded-lg border border-line bg-surface p-5 transition-colors hover:bg-paper">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
        {label}
      </div>
      <div className="mt-2 font-mono text-[30px] font-bold tracking-tight tabular-nums text-ink">
        {formatted}
      </div>
    </div>
  )
}

function ChecksTodayPanel ({
  data,
  isLoading,
  locale,
}: {
  data: ChecksGridCardRecord[] | undefined
  isLoading: boolean
  locale: "en" | "ar"
}) {
  const { t } = useTranslation()
  const numberFormatter = new Intl.NumberFormat(locale === "ar" ? "ar-IQ" : "en-GB")
  return (
    <section className="panel">
      <div className="panel-head flex items-center justify-between">
        <h2 className="text-[14px] font-semibold tracking-[-0.01em] text-ink">
          {t("home.panel.checks_today")}
        </h2>
        <NavLink
          to="/reception"
          className="text-[12px] font-medium text-ink-3 hover:text-ink"
        >
          {t("home.panel.go_to_reception")}
        </NavLink>
      </div>
      <div className="panel-body">
        {isLoading ? (
          <div className="flex flex-wrap gap-2">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="h-7 w-24 animate-pulse rounded-md bg-paper-2" />
            ))}
          </div>
        ) : !data || data.length === 0 ? (
          <p className="text-[12.5px] text-ink-3">{t("home.panel.no_checks")}</p>
        ) : (
          <ul className="flex flex-wrap gap-2">
            {data.map((c) => {
              const name = locale === "en" && c.name_en ? c.name_en : c.name_ar
              return (
                <li key={c.check_type_id}>
                  <NavLink
                    to={`/reception/checks/${c.check_type_id}`}
                    className="inline-flex items-center gap-2 rounded-md border border-line bg-paper-2 px-3 py-1.5 text-[12.5px] text-ink-2 transition-colors hover:bg-surface"
                  >
                    <span>{name}</span>
                    <span className="font-mono text-[11px] tabular-nums text-ink-3">
                      {numberFormatter.format(c.todays_visits)}
                    </span>
                  </NavLink>
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </section>
  )
}

function TopDoctorsPanel ({
  rows,
  isLoading,
}: {
  rows: Array<{
    doctor_id: string | null
    name: string
    revenue_iqd: number
    visits: number
  }>
  isLoading: boolean
}) {
  const { t } = useTranslation()
  return (
    <section className="panel">
      <div className="panel-head flex items-center justify-between">
        <h2 className="text-[14px] font-semibold tracking-[-0.01em] text-ink">
          {t("home.panel.top_doctors_today")}
        </h2>
        <NavLink
          to="/accounting/doctors"
          className="text-[12px] font-medium text-ink-3 hover:text-ink"
        >
          {t("home.panel.see_all")}
        </NavLink>
      </div>
      <div className="panel-body">
        {isLoading ? (
          <div className="space-y-2">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="h-6 animate-pulse rounded bg-paper-2" />
            ))}
          </div>
        ) : rows.length === 0 ? (
          <p className="text-[12.5px] text-ink-3">{t("home.panel.no_top_doctors")}</p>
        ) : (
          <ul className="divide-y divide-line">
            {rows.slice(0, 5).map((d) => (
              <li key={d.doctor_id ?? "house"}>
                <NavLink
                  to={`/accounting/doctors/${d.doctor_id ?? "house"}`}
                  className="flex items-center justify-between py-2 text-[13px] text-ink-2 hover:text-ink"
                >
                  <span className="truncate">{d.name}</span>
                  <span className="ms-3 flex items-center gap-3">
                    <span className="font-mono text-[11px] tabular-nums text-ink-3">
                      {d.visits} {t("home.panel.visits_short")}
                    </span>
                    <span className="font-mono text-[13px] tabular-nums text-ink">
                      {formatIqd(d.revenue_iqd, { withSuffix: true })}
                    </span>
                  </span>
                </NavLink>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}

function KpiSkeleton () {
  return (
    <section className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-5">
      {Array.from({ length: 5 }).map((_, i) => (
        <div key={i} className="h-[110px] animate-pulse rounded-lg bg-paper-2" />
      ))}
    </section>
  )
}
