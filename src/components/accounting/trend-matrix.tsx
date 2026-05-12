import { useTranslation } from "react-i18next"

import { formatIqd, formatPermille } from "@/lib/format/money"
import type { TrendMatrixRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

interface TrendMatrixProps {
  title: string
  matrix: TrendMatrixRecord
  arabicNumerals?: boolean
}

/** 5-row grid trend matrix (phase-07 §7.1). */
export function TrendMatrix ({ title, matrix, arabicNumerals }: TrendMatrixProps) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const rows: Array<{
    key: keyof TrendMatrixRecord
    labelKey: string
    fallback: string
  }> = [
    { key: "revenue", labelKey: "accounting.kpi.revenue", fallback: "Revenue" },
    {
      key: "doctor_cuts",
      labelKey: "accounting.kpi.doctor_cuts",
      fallback: "Doctor cuts",
    },
    {
      key: "operator_cuts",
      labelKey: "accounting.kpi.operator_cuts",
      fallback: "Operator cuts",
    },
    {
      key: "inventory_value",
      labelKey: "accounting.kpi.inventory_value",
      fallback: "Inventory value",
    },
    { key: "net", labelKey: "accounting.kpi.net", fallback: "Net" },
  ]
  return (
    <div className="rounded-lg border border-line bg-surface p-5">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
        {title}
      </div>
      <table className="mt-3 w-full">
        <thead className="text-start text-[10px] uppercase tracking-[0.1em] text-ink-3">
          <tr>
            <th className="pb-2 text-start">
              {t("accounting.kpi.metric", { defaultValue: "Metric" })}
            </th>
            <th className="pb-2 text-end">
              {t("accounting.kpi.current", { defaultValue: "Current" })}
            </th>
            <th className="pb-2 text-end">
              {t("accounting.kpi.prior", { defaultValue: "Prior" })}
            </th>
            <th className="pb-2 text-end">
              {t("accounting.kpi.change", { defaultValue: "Δ" })}
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {rows.map((r) => {
            const cell = matrix[r.key]
            return (
              <tr key={r.key} className="text-[12px]">
                <td className="py-2 text-ink-2">
                  {t(r.labelKey, { defaultValue: r.fallback })}
                </td>
                <td className="py-2 text-end font-mono tabular-nums text-ink">
                  {formatIqd(cell.current_iqd, { arabicNumerals, locale })}
                </td>
                <td className="py-2 text-end font-mono tabular-nums text-ink-3">
                  {formatIqd(cell.prior_iqd, { arabicNumerals, locale })}
                </td>
                <td
                  className={cn(
                    "py-2 text-end font-mono tabular-nums",
                    cell.delta_permille > 0
                      ? "text-success"
                      : cell.delta_permille < 0
                        ? "text-crimson"
                        : "text-ink-3"
                  )}
                >
                  {formatPermille(cell.delta_permille, { arabicNumerals })}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
