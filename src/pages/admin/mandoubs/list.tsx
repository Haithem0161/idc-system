import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import { useMandoubCreate, useMandoubs } from "@/features/catalog/queries"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function MandoubsListPage () {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  const [includeInactive, setIncludeInactive] = useState(false)
  const list = useMandoubs({
    include_inactive: includeInactive,
    query: query.trim().length >= 2 ? query.trim() : undefined,
  })
  const create = useMandoubCreate()

  const [creating, setCreating] = useState(false)
  const [name, setName] = useState("")
  const [phone, setPhone] = useState("")
  const [error, setError] = useState<string | null>(null)

  const total = list.data?.length ?? 0

  const reset = () => {
    setName("")
    setPhone("")
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
        title={t("admin.mandoubs.title", { defaultValue: "Representatives" })}
        subtitle={t("admin.mandoubs.subtitle", { defaultValue: "Representatives who refer patients alongside a doctor." })}
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
              {creating ? t("admin.cancel", { defaultValue: "Cancel" }) : t("admin.mandoubs.new", { defaultValue: "New representative" })}
            </button>
          </>
        }
      />

      {creating ? (
        <form onSubmit={submit} className="panel">
          <div className="panel-head">
            <span className="panel-title">{t("admin.mandoubs.new", { defaultValue: "New representative" })}</span>
          </div>
          <div className="panel-body space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <FieldLabel label={t("admin.mandoubs.name", { defaultValue: "Name" })}>
                <input type="text" value={name} onChange={(e) => setName(e.target.value)} required className="input" />
              </FieldLabel>
              <FieldLabel label={t("admin.mandoubs.phone", { defaultValue: "Phone" })}>
                <input type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} className="input" />
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
              <th>{t("admin.mandoubs.name", { defaultValue: "Name" })}</th>
              <th>{t("admin.mandoubs.phone", { defaultValue: "Phone" })}</th>
              <th>{t("admin.status", { defaultValue: "Status" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((m) => (
              <tr key={m.id}>
                <td className="font-medium text-ink">{m.name}</td>
                <td className="font-mono text-[12px] text-ink-3">{m.phone ?? "—"}</td>
                <td>
                  <span className={`status-pill ${m.is_active ? "is-success" : ""}`}>
                    {m.is_active
                      ? t("admin.active", { defaultValue: "Active" })
                      : t("admin.inactive", { defaultValue: "Inactive" })}
                  </span>
                </td>
                <td className="text-end">
                  <Link
                    to={`/admin/mandoubs/${m.id}`}
                    className="inline-flex items-center text-[12px] font-medium text-ink-2 underline-offset-4 transition-colors hover:text-crimson hover:underline"
                  >
                    {t("admin.edit", { defaultValue: "Edit" })}
                  </Link>
                </td>
              </tr>
            ))}
            {total === 0 ? (
              <EmptyRow colSpan={4} message={t("admin.mandoubs.empty", { defaultValue: "No representatives yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
