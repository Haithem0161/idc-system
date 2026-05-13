import { useMemo, useState } from "react"
import { Link, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft } from "lucide-react"

import {
  useInventoryAdjustments,
  useInventoryItem,
} from "@/features/inventory/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import {
  ItemDetailTabs,
  type ItemDetailTab,
} from "@/components/inventory/item-detail-tabs"
import { ItemOverview } from "@/components/inventory/item-overview"
import { ItemConsumptionMapTable } from "@/components/inventory/item-consumption-map-table"
import { ItemAdjustmentsList } from "@/components/inventory/item-adjustments-list"
import { ItemAuditTab } from "@/components/inventory/item-audit-tab"
import { StockStatusPill } from "@/components/inventory/stock-status-pill"

export default function InventoryItemDetailPage () {
  const { t, i18n } = useTranslation()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"
  const { id } = useParams<{ id: string }>()
  const detailQ = useInventoryItem(id)
  const adjustmentsQ = useInventoryAdjustments(id, 200)
  const [tab, setTab] = useState<ItemDetailTab>("overview")

  const detail = detailQ.data
  const fallbackTabContent = useMemo(() => {
    if (detailQ.isLoading) {
      return (
        <div className="panel">
          <div className="panel-body text-[12px] text-ink-3">
            {t("inventory.list.loading")}
          </div>
        </div>
      )
    }
    return null
  }, [detailQ.isLoading, t])

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <header className="space-y-2">
        <Link
          to="/inventory"
          className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink"
        >
          <ArrowLeft className="h-3.5 w-3.5 rtl:rotate-180" strokeWidth={1.8} />
          {t("inventory.item.back")}
        </Link>
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <div className="eyebrow">{t("inventory.eyebrow")}</div>
            <h1 className="text-2xl font-bold tracking-tight text-ink">
              {detail ? resolveLocaleName(detail.item, locale) : t("inventory.list.loading")}
            </h1>
            {detail ? (
              <div className="mt-2 flex flex-wrap items-center gap-2 text-[12px] text-ink-3">
                <StockStatusPill status={detail.item.status} />
                <span className="status-pill">
                  {detail.item.is_active
                    ? t("inventory.item.header.active")
                    : t("inventory.item.header.inactive")}
                </span>
                <span className="font-mono">{detail.item.unit}</span>
              </div>
            ) : null}
          </div>
          <ItemDetailTabs active={tab} onChange={setTab} />
        </div>
      </header>

      {fallbackTabContent ??
        (detail ? (
          tab === "overview" ? (
            <ItemOverview detail={detail} />
          ) : tab === "consumption_map" ? (
            <ItemConsumptionMapTable rows={detail.consumption_map} />
          ) : tab === "adjustments" ? (
            <ItemAdjustmentsList
              rows={adjustmentsQ.data ?? detail.recent_adjustments}
              loading={adjustmentsQ.isLoading}
            />
          ) : (
            <ItemAuditTab itemId={detail.item.id} />
          )
        ) : (
          <div className="panel">
            <div className="panel-body text-[12px] text-ink-3">
              {t("inventory.list.empty")}
            </div>
          </div>
        ))}
    </div>
  )
}
