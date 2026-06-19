import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"

import { formatIqd } from "@/lib/format/money"
import type { VisitReportRowRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

/**
 * Compact visit-rows table reused by every detail pane (doctor / operator /
 * check type). Rows are clickable and open the full read-only visit detail at
 * `/accounting/visits/:id`. The doctor column renders the house placeholder
 * when `doctor_name` is null.
 */
export function SourceVisitsTable ({
  rows,
  locale,
  emptyLabel,
}: {
  rows: VisitReportRowRecord[]
  locale: string
  emptyLabel: string
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()

  if (rows.length === 0) {
    return (
      <div className="rounded-md border border-line bg-surface p-6 text-center text-[12px] text-ink-3">
        {emptyLabel}
      </div>
    )
  }

  return (
    <div className="overflow-hidden rounded-lg border border-line">
      <table className="data-table w-full">
        <thead>
          <tr>
            <th className="text-start">{t("accounting.visits.col.date", { defaultValue: "Date" })}</th>
            <th className="text-start">{t("accounting.visits.col.patient", { defaultValue: "Patient" })}</th>
            <th className="text-start">{t("accounting.visits.col.check", { defaultValue: "Check" })}</th>
            <th className="text-start">{t("accounting.visits.col.doctor", { defaultValue: "Doctor" })}</th>
            <th className="text-start">{t("accounting.visits.col.operator", { defaultValue: "Operator" })}</th>
            <th className="text-center">{t("accounting.visits.col.dye", { defaultValue: "Dye" })}</th>
            <th className="text-end">{t("accounting.visits.col.price", { defaultValue: "Price" })}</th>
            <th className="text-end">{t("accounting.visits.col.doctor_cut", { defaultValue: "Doc cut" })}</th>
            <th className="text-end">{t("accounting.visits.col.net", { defaultValue: "Net" })}</th>
            <th className="text-start">{t("accounting.visits.col.status", { defaultValue: "Status" })}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr
              key={r.visit_id}
              onClick={() => navigate(`/accounting/visits/${r.visit_id}`)}
              className="cursor-pointer"
            >
              <td className="font-mono">{r.locked_at ? r.locked_at.slice(0, 10) : "—"}</td>
              <td className="font-medium text-ink">{r.patient_name}</td>
              <td>{r.check_type_name_en ?? r.check_type_name_ar}</td>
              <td className={cn(!r.doctor_name && "text-ink-4")}>
                {r.doctor_name ?? t("accounting.house.label", { defaultValue: "Internal" })}
              </td>
              <td>{r.operator_name}</td>
              <td className="text-center text-ink-3">{r.dye ? "Y" : "—"}</td>
              <td className="text-end font-mono tabular-nums">{formatIqd(r.price_iqd, { locale })}</td>
              <td className="text-end font-mono tabular-nums">{formatIqd(r.doctor_cut_iqd, { locale })}</td>
              <td
                className={cn(
                  "text-end font-mono tabular-nums",
                  r.net_iqd < 0 && "text-crimson"
                )}
              >
                {formatIqd(r.net_iqd, { locale })}
              </td>
              <td>
                <StatusPill status={r.status} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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
