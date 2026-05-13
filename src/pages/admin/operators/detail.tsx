import { useMemo, useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft } from "lucide-react"

import {
  useCheckTypes,
  useOperator,
  useOperatorSetActive,
  useOperatorSoftDelete,
  useOperatorSpecialtySoftDelete,
  useOperatorSpecialtyUpsert,
  useOperatorUpdate,
} from "@/features/catalog/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { AdminHeader, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function OperatorDetailPage () {
  const { id = "" } = useParams<{ id: string }>()
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"

  const detail = useOperator(id)
  const checkTypes = useCheckTypes()
  const update = useOperatorUpdate()
  const setActive = useOperatorSetActive()
  const softDelete = useOperatorSoftDelete()
  const upsertSpecialty = useOperatorSpecialtyUpsert()
  const deleteSpecialty = useOperatorSpecialtySoftDelete()

  const [error, setError] = useState<string | null>(null)

  const operatorSpecialtyTypeIds = useMemo(
    () => new Set((detail.data?.specialties ?? []).map((s) => s.check_type_id)),
    [detail.data?.specialties],
  )

  if (!detail.data) {
    return (
      <div className="mx-auto max-w-3xl py-12 text-center text-[13px] text-ink-3">
        {detail.isLoading ? t("admin.loading", { defaultValue: "Loading..." }) : t("admin.not_found", { defaultValue: "Not found" })}
      </div>
    )
  }

  const { operator, specialties } = detail.data

  const onSave = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const form = new FormData(e.currentTarget)
    try {
      await update.mutateAsync({
        id: operator.id,
        name: String(form.get("name") ?? ""),
        phone: (form.get("phone") as string) || null,
        base_cut_per_check_iqd: Number(form.get("base_cut_per_check_iqd") ?? 0),
        notes: (form.get("notes") as string) || null,
      })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const onToggleSpecialty = (checkTypeId: string) => {
    setError(null)
    const existing = specialties.find((s) => s.check_type_id === checkTypeId)
    if (existing) {
      deleteSpecialty.mutate({ id: existing.id, operator_id: operator.id })
    } else {
      upsertSpecialty.mutate(
        { operator_id: operator.id, check_type_id: checkTypeId },
        { onError: (e) => setError((e as { message?: string }).message ?? "Failed") },
      )
    }
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <Link to="/admin/operators" className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink">
        <ArrowLeft className="h-3 w-3 rtl:rotate-180" strokeWidth={1.8} />
        <span>{t("admin.operators.back", { defaultValue: "Back to operators" })}</span>
      </Link>

      <AdminHeader
        title={operator.name}
        subtitle={t("admin.operators.cut_summary", {
          defaultValue: `Earns ${operator.base_cut_per_check_iqd.toLocaleString()} IQD/check`,
          cut: operator.base_cut_per_check_iqd.toLocaleString(),
        })}
        actions={
          <>
            <button
              type="button"
              onClick={() => setActive.mutate({ id: operator.id, is_active: !operator.is_active })}
              className="btn btn-ghost btn-sm"
            >
              {operator.is_active
                ? t("admin.deactivate", { defaultValue: "Deactivate" })
                : t("admin.activate", { defaultValue: "Activate" })}
            </button>
            <button
              type="button"
              className="btn btn-danger btn-sm"
              onClick={() => {
                if (confirm(t("admin.operators.confirm_delete", { defaultValue: "Delete this operator?" }) ?? "")) {
                  softDelete.mutate(operator.id, {
                    onSuccess: () => navigate("/admin/operators"),
                    onError: (e) => setError((e as { message?: string }).message ?? "Failed"),
                  })
                }
              }}
            >
              {t("admin.delete", { defaultValue: "Delete" })}
            </button>
          </>
        }
      />

      <form onSubmit={onSave} className="panel">
        <div className="panel-head"><span className="panel-title">{t("admin.operators.details", { defaultValue: "Details" })}</span></div>
        <div className="panel-body space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FieldLabel label={t("admin.operators.name", { defaultValue: "Name" })}>
              <input type="text" name="name" defaultValue={operator.name} required className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.operators.phone", { defaultValue: "Phone" })}>
              <input type="tel" name="phone" defaultValue={operator.phone ?? ""} className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.operators.base_cut", { defaultValue: "Base cut (IQD/check)" })}>
              <input type="number" name="base_cut_per_check_iqd" defaultValue={operator.base_cut_per_check_iqd} min={0} required className="input font-mono" />
            </FieldLabel>
            <FieldLabel label={t("admin.operators.notes", { defaultValue: "Notes" })}>
              <input type="text" name="notes" defaultValue={operator.notes ?? ""} className="input" />
            </FieldLabel>
          </div>
          <ErrorBanner message={error} />
          <div className="flex justify-end">
            <button type="submit" disabled={update.isPending} className="btn btn-primary btn-sm">
              {t("admin.save", { defaultValue: "Save" })}
            </button>
          </div>
        </div>
      </form>

      <div className="panel">
        <div className="panel-head"><span className="panel-title">{t("admin.operators.specialties", { defaultValue: "Specialties" })}</span></div>
        <div className="panel-body space-y-2">
          {checkTypes.data?.map((ct) => (
            <label key={ct.id} className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-paper-2">
              <input
                type="checkbox"
                className="h-4 w-4 accent-ink"
                checked={operatorSpecialtyTypeIds.has(ct.id)}
                onChange={() => onToggleSpecialty(ct.id)}
              />
              <span className="text-[13px] font-medium text-ink">{resolveLocaleName(ct, locale)}</span>
              <span className="text-[11px] uppercase tracking-[0.06em] text-ink-3">
                {ct.has_subtypes ? t("admin.check_types.subtyped", { defaultValue: "Subtyped" }) : t("admin.check_types.flat", { defaultValue: "Flat" })}
              </span>
            </label>
          ))}
          {checkTypes.data && checkTypes.data.length === 0 ? (
            <div className="py-8 text-center text-[12px] text-ink-3">
              {t("admin.operators.no_check_types", { defaultValue: "No check types defined yet." })}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}
