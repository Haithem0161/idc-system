import { useTranslation } from "react-i18next"

import type { InventoryItemDetailRecord } from "@/lib/ipc"

import { StockStatusPill } from "./stock-status-pill"

interface Props {
  detail: InventoryItemDetailRecord
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

export function ItemOverview ({ detail }: Props) {
  const { t, i18n } = useTranslation()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"
  const { item, recent_adjustments } = detail
  const lastAdjusted = recent_adjustments[0]

  return (
    <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
      <div className="panel">
        <div className="panel-head">
          <span className="eyebrow">{t("inventory.item.overview.on_hand_label")}</span>
        </div>
        <div className="panel-body">
          <div className="text-3xl font-bold font-mono tabular-nums text-ink">
            {item.quantity_on_hand.toLocaleString()}
            <span className="ms-2 text-sm font-medium text-ink-3">{item.unit}</span>
          </div>
          <div className="mt-3">
            <StockStatusPill status={item.status} />
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">
          <span className="eyebrow">
            {t("inventory.item.overview.threshold_label")}
          </span>
        </div>
        <div className="panel-body">
          <div className="text-2xl font-semibold font-mono tabular-nums text-ink-2">
            {item.low_stock_threshold.toLocaleString()}
          </div>
          <div className="mt-2 text-[12px] text-ink-3">
            {t("inventory.item.header.unit_label")}: <span className="font-medium">{item.unit}</span>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">
          <span className="eyebrow">
            {t("inventory.item.overview.last_adjusted_label")}
          </span>
        </div>
        <div className="panel-body">
          {lastAdjusted ? (
            <>
              <div className="font-mono text-[13px] text-ink-2">
                {formatTimestamp(lastAdjusted.created_at, locale)}
              </div>
              <div className="mt-2 inline-flex items-center gap-2 text-[12px] text-ink-3">
                <span className="status-pill">
                  {t(`inventory.item.adjustments.reasons.${lastAdjusted.reason}` as const)}
                </span>
                <span className="font-mono">
                  {lastAdjusted.delta > 0 ? "+" : ""}
                  {lastAdjusted.delta.toLocaleString()}
                </span>
              </div>
            </>
          ) : (
            <div className="text-[12px] text-ink-3">
              {t("inventory.item.overview.no_adjustments")}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
