import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import { useInventoryItemCreate, useInventoryItems } from "@/features/catalog/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function InventoryCatalogListPage () {
  const { t, i18n } = useTranslation()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"
  const [query, setQuery] = useState("")
  const [includeInactive, setIncludeInactive] = useState(false)
  const list = useInventoryItems({
    include_inactive: includeInactive,
    query: query.trim().length >= 2 ? query.trim() : undefined,
  })
  const create = useInventoryItemCreate()

  const [creating, setCreating] = useState(false)
  const [nameAr, setNameAr] = useState("")
  const [nameEn, setNameEn] = useState("")
  const [unit, setUnit] = useState("ml")
  const [threshold, setThreshold] = useState(0)
  const [error, setError] = useState<string | null>(null)

  const total = list.data?.length ?? 0

  const reset = () => {
    setNameAr("")
    setNameEn("")
    setUnit("ml")
    setThreshold(0)
    setError(null)
    setCreating(false)
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await create.mutateAsync({
        name_ar: nameAr,
        name_en: nameEn || null,
        unit,
        low_stock_threshold: threshold,
      })
      reset()
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <AdminHeader
        title={t("admin.inventory.title", { defaultValue: "Inventory" })}
        subtitle={t("admin.inventory.subtitle", { defaultValue: "Consumables tracked per check." })}
        count={total}
        actions={
          <>
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("admin.search_placeholder", { defaultValue: "Search..." }) ?? ""}
              className="input h-8 w-44"
            />
            <label className="inline-flex cursor-pointer items-center gap-2 text-[12px] font-medium text-ink-2">
              <input
                type="checkbox"
                checked={includeInactive}
                onChange={(e) => setIncludeInactive(e.target.checked)}
                className="h-3.5 w-3.5 accent-ink"
              />
              <span>{t("admin.include_inactive", { defaultValue: "Include inactive" })}</span>
            </label>
            <button type="button" onClick={() => setCreating((v) => !v)} className="btn btn-primary btn-sm">
              {creating ? <X className="h-3.5 w-3.5" strokeWidth={1.8} /> : <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />}
              {creating ? t("admin.cancel", { defaultValue: "Cancel" }) : t("admin.inventory.new", { defaultValue: "New item" })}
            </button>
          </>
        }
      />

      {creating ? (
        <form onSubmit={submit} className="panel">
          <div className="panel-head">
            <span className="panel-title">{t("admin.inventory.new", { defaultValue: "New item" })}</span>
          </div>
          <div className="panel-body space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-4">
              <FieldLabel label={t("admin.inventory.name_ar", { defaultValue: "Name (AR)" })}>
                <input type="text" value={nameAr} onChange={(e) => setNameAr(e.target.value)} required className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.inventory.name_en", { defaultValue: "Name (EN)" })}>
                <input type="text" value={nameEn} onChange={(e) => setNameEn(e.target.value)} className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.inventory.unit", { defaultValue: "Unit" })}>
                <input type="text" value={unit} onChange={(e) => setUnit(e.target.value)} required className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.inventory.low_stock_threshold", { defaultValue: "Low stock threshold" })}>
                <input type="number" min={0} value={threshold} onChange={(e) => setThreshold(Number(e.target.value))} className="input font-mono" />
              </FieldLabel>
            </div>
            <ErrorBanner message={error} />
            <div className="flex justify-end gap-2">
              <button type="button" onClick={reset} className="btn btn-ghost btn-sm">
                {t("admin.cancel", { defaultValue: "Cancel" })}
              </button>
              <button type="submit" disabled={create.isPending} className="btn btn-primary btn-sm">
                {t("admin.save", { defaultValue: "Save" })}
              </button>
            </div>
          </div>
        </form>
      ) : null}

      <div className="panel overflow-hidden">
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("admin.inventory.name", { defaultValue: "Name" })}</th>
              <th>{t("admin.inventory.unit", { defaultValue: "Unit" })}</th>
              <th className="text-end">{t("admin.inventory.on_hand", { defaultValue: "On hand" })}</th>
              <th className="text-end">{t("admin.inventory.threshold", { defaultValue: "Threshold" })}</th>
              <th>{t("admin.status", { defaultValue: "Status" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((item) => {
              const lowStock = item.quantity_on_hand <= item.low_stock_threshold
              return (
                <tr key={item.id}>
                  <td className="font-medium text-ink">{resolveLocaleName(item, locale)}</td>
                  <td className="text-[12px] text-ink-3">{item.unit}</td>
                  <td className="text-end font-mono">{item.quantity_on_hand.toLocaleString()}</td>
                  <td className="text-end font-mono text-ink-3">{item.low_stock_threshold.toLocaleString()}</td>
                  <td>
                    {!item.is_active ? (
                      <span className="status-pill">{t("admin.inactive", { defaultValue: "Inactive" })}</span>
                    ) : lowStock ? (
                      <span className="status-pill is-warn">{t("admin.inventory.low", { defaultValue: "Low" })}</span>
                    ) : (
                      <span className="status-pill is-success">{t("admin.inventory.ok", { defaultValue: "Ok" })}</span>
                    )}
                  </td>
                  <td className="text-end">
                    <Link
                      to={`/admin/inventory/${item.id}`}
                      className="inline-flex items-center text-[12px] font-medium text-ink-2 underline-offset-4 transition-colors hover:text-crimson hover:underline"
                    >
                      {t("admin.edit", { defaultValue: "Edit" })}
                    </Link>
                  </td>
                </tr>
              )
            })}
            {total === 0 ? (
              <EmptyRow colSpan={6} message={t("admin.inventory.empty", { defaultValue: "No items yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
