import { useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft, Plus } from "lucide-react"

import {
  useCheckSubtypeCreate,
  useCheckSubtypeSoftDelete,
  useCheckSubtypes,
  useCheckType,
  useCheckTypeSoftDelete,
  useCheckTypeToggleSubtypes,
  useCheckTypeUpdate,
} from "@/features/catalog/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function CheckTypeDetailPage () {
  const { id = "" } = useParams<{ id: string }>()
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"

  const detail = useCheckType(id)
  const subtypes = useCheckSubtypes(detail.data?.id ?? null)
  const update = useCheckTypeUpdate()
  const toggle = useCheckTypeToggleSubtypes()
  const softDelete = useCheckTypeSoftDelete()
  const createSubtype = useCheckSubtypeCreate()
  const softDeleteSubtype = useCheckSubtypeSoftDelete()

  const [error, setError] = useState<string | null>(null)
  const [addingSubtype, setAddingSubtype] = useState(false)
  const [newSubName, setNewSubName] = useState("")
  const [newSubNameEn, setNewSubNameEn] = useState("")
  const [newSubPrice, setNewSubPrice] = useState<number>(0)

  if (!detail.data) {
    return (
      <div className="mx-auto max-w-3xl py-12 text-center text-[13px] text-ink-3">
        {detail.isLoading ? t("admin.loading", { defaultValue: "Loading..." }) : t("admin.not_found", { defaultValue: "Not found" })}
      </div>
    )
  }

  const ct = detail.data

  const onSave = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const form = new FormData(e.currentTarget)
    try {
      await update.mutateAsync({
        id: ct.id,
        name_ar: String(form.get("name_ar") ?? ""),
        name_en: (form.get("name_en") as string) || null,
        base_price_iqd: ct.has_subtypes ? null : Number(form.get("base_price_iqd") ?? 0),
        dye_supported: form.get("dye_supported") === "on",
        report_supported: form.get("report_supported") === "on",
        sort_order: Number(form.get("sort_order") ?? 0),
        is_active: form.get("is_active") === "on",
      })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const onToggleSubtypes = async () => {
    setError(null)
    try {
      if (ct.has_subtypes) {
        const price = Number(prompt(t("admin.check_types.toggle_off_prompt", { defaultValue: "Enter base price (IQD)" }) ?? "0"))
        if (Number.isNaN(price)) return
        await toggle.mutateAsync({ id: ct.id, to_value: false, base_price_iqd: price })
      } else {
        if (!confirm(t("admin.check_types.toggle_on_confirm", { defaultValue: "Switch to subtype mode? The flat price will be cleared." }) ?? "")) return
        await toggle.mutateAsync({ id: ct.id, to_value: true })
      }
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const onCreateSubtype = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await createSubtype.mutateAsync({
        check_type_id: ct.id,
        name_ar: newSubName,
        name_en: newSubNameEn || null,
        price_iqd: newSubPrice,
      })
      setNewSubName("")
      setNewSubNameEn("")
      setNewSubPrice(0)
      setAddingSubtype(false)
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <Link to="/admin/check-types" className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink">
        <ArrowLeft className="h-3 w-3" strokeWidth={1.8} />
        <span>{t("admin.check_types.back", { defaultValue: "Back to check types" })}</span>
      </Link>

      <AdminHeader
        title={resolveLocaleName(ct, locale)}
        subtitle={ct.has_subtypes ? t("admin.check_types.subtyped", { defaultValue: "Subtyped" }) : t("admin.check_types.flat", { defaultValue: "Flat" })}
        actions={
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => softDelete.mutate(ct.id, { onSuccess: () => navigate("/admin/check-types"), onError: (e) => setError((e as { message?: string }).message ?? "Failed") })}
          >
            {t("admin.delete", { defaultValue: "Delete" })}
          </button>
        }
      />

      <form onSubmit={onSave} className="panel">
        <div className="panel-head">
          <span className="panel-title">{t("admin.check_types.details", { defaultValue: "Details" })}</span>
        </div>
        <div className="panel-body space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FieldLabel label={t("admin.check_types.name_ar", { defaultValue: "Name (AR)" })}>
              <input type="text" name="name_ar" defaultValue={ct.name_ar} required className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.check_types.name_en", { defaultValue: "Name (EN)" })}>
              <input type="text" name="name_en" defaultValue={ct.name_en ?? ""} className="input" />
            </FieldLabel>
            {!ct.has_subtypes ? (
              <FieldLabel label={t("admin.check_types.base_price", { defaultValue: "Base price (IQD)" })}>
                <input type="number" name="base_price_iqd" defaultValue={ct.base_price_iqd ?? 0} min={0} className="input font-mono" />
              </FieldLabel>
            ) : null}
            <FieldLabel label={t("admin.check_types.sort_order", { defaultValue: "Sort order" })}>
              <input type="number" name="sort_order" defaultValue={ct.sort_order} className="input font-mono" />
            </FieldLabel>
            <label className="inline-flex items-center gap-2 text-[12px] font-medium text-ink-2">
              <input type="checkbox" name="dye_supported" defaultChecked={ct.dye_supported} className="h-4 w-4 accent-ink" />
              <span>{t("admin.check_types.dye_supported", { defaultValue: "Supports dye" })}</span>
            </label>
            <label className="inline-flex items-center gap-2 text-[12px] font-medium text-ink-2">
              <input type="checkbox" name="report_supported" defaultChecked={ct.report_supported} className="h-4 w-4 accent-ink" />
              <span>{t("admin.check_types.report_supported", { defaultValue: "Generates report" })}</span>
            </label>
            <label className="inline-flex items-center gap-2 text-[12px] font-medium text-ink-2">
              <input type="checkbox" name="is_active" defaultChecked={ct.is_active} className="h-4 w-4 accent-ink" />
              <span>{t("admin.is_active", { defaultValue: "Active" })}</span>
            </label>
          </div>
          <ErrorBanner message={error} />
          <div className="flex items-center justify-between">
            <button type="button" onClick={onToggleSubtypes} className="btn btn-ghost btn-sm">
              {ct.has_subtypes
                ? t("admin.check_types.switch_to_flat", { defaultValue: "Switch to flat" })
                : t("admin.check_types.switch_to_subtyped", { defaultValue: "Switch to subtyped" })}
            </button>
            <button type="submit" disabled={update.isPending} className="btn btn-primary btn-sm">
              {t("admin.save", { defaultValue: "Save" })}
            </button>
          </div>
        </div>
      </form>

      {ct.has_subtypes ? (
        <div className="panel overflow-hidden">
          <div className="panel-head flex items-center justify-between">
            <span className="panel-title">{t("admin.check_types.subtypes", { defaultValue: "Subtypes" })}</span>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setAddingSubtype((v) => !v)}>
              <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />
              {t("admin.check_types.add_subtype", { defaultValue: "Add subtype" })}
            </button>
          </div>
          {addingSubtype ? (
            <form onSubmit={onCreateSubtype} className="panel-body space-y-4">
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                <FieldLabel label={t("admin.check_types.name_ar", { defaultValue: "Name (AR)" })}>
                  <input type="text" value={newSubName} onChange={(e) => setNewSubName(e.target.value)} required className="input" />
                </FieldLabel>
                <FieldLabel label={t("admin.check_types.name_en", { defaultValue: "Name (EN)" })}>
                  <input type="text" value={newSubNameEn} onChange={(e) => setNewSubNameEn(e.target.value)} className="input" />
                </FieldLabel>
                <FieldLabel label={t("admin.check_types.subtype_price", { defaultValue: "Price (IQD)" })}>
                  <input type="number" value={newSubPrice} onChange={(e) => setNewSubPrice(Number(e.target.value))} min={0} required className="input font-mono" />
                </FieldLabel>
              </div>
              <div className="flex justify-end gap-2">
                <button type="button" className="btn btn-ghost btn-sm" onClick={() => setAddingSubtype(false)}>
                  {t("admin.cancel", { defaultValue: "Cancel" })}
                </button>
                <button type="submit" disabled={createSubtype.isPending} className="btn btn-primary btn-sm">
                  {t("admin.save", { defaultValue: "Save" })}
                </button>
              </div>
            </form>
          ) : null}
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("admin.check_types.name", { defaultValue: "Name" })}</th>
                <th className="text-end">{t("admin.check_types.subtype_price", { defaultValue: "Price" })}</th>
                <th>{t("admin.check_types.sort_order_short", { defaultValue: "Order" })}</th>
                <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
              </tr>
            </thead>
            <tbody>
              {subtypes.data?.map((s) => (
                <tr key={s.id}>
                  <td className="font-medium text-ink">{resolveLocaleName(s, locale)}</td>
                  <td className="text-end font-mono">{s.price_iqd.toLocaleString()}</td>
                  <td className="text-[12px] text-ink-3">{s.sort_order}</td>
                  <td className="text-end">
                    <button
                      type="button"
                      onClick={() => softDeleteSubtype.mutate({ id: s.id, check_type_id: ct.id })}
                      className="text-[12px] font-medium text-crimson hover:underline"
                    >
                      {t("admin.delete", { defaultValue: "Delete" })}
                    </button>
                  </td>
                </tr>
              ))}
              {subtypes.data && subtypes.data.length === 0 ? (
                <EmptyRow colSpan={4} message={t("admin.check_types.no_subtypes", { defaultValue: "No subtypes yet" })} />
              ) : null}
            </tbody>
          </table>
        </div>
      ) : null}
    </div>
  )
}
