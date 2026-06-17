import { useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft, Plus } from "lucide-react"

import {
  useCheckTypes,
  useDoctor,
  useDoctorPricingSoftDelete,
  useDoctorPricingUpsert,
  useDoctorSetActive,
  useDoctorSoftDelete,
  useDoctorUpdate,
} from "@/features/catalog/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"
import type { CutKindLiteral, DoctorCutKindLiteral } from "@/lib/ipc"

export default function DoctorDetailPage () {
  const { id = "" } = useParams<{ id: string }>()
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"

  const detail = useDoctor(id)
  const checkTypes = useCheckTypes({ include_inactive: false })
  const update = useDoctorUpdate()
  const setActive = useDoctorSetActive()
  const softDelete = useDoctorSoftDelete()
  const upsertPricing = useDoctorPricingUpsert()
  const deletePricing = useDoctorPricingSoftDelete()

  const [error, setError] = useState<string | null>(null)
  const [pricingForm, setPricingForm] = useState({
    check_type_id: "",
    cut_kind: "pct" as CutKindLiteral,
    cut_value: 30,
    price_override: "",
  })
  // Default-cut editor state, seeded once from the loaded doctor. Blank value
  // clears the default (sends default_cut: null); a value sends [kind, value].
  const [defaultCutKind, setDefaultCutKind] = useState<DoctorCutKindLiteral>("pct")
  const [defaultCutValue, setDefaultCutValue] = useState("")
  const [cutSeeded, setCutSeeded] = useState(false)
  if (detail.data && !cutSeeded) {
    setDefaultCutKind((detail.data.doctor.default_cut_kind ?? "pct") as DoctorCutKindLiteral)
    setDefaultCutValue(
      detail.data.doctor.default_cut_value != null
        ? String(detail.data.doctor.default_cut_value)
        : "",
    )
    setCutSeeded(true)
  }

  if (!detail.data) {
    return (
      <div className="mx-auto max-w-3xl py-12 text-center text-[13px] text-ink-3">
        {detail.isLoading ? t("admin.loading", { defaultValue: "Loading..." }) : t("admin.not_found", { defaultValue: "Not found" })}
      </div>
    )
  }

  const { doctor, pricings } = detail.data
  const checkTypeById = new Map((checkTypes.data ?? []).map((ct) => [ct.id, ct]))

  const onSave = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const form = new FormData(e.currentTarget)
    const cutRaw = defaultCutValue.trim()
    // Blank -> clear the default cut (null). Otherwise send [kind, value].
    const defaultCut: [DoctorCutKindLiteral, number] | null =
      cutRaw === "" ? null : [defaultCutKind, Math.round(Number(cutRaw))]
    try {
      await update.mutateAsync({
        id: doctor.id,
        name: String(form.get("name") ?? ""),
        specialty: (form.get("specialty") as string) || null,
        phone: (form.get("phone") as string) || null,
        notes: (form.get("notes") as string) || null,
        default_cut: defaultCut,
      })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const onAddPricing = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    if (!pricingForm.check_type_id) return
    const ct = checkTypeById.get(pricingForm.check_type_id)
    if (!ct) return
    if (ct.has_subtypes) {
      setError(t("admin.doctors.subtype_picker_required", { defaultValue: "Pick a subtyped check type by editing the row inline (not supported in this MVP form)." }) ?? "")
      return
    }
    try {
      await upsertPricing.mutateAsync({
        doctor_id: doctor.id,
        check_type_id: pricingForm.check_type_id,
        check_subtype_id: null,
        price_override_iqd: pricingForm.price_override === "" ? null : Number(pricingForm.price_override),
        cut_kind: pricingForm.cut_kind,
        cut_value: pricingForm.cut_value,
      })
      setPricingForm({ check_type_id: "", cut_kind: "pct", cut_value: 30, price_override: "" })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <Link to="/admin/doctors" className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink">
        <ArrowLeft className="h-3 w-3 rtl:rotate-180" strokeWidth={1.8} />
        <span>{t("admin.doctors.back", { defaultValue: "Back to doctors" })}</span>
      </Link>

      <AdminHeader
        title={doctor.name}
        subtitle={doctor.specialty ?? undefined}
        actions={
          <>
            <button
              type="button"
              onClick={() => setActive.mutate({ id: doctor.id, is_active: !doctor.is_active })}
              className="btn btn-ghost btn-sm"
            >
              {doctor.is_active
                ? t("admin.deactivate", { defaultValue: "Deactivate" })
                : t("admin.activate", { defaultValue: "Activate" })}
            </button>
            <button
              type="button"
              className="btn btn-danger btn-sm"
              onClick={() => {
                if (confirm(t("admin.doctors.confirm_delete", { defaultValue: "Delete this doctor and all pricings?" }) ?? "")) {
                  softDelete.mutate(doctor.id, {
                    onSuccess: () => navigate("/admin/doctors"),
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
        <div className="panel-head"><span className="panel-title">{t("admin.doctors.details", { defaultValue: "Details" })}</span></div>
        <div className="panel-body space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FieldLabel label={t("admin.doctors.name", { defaultValue: "Name" })}>
              <input type="text" name="name" defaultValue={doctor.name} required className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.doctors.specialty", { defaultValue: "Specialty" })}>
              <input type="text" name="specialty" defaultValue={doctor.specialty ?? ""} className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.doctors.phone", { defaultValue: "Phone" })}>
              <input type="tel" name="phone" defaultValue={doctor.phone ?? ""} className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.doctors.notes", { defaultValue: "Notes" })}>
              <input type="text" name="notes" defaultValue={doctor.notes ?? ""} className="input" />
            </FieldLabel>
          </div>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <FieldLabel label={t("admin.doctors.default_cut_kind", { defaultValue: "Default cut kind" })}>
              <select
                value={defaultCutKind}
                onChange={(e) => setDefaultCutKind(e.target.value as DoctorCutKindLiteral)}
                className="input"
              >
                <option value="pct">{t("admin.doctors.cut_pct", { defaultValue: "Percentage" })}</option>
                <option value="fixed">{t("admin.doctors.cut_fixed", { defaultValue: "Fixed (IQD)" })}</option>
              </select>
            </FieldLabel>
            <FieldLabel
              label={
                defaultCutKind === "pct"
                  ? t("admin.doctors.default_cut_pct", { defaultValue: "Default cut %" })
                  : t("admin.doctors.default_cut_iqd", { defaultValue: "Default cut (IQD)" })
              }
            >
              <input
                type="number"
                min={0}
                max={defaultCutKind === "pct" ? 100 : undefined}
                value={defaultCutValue}
                onChange={(e) => setDefaultCutValue(e.target.value)}
                className="input font-mono"
                placeholder={t("admin.doctors.default_cut_none", { defaultValue: "None" }) ?? ""}
              />
            </FieldLabel>
          </div>
          <p className="text-[11px] text-ink-3">
            {t("admin.doctors.default_cut_hint", {
              defaultValue:
                "Used when no per-check pricing row below matches. Leave blank for no default (cut falls to 0).",
            })}
          </p>
          <ErrorBanner message={error} />
          <div className="flex justify-end">
            <button type="submit" disabled={update.isPending} className="btn btn-primary btn-sm">
              {t("admin.save", { defaultValue: "Save" })}
            </button>
          </div>
        </div>
      </form>

      <div className="panel overflow-hidden">
        <div className="panel-head"><span className="panel-title">{t("admin.doctors.pricings", { defaultValue: "Pricing rows" })}</span></div>
        <form onSubmit={onAddPricing} className="panel-body space-y-3 border-b border-line">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-5">
            <FieldLabel label={t("admin.doctors.pricing_check_type", { defaultValue: "Check type" })}>
              <select
                value={pricingForm.check_type_id}
                onChange={(e) => setPricingForm((f) => ({ ...f, check_type_id: e.target.value }))}
                required
                className="input"
              >
                <option value="">—</option>
                {checkTypes.data?.filter((ct) => !ct.has_subtypes).map((ct) => (
                  <option key={ct.id} value={ct.id}>{resolveLocaleName(ct, locale)}</option>
                ))}
              </select>
            </FieldLabel>
            <FieldLabel label={t("admin.doctors.cut_kind", { defaultValue: "Cut kind" })}>
              <select
                value={pricingForm.cut_kind}
                onChange={(e) => setPricingForm((f) => ({ ...f, cut_kind: e.target.value as CutKindLiteral }))}
                className="input"
              >
                <option value="pct">{t("admin.doctors.cut_pct", { defaultValue: "Percentage" })}</option>
                <option value="fixed">{t("admin.doctors.cut_fixed", { defaultValue: "Fixed (IQD)" })}</option>
              </select>
            </FieldLabel>
            <FieldLabel label={pricingForm.cut_kind === "pct" ? t("admin.doctors.cut_value_pct", { defaultValue: "Cut %" }) : t("admin.doctors.cut_value_iqd", { defaultValue: "Cut (IQD)" })}>
              <input
                type="number"
                min={0}
                max={pricingForm.cut_kind === "pct" ? 100 : undefined}
                value={pricingForm.cut_value}
                onChange={(e) => setPricingForm((f) => ({ ...f, cut_value: Number(e.target.value) }))}
                className="input font-mono"
              />
            </FieldLabel>
            <FieldLabel label={t("admin.doctors.price_override", { defaultValue: "Price override (IQD)" })}>
              <input
                type="number"
                min={0}
                value={pricingForm.price_override}
                onChange={(e) => setPricingForm((f) => ({ ...f, price_override: e.target.value }))}
                className="input font-mono"
                placeholder="—"
              />
            </FieldLabel>
            <div className="flex items-end">
              <button type="submit" disabled={upsertPricing.isPending} className="btn btn-primary btn-sm w-full">
                <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />
                {t("admin.doctors.add_pricing", { defaultValue: "Add row" })}
              </button>
            </div>
          </div>
        </form>
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("admin.doctors.pricing_check_type", { defaultValue: "Check type" })}</th>
              <th>{t("admin.doctors.cut", { defaultValue: "Cut" })}</th>
              <th className="text-end">{t("admin.doctors.price_override_short", { defaultValue: "Override" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {pricings.map((p) => {
              const ct = checkTypeById.get(p.check_type_id)
              return (
                <tr key={p.id}>
                  <td className="font-medium text-ink">
                    {ct ? resolveLocaleName(ct, locale) : p.check_type_id.slice(0, 8)}
                  </td>
                  <td className="font-mono text-[12px] text-ink-3">
                    {p.cut_kind === "pct" ? `${p.cut_value}%` : `${p.cut_value.toLocaleString()} IQD`}
                  </td>
                  <td className="text-end font-mono">
                    {p.price_override_iqd != null ? p.price_override_iqd.toLocaleString() : "—"}
                  </td>
                  <td className="text-end">
                    <button
                      type="button"
                      onClick={() => deletePricing.mutate({ id: p.id, doctor_id: doctor.id })}
                      className="text-[12px] font-medium text-crimson hover:underline"
                    >
                      {t("admin.delete", { defaultValue: "Delete" })}
                    </button>
                  </td>
                </tr>
              )
            })}
            {pricings.length === 0 ? (
              <EmptyRow colSpan={4} message={t("admin.doctors.no_pricings", { defaultValue: "No pricing rows yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
