import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"
import { save } from "@tauri-apps/plugin-dialog"

import { DateRangePicker } from "@/components/accounting/date-range-picker"
import { IncludeVoidedToggle } from "@/components/accounting/include-voided-toggle"
import { useExportVisitsCsv, useVisitsReport } from "@/features/reports/queries"
import { formatIqd } from "@/lib/format/money"
import type {
  ReportsVisitsArgs,
  VisitReportRowRecord,
  VisitsReportGroupByLiteral,
} from "@/lib/ipc"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"
import { cn } from "@/lib/utils"

const GROUP_OPTIONS: VisitsReportGroupByLiteral[] = [
  "none",
  "by_date",
  "by_doctor",
  "by_operator",
  "by_check_type",
  "by_subtype",
  "by_status",
]

export default function AccountingVisitsPage () {
  const { t, i18n } = useTranslation()
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const [groupBy, setGroupBy] = useState<VisitsReportGroupByLiteral>("none")
  const filters: ReportsVisitsArgs = useMemo(
    () => ({
      ...rangeAsUtc(fromDate, toDate),
      include_voided: includeVoided,
      group_by: groupBy,
    }),
    [fromDate, toDate, includeVoided, groupBy]
  )
  const report = useVisitsReport(filters)
  const exportMutation = useExportVisitsCsv()

  const onExport = async () => {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19)
    const slug = `visits_${fromDate}_${toDate}_${stamp}.csv`
    const path = await save({
      defaultPath: slug,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    })
    if (!path) return
    await exportMutation.mutateAsync({ filters, path })
  }

  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="eyebrow">
            {t("accounting.visits.eyebrow", { defaultValue: "Visits report" })}
          </div>
          <h1 className="mt-1 text-[28px] font-bold tracking-tight text-ink">
            {t("accounting.visits.title", { defaultValue: "Visits" })}
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
        <label className="flex items-center gap-2 text-[11px] uppercase tracking-[0.08em] text-ink-3">
          <span>{t("accounting.visits.group_by", { defaultValue: "Group by" })}</span>
          <select
            value={groupBy}
            onChange={(e) => setGroupBy(e.target.value as VisitsReportGroupByLiteral)}
            className="input h-9 px-2 py-1 text-[12px]"
          >
            {GROUP_OPTIONS.map((g) => (
              <option key={g} value={g}>
                {t(`accounting.visits.group.${g}`, { defaultValue: g })}
              </option>
            ))}
          </select>
        </label>
      </div>

      {report.error ? (
        <div className="rounded-md border border-crimson/30 bg-crimson-soft px-4 py-3 text-[12px] text-crimson">
          {t("accounting.errors.visits_failed", {
            defaultValue: "Could not load the visits report.",
          })}
        </div>
      ) : null}

      {report.data ? (
        report.data.mode === "rows" ? (
          <RowsTable rows={report.data.rows} totals={report.data.totals} locale={locale} />
        ) : (
          <GroupsTable
            groups={report.data.groups}
            totals={report.data.totals}
            locale={locale}
          />
        )
      ) : (
        <div className="h-[200px] animate-pulse rounded-lg bg-paper-2" />
      )}
    </div>
  )
}

function RowsTable ({
  rows,
  totals,
  locale,
}: {
  rows: VisitReportRowRecord[]
  totals: { revenue_iqd: number; doctor_cut_iqd: number; operator_cut_iqd: number; net_iqd: number; visits: number }
  locale: string
}) {
  const { t } = useTranslation()
  if (rows.length === 0) {
    return (
      <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
        {t("accounting.visits.empty", { defaultValue: "No visits in range." })}
      </div>
    )
  }
  return (
    <div className="overflow-hidden rounded-lg border border-line">
      <table className="data-table w-full">
        <thead>
          <tr>
            <th className="text-start">{t("accounting.visits.col.date", { defaultValue: "Date" })}</th>
            <th className="text-start">{t("accounting.visits.col.visit_no", { defaultValue: "Visit #" })}</th>
            <th className="text-start">{t("accounting.visits.col.patient", { defaultValue: "Patient" })}</th>
            <th className="text-start">{t("accounting.visits.col.check", { defaultValue: "Check" })}</th>
            <th className="text-start">{t("accounting.visits.col.subtype", { defaultValue: "Subtype" })}</th>
            <th className="text-start">{t("accounting.visits.col.doctor", { defaultValue: "Doctor" })}</th>
            <th className="text-start">{t("accounting.visits.col.operator", { defaultValue: "Operator" })}</th>
            <th className="text-center">{t("accounting.visits.col.dye", { defaultValue: "Dye" })}</th>
            <th className="text-center">{t("accounting.visits.col.report", { defaultValue: "Report" })}</th>
            <th className="text-end">{t("accounting.visits.col.price", { defaultValue: "Price" })}</th>
            <th className="text-end">{t("accounting.visits.col.doctor_cut", { defaultValue: "Doc cut" })}</th>
            <th className="text-end">{t("accounting.visits.col.operator_cut", { defaultValue: "Op cut" })}</th>
            <th className="text-end">{t("accounting.visits.col.net", { defaultValue: "Net" })}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.visit_id}>
              <td className="font-mono">{r.locked_at ? r.locked_at.slice(0, 10) : "—"}</td>
              <td className="font-mono">{r.visit_id.slice(-6)}</td>
              <td>
                <NavLink to={`/accounting/visits/${r.visit_id}`} className="text-ink hover:underline">
                  {r.patient_name}
                </NavLink>
              </td>
              <td>{r.check_type_name_en ?? r.check_type_name_ar}</td>
              <td className="text-ink-3">
                {r.check_subtype_name_en ?? r.check_subtype_name_ar ?? "—"}
              </td>
              <td>{r.doctor_name ?? <span className="text-ink-3">(house)</span>}</td>
              <td>{r.operator_name}</td>
              <td className="text-center text-ink-3">{r.dye ? "Y" : "—"}</td>
              <td className="text-center text-ink-3">{r.report ? "Y" : "—"}</td>
              <td className="text-end font-mono tabular-nums">
                {formatIqd(r.price_iqd, { locale })}
              </td>
              <td className="text-end font-mono tabular-nums">
                {formatIqd(r.doctor_cut_iqd, { locale })}
              </td>
              <td className="text-end font-mono tabular-nums">
                {formatIqd(r.operator_cut_iqd, { locale })}
              </td>
              <td
                className={cn(
                  "text-end font-mono tabular-nums",
                  r.net_iqd < 0 && "text-crimson"
                )}
              >
                {formatIqd(r.net_iqd, { locale })}
              </td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <td colSpan={9} className="text-end text-[11px] font-semibold uppercase tracking-[0.1em] text-ink-3">
              {t("accounting.visits.totals", { defaultValue: "Totals" })} · {totals.visits}{" "}
              {t("accounting.visits.visits_unit", { defaultValue: "visits" })}
            </td>
            <td className="text-end font-mono font-semibold tabular-nums">
              {formatIqd(totals.revenue_iqd, { locale })}
            </td>
            <td className="text-end font-mono font-semibold tabular-nums">
              {formatIqd(totals.doctor_cut_iqd, { locale })}
            </td>
            <td className="text-end font-mono font-semibold tabular-nums">
              {formatIqd(totals.operator_cut_iqd, { locale })}
            </td>
            <td className="text-end font-mono font-semibold tabular-nums">
              {formatIqd(totals.net_iqd, { locale })}
            </td>
          </tr>
        </tfoot>
      </table>
    </div>
  )
}

function GroupsTable ({
  groups,
  totals,
  locale,
}: {
  groups: Array<{
    key: string
    label: string
    visits: number
    revenue_iqd: number
    doctor_cut_iqd: number
    operator_cut_iqd: number
    net_iqd: number
  }>
  totals: { revenue_iqd: number; doctor_cut_iqd: number; operator_cut_iqd: number; net_iqd: number; visits: number }
  locale: string
}) {
  const { t } = useTranslation()
  if (groups.length === 0) {
    return (
      <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
        {t("accounting.visits.empty", { defaultValue: "No visits in range." })}
      </div>
    )
  }
  return (
    <div className="overflow-hidden rounded-lg border border-line">
      <table className="data-table w-full">
        <thead>
          <tr>
            <th className="text-start">
              {t("accounting.visits.col.group", { defaultValue: "Group" })}
            </th>
            <th className="text-end">
              {t("accounting.visits.col.visits", { defaultValue: "Visits" })}
            </th>
            <th className="text-end">
              {t("accounting.visits.col.revenue", { defaultValue: "Revenue" })}
            </th>
            <th className="text-end">
              {t("accounting.visits.col.doctor_cut", { defaultValue: "Doc cut" })}
            </th>
            <th className="text-end">
              {t("accounting.visits.col.operator_cut", { defaultValue: "Op cut" })}
            </th>
            <th className="text-end">
              {t("accounting.visits.col.net", { defaultValue: "Net" })}
            </th>
          </tr>
        </thead>
        <tbody>
          {groups.map((g) => (
            <tr key={g.key}>
              <td>{g.label || g.key}</td>
              <td className="text-end font-mono tabular-nums">{g.visits}</td>
              <td className="text-end font-mono tabular-nums">{formatIqd(g.revenue_iqd, { locale })}</td>
              <td className="text-end font-mono tabular-nums">{formatIqd(g.doctor_cut_iqd, { locale })}</td>
              <td className="text-end font-mono tabular-nums">{formatIqd(g.operator_cut_iqd, { locale })}</td>
              <td className="text-end font-mono tabular-nums">{formatIqd(g.net_iqd, { locale })}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <td className="text-end font-semibold uppercase tracking-[0.1em] text-ink-3">
              {t("accounting.visits.totals", { defaultValue: "Totals" })}
            </td>
            <td className="text-end font-mono font-semibold tabular-nums">{totals.visits}</td>
            <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(totals.revenue_iqd, { locale })}</td>
            <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(totals.doctor_cut_iqd, { locale })}</td>
            <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(totals.operator_cut_iqd, { locale })}</td>
            <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(totals.net_iqd, { locale })}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  )
}
