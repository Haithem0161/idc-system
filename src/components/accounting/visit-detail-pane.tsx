import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"
import { ExternalLink } from "lucide-react"

import { StatStrip } from "@/components/accounting/stat-strip"
import { DetailHeader, DetailSection } from "@/components/accounting/detail-chrome"
import { useVisitsReport } from "@/features/reports/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { formatIqd } from "@/lib/format/money"
import { cn } from "@/lib/utils"
import type { ReportsVisitsArgs, VisitReportRowRecord } from "@/lib/ipc"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

/**
 * Visit detail pane: the accounting-relevant breakdown of a single visit,
 * shown inline in the explorer's detail pane (master/detail) rather than
 * bouncing the user out to the standalone read-only visit page. The full page
 * (print + reception tabs) is still one click away via "Open full page".
 *
 * The visit row already lives in the visits-report cache the master list
 * fetches, so this pane reads the same query (identical key -> shared cache)
 * and selects the row by id. No dedicated per-visit IPC command is needed.
 */
export function VisitDetailPane ({ visitId }: { visitId: string }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const localeShort = i18n.language === "ar" ? "ar" : "en"
  const navigate = useNavigate()
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)

  // Same args the master list uses -> same React Query key -> shared cache.
  const visitsArgs: ReportsVisitsArgs = useMemo(
    () => ({
      ...rangeAsUtc(fromDate, toDate),
      include_voided: includeVoided,
      group_by: "none",
      limit: 500,
    }),
    [fromDate, toDate, includeVoided]
  )
  const visits = useVisitsReport(visitsArgs)

  const row: VisitReportRowRecord | undefined = useMemo(() => {
    if (!visits.data || visits.data.mode !== "rows") return undefined
    return visits.data.rows.find((r) => r.visit_id === visitId)
  }, [visits.data, visitId])

  if (visits.isLoading) {
    return <DetailSkeleton />
  }

  if (!row) {
    return (
      <div className="space-y-4">
        <DetailHeader
          eyebrow={[t("accounting.explorer.entity.visits_singular", { defaultValue: "Visit" })]}
          title={t("accounting.visits.not_in_range_title", { defaultValue: "Visit not in range" })}
        />
        <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
          {t("accounting.visits.not_in_range_body", {
            defaultValue: "This visit is outside the current date range. Adjust the range or open the full page.",
          })}
        </div>
        <button
          type="button"
          onClick={() => navigate(`/accounting/visits/${visitId}`)}
          className="btn btn-ghost btn-sm"
        >
          <ExternalLink className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
          {t("accounting.explorer.open_full", { defaultValue: "Open full page" })}
        </button>
      </div>
    )
  }

  const checkName = resolveLocaleName(
    { name_ar: row.check_type_name_ar, name_en: row.check_type_name_en },
    localeShort
  )
  const subtypeName =
    row.check_subtype_name_ar || row.check_subtype_name_en
      ? resolveLocaleName(
          { name_ar: row.check_subtype_name_ar ?? "", name_en: row.check_subtype_name_en },
          localeShort
        )
      : null
  const doctorLabel =
    row.doctor_name ?? t("accounting.house.label", { defaultValue: "Internal" })

  return (
    <div className="space-y-4">
      <DetailHeader
        eyebrow={[
          t("accounting.explorer.entity.visits_singular", { defaultValue: "Visit" }),
          row.locked_at ? row.locked_at.slice(0, 10) : "—",
        ]}
        title={row.patient_name}
        actions={
          <button
            type="button"
            onClick={() => navigate(`/accounting/visits/${visitId}`)}
            className="btn btn-ghost btn-sm"
          >
            <ExternalLink className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
            {t("accounting.explorer.open_full", { defaultValue: "Open full page" })}
          </button>
        }
      />

      <StatStrip
        items={[
          {
            label: t("accounting.visits.col.price", { defaultValue: "Price" }),
            value: formatIqd(row.price_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.visits.col.doctor_cut", { defaultValue: "Doc cut" }),
            value: formatIqd(row.doctor_cut_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.visits.col.operator_cut", { defaultValue: "Op cut" }),
            value: formatIqd(row.operator_cut_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
          {
            label: t("accounting.visits.col.net", { defaultValue: "Net" }),
            value: formatIqd(row.net_iqd, { locale }),
            unit: t("accounting.currency_suffix", { defaultValue: "IQD" }),
          },
        ]}
      />

      <DetailSection title={t("accounting.visits.breakdown.title", { defaultValue: "Visit" })}>
        <dl className="overflow-hidden rounded-lg border border-line bg-surface">
          <Field
            label={t("accounting.visits.col.status", { defaultValue: "Status" })}
            value={<StatusPill status={row.status} />}
          />
          <Field
            label={t("accounting.visits.col.check", { defaultValue: "Check" })}
            value={
              <span>
                {checkName}
                {subtypeName ? (
                  <span className="text-ink-3"> · {subtypeName}</span>
                ) : null}
              </span>
            }
          />
          <Field
            label={t("accounting.visits.col.doctor", { defaultValue: "Doctor" })}
            value={
              <span className={cn(!row.doctor_name && "text-ink-4")}>{doctorLabel}</span>
            }
          />
          <Field
            label={t("accounting.visits.col.operator", { defaultValue: "Operator" })}
            value={row.operator_name}
          />
          <Field
            label={t("accounting.visits.col.dye", { defaultValue: "Dye" })}
            value={
              row.dye
                ? t("common.yes", { defaultValue: "Yes" })
                : t("common.no", { defaultValue: "No" })
            }
          />
          <Field
            label={t("accounting.visits.col.report", { defaultValue: "Report" })}
            value={
              row.report
                ? t("common.yes", { defaultValue: "Yes" })
                : t("common.no", { defaultValue: "No" })
            }
            last={row.amount_paid_override_iqd == null}
          />
          {row.amount_paid_override_iqd != null ? (
            <Field
              label={t("accounting.visits.col.paid", { defaultValue: "Amount paid" })}
              value={
                <span className="inline-flex items-center gap-2">
                  <span className="font-mono tabular-nums text-crimson">
                    {formatIqd(row.amount_paid_override_iqd, { locale })}
                  </span>
                  <span className="rounded-full bg-crimson-soft px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.04em] text-crimson">
                    {t("accounting.visits.overridden", { defaultValue: "Override" })}
                  </span>
                </span>
              }
              last
            />
          ) : null}
        </dl>
      </DetailSection>
    </div>
  )
}

function Field ({
  label,
  value,
  last,
}: {
  label: string
  value: React.ReactNode
  last?: boolean
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 px-4 py-3",
        !last && "border-b border-line"
      )}
    >
      <dt className="text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
        {label}
      </dt>
      <dd className="min-w-0 truncate text-end text-[13px] text-ink">{value}</dd>
    </div>
  )
}

function StatusPill ({ status }: { status: string }) {
  const { t } = useTranslation()
  const tone =
    status === "locked"
      ? "bg-success-soft text-success before:bg-success"
      : status === "voided"
        ? "bg-crimson-soft text-crimson before:bg-crimson"
        : "bg-paper-2 text-ink-3 before:bg-ink-4"
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-semibold uppercase tracking-[0.04em] before:h-1.5 before:w-1.5 before:rounded-full before:content-['']",
        tone
      )}
    >
      {t(`accounting.status.${status}`, { defaultValue: status })}
    </span>
  )
}

function DetailSkeleton () {
  return (
    <div className="space-y-4">
      <div className="h-8 w-48 animate-pulse rounded bg-paper-2" />
      <div className="h-[88px] animate-pulse rounded-lg bg-paper-2" />
      <div className="h-[220px] animate-pulse rounded-lg bg-paper-2" />
    </div>
  )
}
