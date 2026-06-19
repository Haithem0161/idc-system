import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"

import { SourceVisitsTable } from "@/components/accounting/source-visits-table"
import { StatStrip } from "@/components/accounting/stat-strip"
import { DetailHeader, DetailSection } from "@/components/accounting/detail-chrome"
import { useVisitsReport } from "@/features/reports/queries"
import {
  doctorGroupKeyToSegment,
  isHouseGroupKey,
} from "@/components/accounting/entity-link"
import { formatIqd } from "@/lib/format/money"
import type { ReportsVisitsArgs } from "@/lib/ipc"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

const RECENT_LIMIT = 50

/**
 * Check-type detail pane. There is no dedicated check-type drilldown command,
 * so this composes two `reports_visits` queries scoped to the one check type:
 * a `by_doctor` grouping for the contribution breakdown, and an ungrouped row
 * list (capped) for recent visits. The display name is resolved from the first
 * visit row's snapshot (every row in scope shares the same check type).
 */
export function CheckDetailPane ({ checkTypeId }: { checkTypeId: string }) {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)

  const base = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const byDoctorArgs: ReportsVisitsArgs = useMemo(
    () => ({ ...base, check_type_ids: [checkTypeId], group_by: "by_doctor" }),
    [base, checkTypeId]
  )
  const rowsArgs: ReportsVisitsArgs = useMemo(
    () => ({ ...base, check_type_ids: [checkTypeId], group_by: "none", limit: RECENT_LIMIT }),
    [base, checkTypeId]
  )

  const byDoctor = useVisitsReport(byDoctorArgs)
  const recent = useVisitsReport(rowsArgs)

  if (byDoctor.isLoading || recent.isLoading || !byDoctor.data || !recent.data) {
    return <DetailSkeleton />
  }

  const groups = byDoctor.data.mode === "groups" ? byDoctor.data.groups : []
  const totals = byDoctor.data.totals
  const rows = recent.data.mode === "rows" ? recent.data.rows : []

  // Resolve the check-type display name from any in-scope visit row.
  const sample = rows[0]
  const title = sample
    ? (i18n.language === "ar"
        ? sample.check_type_name_ar
        : sample.check_type_name_en ?? sample.check_type_name_ar)
    : t("accounting.checks.unknown", { defaultValue: "Check type" })

  return (
    <div className="space-y-4">
      <DetailHeader
        eyebrow={[t("accounting.explorer.entity.checks_singular", { defaultValue: "Check type" })]}
        title={title}
      />

      <StatStrip
        items={[
          {
            label: t("accounting.checks.col.visits", { defaultValue: "Visits" }),
            value: String(totals.visits),
          },
          {
            label: t("accounting.checks.col.revenue", { defaultValue: "Revenue" }),
            value: formatIqd(totals.revenue_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.checks.col.doctor_cut", { defaultValue: "Doctor cut" }),
            value: formatIqd(totals.doctor_cut_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.checks.col.operator_cut", { defaultValue: "Operator cut" }),
            value: formatIqd(totals.operator_cut_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
        ]}
      />

      <DetailSection
        title={t("accounting.checks.by_doctor.title", { defaultValue: "By doctor" })}
        meta={t("accounting.checks.doctor_count", {
          defaultValue: "{{count}} doctors",
          count: groups.length,
        })}
      >
        {groups.length === 0 ? (
          <Empty label={t("accounting.checks.empty", { defaultValue: "No checks in range." })} />
        ) : (
          <div className="overflow-hidden rounded-lg border border-line">
            <table className="data-table w-full">
              <thead>
                <tr>
                  <th className="text-start">{t("accounting.checks.by_doctor.columns.doctor", { defaultValue: "Doctor" })}</th>
                  <th className="text-end">{t("accounting.checks.by_doctor.columns.visits", { defaultValue: "Visits" })}</th>
                  <th className="text-end">{t("accounting.checks.by_doctor.columns.revenue", { defaultValue: "Revenue" })}</th>
                  <th className="text-end">{t("accounting.checks.by_doctor.columns.doctor_cut", { defaultValue: "Doctor cut" })}</th>
                </tr>
              </thead>
              <tbody>
                {groups.map((g) => {
                  const house = isHouseGroupKey(g.key)
                  return (
                  <tr
                    key={g.key}
                    onClick={() =>
                      navigate(`/accounting/explore/doctors/${doctorGroupKeyToSegment(g.key)}`)
                    }
                    className="cursor-pointer"
                  >
                    <td className={house ? "font-medium text-ink-4" : "font-medium text-ink"}>
                      {house
                        ? t("accounting.house.label", { defaultValue: "Internal" })
                        : g.label || g.key}
                    </td>
                    <td className="text-end font-mono tabular-nums">{g.visits}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(g.revenue_iqd, { locale })}</td>
                    <td className="text-end font-mono tabular-nums">{formatIqd(g.doctor_cut_iqd, { locale })}</td>
                  </tr>
                  )
                })}
              </tbody>
              <tfoot>
                <tr>
                  <td className="text-end text-[11px] font-semibold uppercase tracking-[0.1em] text-ink-3">
                    {t("accounting.visits.totals", { defaultValue: "Totals" })}
                  </td>
                  <td className="text-end font-mono font-semibold tabular-nums">{totals.visits}</td>
                  <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(totals.revenue_iqd, { locale })}</td>
                  <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(totals.doctor_cut_iqd, { locale })}</td>
                </tr>
              </tfoot>
            </table>
          </div>
        )}
      </DetailSection>

      <DetailSection
        title={t("accounting.checks.recent.title", { defaultValue: "Recent visits" })}
        meta={
          rows.length >= RECENT_LIMIT
            ? t("accounting.checks.recent.capped", {
                defaultValue: "First {{count}}",
                count: RECENT_LIMIT,
              })
            : t("accounting.tops.visits_count", {
                defaultValue: "{{count}} visits",
                count: rows.length,
              })
        }
      >
        <SourceVisitsTable
          rows={rows}
          locale={locale}
          emptyLabel={t("accounting.checks.empty", { defaultValue: "No checks in range." })}
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
