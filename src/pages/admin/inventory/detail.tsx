import { useMemo, useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft, Plus } from "lucide-react"

import {
  useCheckTypes,
  useConsumptionCreate,
  useConsumptionSoftDelete,
  useInventoryItem,
  useInventoryItemSoftDelete,
  useInventoryItemUpdate,
} from "@/features/catalog/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function InventoryItemDetailPage () {
  const { id = "" } = useParams<{ id: string }>()
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"

  const detail = useInventoryItem(id)
  const checkTypes = useCheckTypes()
  const update = useInventoryItemUpdate()
  const softDelete = useInventoryItemSoftDelete()
  const createConsumption = useConsumptionCreate()
  const deleteConsumption = useConsumptionSoftDelete()

  const [error, setError] = useState<string | null>(null)
  const [consumptionForm, setConsumptionForm] = useState({
    check_type_id: "",
    quantity_per_check: 1,
    on_dye_only: false,
  })

  const checkTypeById = useMemo(
    () => new Map((checkTypes.data ?? []).map((ct) => [ct.id, ct])),
    [checkTypes.data],
  )

  if (!detail.data) {
    return (
      <div className="mx-auto max-w-3xl py-12 text-center text-[13px] text-ink-3">
        {detail.isLoading ? t("admin.loading", { defaultValue: "Loading..." }) : t("admin.not_found", { defaultValue: "Not found" })}
      </div>
    )
  }

  const { item, consumption } = detail.data

  const onSave = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const form = new FormData(e.currentTarget)
    try {
      await update.mutateAsync({
        id: item.id,
        name_ar: String(form.get("name_ar") ?? ""),
        name_en: (form.get("name_en") as string) || null,
        unit: String(form.get("unit") ?? ""),
        low_stock_threshold: Number(form.get("low_stock_threshold") ?? 0),
        is_active: form.get("is_active") === "on",
      })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const onAddConsumption = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    if (!consumptionForm.check_type_id) return
    const parent = checkTypeById.get(consumptionForm.check_type_id)
    if (!parent) return
    if (parent.has_subtypes) {
      setError(t("admin.inventory.consumption_subtype_picker", { defaultValue: "Subtype mapping is not supported in this form. Pick a flat check type." }) ?? "")
      return
    }
    if (consumptionForm.on_dye_only && parent.dye_price_iqd == null) {
      setError(t("admin.inventory.consumption_dye_unsupported", { defaultValue: "Selected check type does not support dye." }) ?? "")
      return
    }
    try {
      await createConsumption.mutateAsync({
        check_type_id: consumptionForm.check_type_id,
        check_subtype_id: null,
        item_id: item.id,
        quantity_per_check: consumptionForm.quantity_per_check,
        on_dye_only: consumptionForm.on_dye_only,
      })
      setConsumptionForm({ check_type_id: "", quantity_per_check: 1, on_dye_only: false })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <Link to="/admin/inventory" className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink">
        <ArrowLeft className="h-3 w-3 rtl:rotate-180" strokeWidth={1.8} />
        <span>{t("admin.inventory.back", { defaultValue: "Back to inventory" })}</span>
      </Link>

      <AdminHeader
        title={resolveLocaleName(item, locale)}
        subtitle={`${item.unit} · ${item.quantity_on_hand.toLocaleString()} ${t("admin.inventory.on_hand_short", { defaultValue: "on hand" })}`}
        actions={
          <button
            type="button"
            className="btn btn-danger btn-sm"
            onClick={() => {
              if (confirm(t("admin.inventory.confirm_delete", { defaultValue: "Delete this item?" }) ?? "")) {
                softDelete.mutate(item.id, {
                  onSuccess: () => navigate("/admin/inventory"),
                  onError: (e) => setError((e as { message?: string }).message ?? "Failed"),
                })
              }
            }}
          >
            {t("admin.delete", { defaultValue: "Delete" })}
          </button>
        }
      />

      <form onSubmit={onSave} className="panel">
        <div className="panel-head"><span className="panel-title">{t("admin.inventory.details", { defaultValue: "Details" })}</span></div>
        <div className="panel-body space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FieldLabel label={t("admin.inventory.name_ar", { defaultValue: "Name (AR)" })}>
              <input type="text" name="name_ar" defaultValue={item.name_ar} required className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.inventory.name_en", { defaultValue: "Name (EN)" })}>
              <input type="text" name="name_en" defaultValue={item.name_en ?? ""} className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.inventory.unit", { defaultValue: "Unit" })}>
              <input type="text" name="unit" defaultValue={item.unit} required className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.inventory.low_stock_threshold", { defaultValue: "Low stock threshold" })}>
              <input type="number" name="low_stock_threshold" min={0} defaultValue={item.low_stock_threshold} className="input font-mono" />
            </FieldLabel>
            <label className="inline-flex items-center gap-2 text-[12px] font-medium text-ink-2">
              <input type="checkbox" name="is_active" defaultChecked={item.is_active} className="h-4 w-4 accent-ink" />
              <span>{t("admin.is_active", { defaultValue: "Active" })}</span>
            </label>
          </div>
          <ErrorBanner message={error} />
          <div className="flex justify-end">
            <button type="submit" disabled={update.isPending} className="btn btn-primary btn-sm">
              {t("admin.save", { defaultValue: "Save" })}
            </button>
          </div>
        </div>
      </form>

      <div className="panel overflow-hidden">
        <div className="panel-head"><span className="panel-title">{t("admin.inventory.consumption", { defaultValue: "Consumption rules" })}</span></div>
        <form onSubmit={onAddConsumption} className="panel-body space-y-3 border-b border-line">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-4">
            <FieldLabel label={t("admin.inventory.consumption_check_type", { defaultValue: "Check type" })}>
              <select
                value={consumptionForm.check_type_id}
                onChange={(e) => setConsumptionForm((f) => ({ ...f, check_type_id: e.target.value }))}
                required
                className="input"
              >
                <option value="">—</option>
                {checkTypes.data?.filter((ct) => !ct.has_subtypes).map((ct) => (
                  <option key={ct.id} value={ct.id}>{resolveLocaleName(ct, locale)}</option>
                ))}
              </select>
            </FieldLabel>
            <FieldLabel label={t("admin.inventory.consumption_qty", { defaultValue: "Quantity / check" })}>
              <input
                type="number"
                min={1}
                value={consumptionForm.quantity_per_check}
                onChange={(e) => setConsumptionForm((f) => ({ ...f, quantity_per_check: Number(e.target.value) }))}
                required
                className="input font-mono"
              />
            </FieldLabel>
            <label className="inline-flex items-end gap-2 text-[12px] font-medium text-ink-2">
              <input
                type="checkbox"
                checked={consumptionForm.on_dye_only}
                onChange={(e) => setConsumptionForm((f) => ({ ...f, on_dye_only: e.target.checked }))}
                className="h-4 w-4 accent-ink"
              />
              <span>{t("admin.inventory.consumption_on_dye_only", { defaultValue: "Only when dye is used" })}</span>
            </label>
            <div className="flex items-end">
              <button type="submit" disabled={createConsumption.isPending} className="btn btn-primary btn-sm w-full">
                <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />
                {t("admin.inventory.consumption_add", { defaultValue: "Add" })}
              </button>
            </div>
          </div>
        </form>
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("admin.inventory.consumption_check_type", { defaultValue: "Check type" })}</th>
              <th className="text-end">{t("admin.inventory.consumption_qty_short", { defaultValue: "Qty" })}</th>
              <th>{t("admin.inventory.consumption_on_dye_only_short", { defaultValue: "Dye-only" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {consumption.map((row) => {
              const parent = checkTypeById.get(row.check_type_id)
              return (
                <tr key={row.id}>
                  <td className="font-medium text-ink">
                    {parent ? resolveLocaleName(parent, locale) : row.check_type_id.slice(0, 8)}
                  </td>
                  <td className="text-end font-mono">{row.quantity_per_check}</td>
                  <td>
                    <span className={`status-pill ${row.on_dye_only ? "is-info" : ""}`}>
                      {row.on_dye_only ? t("admin.inventory.yes", { defaultValue: "Yes" }) : t("admin.inventory.no", { defaultValue: "No" })}
                    </span>
                  </td>
                  <td className="text-end">
                    <button
                      type="button"
                      onClick={() =>
                        deleteConsumption.mutate({
                          id: row.id,
                          item_id: item.id,
                          check_type_id: row.check_type_id,
                        })
                      }
                      className="text-[12px] font-medium text-crimson hover:underline"
                    >
                      {t("admin.delete", { defaultValue: "Delete" })}
                    </button>
                  </td>
                </tr>
              )
            })}
            {consumption.length === 0 ? (
              <EmptyRow colSpan={4} message={t("admin.inventory.no_consumption", { defaultValue: "No consumption rules yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
