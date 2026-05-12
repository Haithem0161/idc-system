import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import { useOperatorCreate, useOperators } from "@/features/catalog/queries"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function OperatorsListPage () {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  const [includeInactive, setIncludeInactive] = useState(false)
  const list = useOperators({
    include_inactive: includeInactive,
    query: query.trim().length >= 2 ? query.trim() : undefined,
  })
  const create = useOperatorCreate()

  const [creating, setCreating] = useState(false)
  const [name, setName] = useState("")
  const [phone, setPhone] = useState("")
  const [cut, setCut] = useState(1000)
  const [error, setError] = useState<string | null>(null)

  const total = list.data?.length ?? 0

  const reset = () => {
    setName("")
    setPhone("")
    setCut(1000)
    setError(null)
    setCreating(false)
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await create.mutateAsync({
        name,
        phone: phone || null,
        base_cut_per_check_iqd: cut,
        notes: null,
      })
      reset()
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <AdminHeader
        title={t("admin.operators.title", { defaultValue: "Operators" })}
        subtitle={t("admin.operators.subtitle", { defaultValue: "Technicians and the cut they earn per check." })}
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
              {creating ? t("admin.cancel", { defaultValue: "Cancel" }) : t("admin.operators.new", { defaultValue: "New operator" })}
            </button>
          </>
        }
      />

      {creating ? (
        <form onSubmit={submit} className="panel">
          <div className="panel-head">
            <span className="panel-title">{t("admin.operators.new", { defaultValue: "New operator" })}</span>
          </div>
          <div className="panel-body space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <FieldLabel label={t("admin.operators.name", { defaultValue: "Name" })}>
                <input type="text" value={name} onChange={(e) => setName(e.target.value)} required className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.operators.phone", { defaultValue: "Phone" })}>
                <input type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.operators.base_cut", { defaultValue: "Base cut (IQD/check)" })}>
                <input type="number" value={cut} min={0} onChange={(e) => setCut(Number(e.target.value))} required className="input font-mono" />
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
              <th>{t("admin.operators.name", { defaultValue: "Name" })}</th>
              <th>{t("admin.operators.phone", { defaultValue: "Phone" })}</th>
              <th className="text-end">{t("admin.operators.base_cut_short", { defaultValue: "Cut/check" })}</th>
              <th>{t("admin.status", { defaultValue: "Status" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((op) => (
              <tr key={op.id}>
                <td className="font-medium text-ink">{op.name}</td>
                <td className="font-mono text-[12px] text-ink-3">{op.phone ?? "—"}</td>
                <td className="text-end font-mono">{op.base_cut_per_check_iqd.toLocaleString()}</td>
                <td>
                  <span className={`status-pill ${op.is_active ? "is-success" : ""}`}>
                    {op.is_active
                      ? t("admin.active", { defaultValue: "Active" })
                      : t("admin.inactive", { defaultValue: "Inactive" })}
                  </span>
                </td>
                <td className="text-end">
                  <Link
                    to={`/admin/operators/${op.id}`}
                    className="inline-flex items-center text-[12px] font-medium text-ink-2 underline-offset-4 transition-colors hover:text-crimson hover:underline"
                  >
                    {t("admin.edit", { defaultValue: "Edit" })}
                  </Link>
                </td>
              </tr>
            ))}
            {total === 0 ? (
              <EmptyRow colSpan={5} message={t("admin.operators.empty", { defaultValue: "No operators yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
