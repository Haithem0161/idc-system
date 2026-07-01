import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import { SourceVisitsTable } from "@/components/accounting/source-visits-table"
import { StatStrip } from "@/components/accounting/stat-strip"
import { DetailHeader, DetailSection } from "@/components/accounting/detail-chrome"
import { useMandoubDrilldown } from "@/features/reports/queries"
import { formatIqd } from "@/lib/format/money"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

/**
 * Representative (مندوب) detail pane: a stat strip (visits / cut total / avg
 * cut per visit) and the visits that carried this representative. Mirrors the
 * operator pane but has no shifts section -- a mandoub does not clock in.
 * Driven by `reports_mandoub_drilldown`.
 */
export function MandoubDetailPane ({ mandoubId }: { mandoubId: string }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )
  const detail = useMandoubDrilldown(mandoubId, range)

  if (detail.isLoading || !detail.data) {
    return <DetailSkeleton />
  }
  const m = detail.data
  const cutTotal = m.totals.mandoub_cut_iqd
  const avgCut = m.totals.visits > 0 ? cutTotal / m.totals.visits : 0

  return (
    <div className="space-y-4">
      <DetailHeader
        eyebrow={[t("accounting.explorer.entity.mandoubs_singular", { defaultValue: "Representative" })]}
        title={m.name}
      />

      <StatStrip
        items={[
          {
            label: t("accounting.mandoubs.col.visits", { defaultValue: "Visits" }),
            value: String(m.totals.visits),
          },
          {
            label: t("accounting.mandoubs.col.cut_total", { defaultValue: "Cut total" }),
            value: formatIqd(cutTotal, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.mandoubs.col.avg_cut", { defaultValue: "Avg / visit" }),
            value: formatIqd(avgCut, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
        ]}
      />

      <DetailSection
        title={t("accounting.mandoubs.source_visits.title", { defaultValue: "Source visits" })}
        meta={t("accounting.tops.visits_count", {
          defaultValue: "{{count}} visits",
          count: m.totals.visits,
        })}
      >
        <SourceVisitsTable
          rows={m.attributed_visits}
          locale={locale}
          emptyLabel={t("accounting.mandoubs.source_visits.empty", { defaultValue: "No visits in range." })}
        />
      </DetailSection>
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
