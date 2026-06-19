import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import { SourceVisitsTable } from "@/components/accounting/source-visits-table"
import { StatStrip } from "@/components/accounting/stat-strip"
import { DetailHeader, DetailSection } from "@/components/accounting/detail-chrome"
import { useDoctorDrilldown } from "@/features/reports/queries"
import { segmentToDoctorId } from "@/components/accounting/entity-link"
import { formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

/**
 * Doctor detail pane: financial stat strip, per-check breakdown, and the
 * source visits that produced the cut. Driven by `reports_doctor_drilldown`.
 * The `segment` is the URL id (`house` for the no-referral clinic row).
 */
export function DoctorDetailPane ({ segment }: { segment: string }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const doctorId = segmentToDoctorId(segment)
  const detail = useDoctorDrilldown(doctorId, range)

  if (detail.isLoading || !detail.data) {
    return <DetailSkeleton />
  }
  const d = detail.data
  const isHouse = d.doctor_id === null
  const title = isHouse ? t("accounting.house.label", { defaultValue: "Internal" }) : d.name

  return (
    <div className="space-y-4">
      <DetailHeader
        eyebrow={[
          t("accounting.explorer.entity.doctors_singular", { defaultValue: "Doctor" }),
          isHouse
            ? t("accounting.house.eyebrow", { defaultValue: "House / internal" })
            : d.specialty ?? t("accounting.doctors.no_specialty", { defaultValue: "No specialty" }),
        ]}
        title={title}
        muted={isHouse}
      />

      <StatStrip
        items={[
          {
            label: t("accounting.doctors.col.visits", { defaultValue: "Visits" }),
            value: String(d.totals.visits),
          },
          {
            label: t("accounting.doctors.col.revenue", { defaultValue: "Revenue" }),
            value: formatIqd(d.totals.revenue_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.doctors.col.cut_total", { defaultValue: "Cut total" }),
            value: formatIqd(d.totals.doctor_cut_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.doctors.col.net", { defaultValue: "Net" }),
            value: formatIqd(d.totals.net_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
        ]}
      />

      <DetailSection
        title={t("accounting.doctors.breakdown.title", { defaultValue: "Per-check breakdown" })}
        meta={t("accounting.checks.count", {
          defaultValue: "{{count}} check types",
          count: d.per_check.length,
        })}
      >
        {d.per_check.length === 0 ? (
          <Empty label={t("accounting.doctors.breakdown.empty", { defaultValue: "No checks in range." })} />
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
              <tfoot>
                <tr>
                  <td colSpan={2} className="text-end text-[11px] font-semibold uppercase tracking-[0.1em] text-ink-3">
                    {t("accounting.visits.totals", { defaultValue: "Totals" })}
                  </td>
                  <td className="text-end font-mono font-semibold tabular-nums">{d.totals.visits}</td>
                  <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(d.totals.revenue_iqd, { locale })}</td>
                  <td className="text-end font-mono font-semibold tabular-nums">{formatIqd(d.totals.doctor_cut_iqd, { locale })}</td>
                  <td />
                </tr>
              </tfoot>
            </table>
          </div>
        )}
      </DetailSection>

      <DetailSection
        title={t("accounting.doctors.source_visits.title", { defaultValue: "Source visits" })}
        meta={t("accounting.tops.visits_count", {
          defaultValue: "{{count}} visits",
          count: d.totals.visits,
        })}
      >
        <SourceVisitsTable
          rows={d.source_visits}
          locale={locale}
          emptyLabel={t("accounting.doctors.source_visits.empty", { defaultValue: "No visits in range." })}
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
