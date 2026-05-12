import { Link } from "react-router"
import { useTranslation } from "react-i18next"

import type { InventoryAdjustmentRecord } from "@/lib/ipc"

interface Props {
  rows: InventoryAdjustmentRecord[]
  loading?: boolean
}

function formatTimestamp (iso: string, locale: "ar" | "en") {
  try {
    return new Date(iso).toLocaleString(locale === "ar" ? "ar-IQ" : "en-GB", {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return iso
  }
}

/**
 * Chronological list of adjustments for an item. Voided-visit offset rows
 * (positive delta with `reason='consume_visit'`) render with a reversal
 * badge that links back to the voided visit (phase-06 §7.15).
 */
export function ItemAdjustmentsList ({ rows, loading }: Props) {
  const { t, i18n } = useTranslation()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"

  return (
    <div className="panel overflow-hidden">
      <div className="panel-head">
        <div>
          <span className="panel-title">
            {t("inventory.item.adjustments.title")}
          </span>
          <div className="text-[12px] text-ink-3 mt-1">
            {t("inventory.item.adjustments.subtitle")}
          </div>
        </div>
      </div>
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("inventory.item.adjustments.columns.created")}</th>
            <th>{t("inventory.item.adjustments.columns.reason")}</th>
            <th className="text-end">
              {t("inventory.item.adjustments.columns.delta")}
            </th>
            <th>{t("inventory.item.adjustments.columns.note")}</th>
            <th>{t("inventory.item.adjustments.columns.by")}</th>
          </tr>
        </thead>
        <tbody>
          {loading ? (
            <tr>
              <td colSpan={5} className="text-[12px] text-ink-3">
                {t("inventory.list.loading")}
              </td>
            </tr>
          ) : rows.length === 0 ? (
            <tr>
              <td colSpan={5} className="text-[12px] text-ink-3">
                {t("inventory.item.adjustments.empty")}
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr key={row.id}>
                <td className="font-mono text-[12px] text-ink-3">
                  {formatTimestamp(row.created_at, locale)}
                </td>
                <td>
                  <span className="status-pill">
                    {t(`inventory.item.adjustments.reasons.${row.reason}` as const)}
                  </span>
                  {row.is_reversal ? (
                    <span
                      className="ms-2 status-pill is-info"
                      title={t("inventory.item.adjustments.reversal_tooltip")}
                    >
                      {t("inventory.item.adjustments.reversal_badge")}
                    </span>
                  ) : null}
                </td>
                <td
                  className={
                    "text-end font-mono tabular-nums " +
                    (row.delta > 0 ? "text-success" : "text-crimson")
                  }
                >
                  {row.delta > 0 ? "+" : ""}
                  {row.delta.toLocaleString()}
                </td>
                <td className="text-[12px] text-ink-3 max-w-[260px] truncate">
                  {row.is_reversal && row.visit_id ? (
                    <Link
                      to={`/reception/visits/${row.visit_id}`}
                      className="text-info hover:underline underline-offset-4"
                    >
                      {row.note ?? "—"}
                    </Link>
                  ) : (
                    row.note ?? "—"
                  )}
                </td>
                <td className="font-mono text-[11px] text-ink-3">
                  {row.by_user_id.slice(0, 8)}
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  )
}
