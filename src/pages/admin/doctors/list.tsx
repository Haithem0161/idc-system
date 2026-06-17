import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import { useDoctorCreate, useDoctors } from "@/features/catalog/queries"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"
import type { DoctorCutKindLiteral } from "@/lib/ipc"

export default function DoctorsListPage () {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  const [includeInactive, setIncludeInactive] = useState(false)
  const list = useDoctors({
    include_inactive: includeInactive,
    query: query.trim().length >= 2 ? query.trim() : undefined,
  })
  const create = useDoctorCreate()
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState("")
  const [specialty, setSpecialty] = useState("")
  const [phone, setPhone] = useState("")
  const [cutKind, setCutKind] = useState<DoctorCutKindLiteral>("pct")
  const [cutValue, setCutValue] = useState("")
  const [error, setError] = useState<string | null>(null)

  const total = list.data?.length ?? 0

  const reset = () => {
    setName("")
    setSpecialty("")
    setPhone("")
    setCutKind("pct")
    setCutValue("")
    setError(null)
    setCreating(false)
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const cutRaw = cutValue.trim()
    try {
      await create.mutateAsync({
        name,
        specialty: specialty || null,
        phone: phone || null,
        notes: null,
        // Both halves go together; omit entirely when no default cut is set.
        ...(cutRaw === ""
          ? {}
          : { default_cut_kind: cutKind, default_cut_value: Math.round(Number(cutRaw)) }),
      })
      reset()
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <AdminHeader
        title={t("admin.doctors.title", { defaultValue: "Doctors" })}
        subtitle={t("admin.doctors.subtitle", { defaultValue: "Referring physicians and their per-check pricing." })}
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
              {creating ? t("admin.cancel", { defaultValue: "Cancel" }) : t("admin.doctors.new", { defaultValue: "New doctor" })}
            </button>
          </>
        }
      />

      {creating ? (
        <form onSubmit={submit} className="panel">
          <div className="panel-head">
            <span className="panel-title">{t("admin.doctors.new", { defaultValue: "New doctor" })}</span>
          </div>
          <div className="panel-body space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <FieldLabel label={t("admin.doctors.name", { defaultValue: "Name" })}>
                <input type="text" value={name} onChange={(e) => setName(e.target.value)} required className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.doctors.specialty", { defaultValue: "Specialty" })}>
                <input type="text" value={specialty} onChange={(e) => setSpecialty(e.target.value)} className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.doctors.phone", { defaultValue: "Phone" })}>
                <input type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} className="input" />
              </FieldLabel>
            </div>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <FieldLabel label={t("admin.doctors.default_cut_kind", { defaultValue: "Default cut kind" })}>
                <select
                  value={cutKind}
                  onChange={(e) => setCutKind(e.target.value as DoctorCutKindLiteral)}
                  className="input"
                >
                  <option value="pct">{t("admin.doctors.cut_pct", { defaultValue: "Percentage" })}</option>
                  <option value="fixed">{t("admin.doctors.cut_fixed", { defaultValue: "Fixed (IQD)" })}</option>
                </select>
              </FieldLabel>
              <FieldLabel
                label={
                  cutKind === "pct"
                    ? t("admin.doctors.default_cut_pct", { defaultValue: "Default cut %" })
                    : t("admin.doctors.default_cut_iqd", { defaultValue: "Default cut (IQD)" })
                }
              >
                <input
                  type="number"
                  min={0}
                  max={cutKind === "pct" ? 100 : undefined}
                  value={cutValue}
                  onChange={(e) => setCutValue(e.target.value)}
                  className="input font-mono"
                  placeholder={t("admin.doctors.default_cut_none", { defaultValue: "None" }) ?? ""}
                />
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
              <th>{t("admin.doctors.name", { defaultValue: "Name" })}</th>
              <th>{t("admin.doctors.specialty", { defaultValue: "Specialty" })}</th>
              <th>{t("admin.doctors.phone", { defaultValue: "Phone" })}</th>
              <th>{t("admin.doctors.default_cut", { defaultValue: "Default cut" })}</th>
              <th>{t("admin.status", { defaultValue: "Status" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((d) => (
              <tr key={d.id}>
                <td className="font-medium text-ink">{d.name}</td>
                <td className="text-[12px] text-ink-3">{d.specialty ?? "—"}</td>
                <td className="font-mono text-[12px] text-ink-3">{d.phone ?? "—"}</td>
                <td className="font-mono text-[12px] text-ink-3">
                  {d.default_cut_kind === "pct"
                    ? `${d.default_cut_value}%`
                    : d.default_cut_kind === "fixed"
                      ? `${(d.default_cut_value ?? 0).toLocaleString()} IQD`
                      : "—"}
                </td>
                <td>
                  <span className={`status-pill ${d.is_active ? "is-success" : ""}`}>
                    {d.is_active
                      ? t("admin.active", { defaultValue: "Active" })
                      : t("admin.inactive", { defaultValue: "Inactive" })}
                  </span>
                </td>
                <td className="text-end">
                  <Link
                    to={`/admin/doctors/${d.id}`}
                    className="inline-flex items-center text-[12px] font-medium text-ink-2 underline-offset-4 transition-colors hover:text-crimson hover:underline"
                  >
                    {t("admin.edit", { defaultValue: "Edit" })}
                  </Link>
                </td>
              </tr>
            ))}
            {total === 0 ? (
              <EmptyRow colSpan={6} message={t("admin.doctors.empty", { defaultValue: "No doctors yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
