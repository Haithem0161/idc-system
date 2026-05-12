import { Link } from "react-router"
import { useTranslation } from "react-i18next"

import type { InventoryItemWithStatusRecord } from "@/lib/ipc"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { DirtyDot } from "@/components/ui/dirty-dot"

import { StockStatusPill } from "./stock-status-pill"

interface Props {
  items: InventoryItemWithStatusRecord[]
  loading?: boolean
  emptyMessage?: string
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

export function InventoryItemsTable ({ items, loading, emptyMessage }: Props) {
  const { t, i18n } = useTranslation()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"
  const total = items.length

  return (
    <div className="panel overflow-hidden">
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("inventory.list.columns.name")}</th>
            <th>{t("inventory.list.columns.unit")}</th>
            <th className="text-end">{t("inventory.list.columns.on_hand")}</th>
            <th className="text-end">{t("inventory.list.columns.threshold")}</th>
            <th>{t("inventory.list.columns.status")}</th>
            <th>{t("inventory.list.columns.last_adjusted")}</th>
            <th className="text-end">{t("inventory.list.columns.pending_sync")}</th>
          </tr>
        </thead>
        <tbody>
          {loading ? (
            <tr>
              <td colSpan={7} className="text-[12px] text-ink-3">
                {t("inventory.list.loading")}
              </td>
            </tr>
          ) : total === 0 ? (
            <tr>
              <td colSpan={7} className="text-[12px] text-ink-3">
                {emptyMessage ?? t("inventory.list.empty")}
              </td>
            </tr>
          ) : (
            items.map((item) => (
              <tr key={item.id}>
                <td className="font-medium text-ink">
                  <Link
                    to={`/inventory/items/${item.id}`}
                    className="hover:text-crimson hover:underline underline-offset-4"
                  >
                    {resolveLocaleName(item, locale)}
                  </Link>
                  {!item.is_active ? (
                    <span className="ms-2 status-pill">
                      {t("inventory.item.header.inactive")}
                    </span>
                  ) : null}
                </td>
                <td className="text-[12px] text-ink-3">{item.unit}</td>
                <td className="text-end font-mono">
                  {item.quantity_on_hand.toLocaleString()}
                </td>
                <td className="text-end font-mono text-ink-3">
                  {item.low_stock_threshold.toLocaleString()}
                </td>
                <td>
                  <StockStatusPill status={item.status} />
                </td>
                <td className="text-[12px] text-ink-3 font-mono">
                  {formatTimestamp(item.updated_at, locale)}
                </td>
                <td className="text-end">
                  <DirtyDot dirty={item.dirty} />
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  )
}
