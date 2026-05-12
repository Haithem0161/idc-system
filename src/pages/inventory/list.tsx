import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus } from "lucide-react"

import { useInventoryItems } from "@/features/inventory/queries"
import type { StockStatusLiteral } from "@/lib/ipc"
import { InventoryItemsTable } from "@/components/inventory/items-table"

type StatusFilter = StockStatusLiteral | "all"

const STATUS_FILTERS: StatusFilter[] = ["all", "ok", "low", "neg"]

export default function InventoryListPage () {
  const { t } = useTranslation()
  const [status, setStatus] = useState<StatusFilter>("all")
  const [includeInactive, setIncludeInactive] = useState(false)
  const [query, setQuery] = useState("")

  const list = useInventoryItems({
    status: status === "all" ? null : status,
    include_inactive: includeInactive,
    query: query.trim().length >= 2 ? query.trim() : undefined,
  })

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="eyebrow">{t("inventory.eyebrow")}</div>
          <h1 className="text-2xl font-bold tracking-tight text-ink">
            {t("inventory.list.title")}
          </h1>
          <p className="text-[12px] text-ink-3">
            {t("inventory.list.subtitle")}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={
              t("inventory.list.filters.search_placeholder") as string
            }
            className="input h-8 w-44"
          />
          <label className="inline-flex cursor-pointer items-center gap-2 text-[12px] font-medium text-ink-2">
            <input
              type="checkbox"
              checked={includeInactive}
              onChange={(e) => setIncludeInactive(e.target.checked)}
              className="h-3.5 w-3.5 accent-ink"
            />
            <span>
              {includeInactive
                ? t("inventory.list.filters.active.label_all")
                : t("inventory.list.filters.active.label_active_only")}
            </span>
          </label>
          <Link to="/inventory/adjust" className="btn btn-primary btn-sm">
            <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />
            {t("inventory.list.actions.new_adjustment")}
          </Link>
        </div>
      </header>

      <div className="inline-flex items-center gap-1 rounded-md border border-line bg-paper-2 p-1">
        {STATUS_FILTERS.map((s) => {
          const isActive = s === status
          return (
            <button
              type="button"
              key={s}
              onClick={() => setStatus(s)}
              className={
                "rounded-sm px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider transition-colors " +
                (isActive
                  ? "bg-surface text-ink shadow-sm"
                  : "text-ink-3 hover:text-ink-2")
              }
            >
              {t(`inventory.list.filters.status.${s}` as const)}
            </button>
          )
        })}
      </div>

      <InventoryItemsTable
        items={list.data ?? []}
        loading={list.isLoading}
        emptyMessage={
          (status !== "all" || query.length > 0 || includeInactive
            ? t("inventory.list.empty_filtered")
            : t("inventory.list.empty")) as string
        }
      />
    </div>
  )
}
