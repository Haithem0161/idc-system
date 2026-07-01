import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import {
  useCheckTypeCreate,
  useCheckTypes,
} from "@/features/catalog/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"
import type { CheckTypeCreateArgs } from "@/lib/ipc"

export default function CheckTypesListPage () {
  const { t, i18n } = useTranslation()
  const [query, setQuery] = useState("")
  const [includeInactive, setIncludeInactive] = useState(false)
  const list = useCheckTypes({
    include_inactive: includeInactive,
    query: query.trim().length >= 2 ? query.trim() : undefined,
  })
  const create = useCheckTypeCreate()

  const [creating, setCreating] = useState(false)
  const [form, setForm] = useState<CheckTypeCreateArgs>({
    name_ar: "",
    name_en: "",
    has_subtypes: false,
    base_price_iqd: 0,
    dye_supported: false,
    sort_order: 0,
  })
  const [error, setError] = useState<string | null>(null)

  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"
  const total = list.data?.length ?? 0

  const reset = () => {
    setForm({ name_ar: "", name_en: "", has_subtypes: false, base_price_iqd: 0, dye_supported: false, sort_order: 0 })
    setError(null)
    setCreating(false)
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      const payload: CheckTypeCreateArgs = {
        ...form,
        name_en: form.name_en && form.name_en.trim().length > 0 ? form.name_en : null,
        base_price_iqd: form.has_subtypes ? null : (form.base_price_iqd ?? 0),
      }
      await create.mutateAsync(payload)
      reset()
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <AdminHeader
        title={t("admin.check_types.title", { defaultValue: "Check Types" })}
        subtitle={t("admin.check_types.subtitle", { defaultValue: "Procedures offered and their pricing." })}
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
            <button
              type="button"
              onClick={() => setCreating((v) => !v)}
              className="btn btn-primary btn-sm"
            >
              {creating ? <X className="h-3.5 w-3.5" strokeWidth={1.8} /> : <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />}
              {creating
                ? t("admin.cancel", { defaultValue: "Cancel" })
                : t("admin.check_types.new", { defaultValue: "New check type" })}
            </button>
          </>
        }
      />

      {creating ? (
        <form onSubmit={submit} className="panel">
          <div className="panel-head">
            <span className="panel-title">{t("admin.check_types.new", { defaultValue: "New check type" })}</span>
          </div>
          <div className="panel-body space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <FieldLabel label={t("admin.check_types.name_ar", { defaultValue: "Name (AR)" })}>
                <input
                  type="text"
                  value={form.name_ar}
                  onChange={(e) => setForm((f) => ({ ...f, name_ar: e.target.value }))}
                  required
                  className="input"
                />
              </FieldLabel>
              <FieldLabel label={t("admin.check_types.name_en", { defaultValue: "Name (EN)" })}>
                <input
                  type="text"
                  value={form.name_en ?? ""}
                  onChange={(e) => setForm((f) => ({ ...f, name_en: e.target.value }))}
                  className="input"
                />
              </FieldLabel>
              <label className="inline-flex items-center gap-2 text-[12px] font-medium text-ink-2">
                <input
                  type="checkbox"
                  checked={form.has_subtypes}
                  onChange={(e) => setForm((f) => ({ ...f, has_subtypes: e.target.checked }))}
                  className="h-4 w-4 accent-ink"
                />
                <span>{t("admin.check_types.has_subtypes", { defaultValue: "Uses subtypes" })}</span>
              </label>
              {!form.has_subtypes ? (
                <FieldLabel label={t("admin.check_types.base_price", { defaultValue: "Base price (IQD)" })}>
                  <input
                    type="number"
                    min={0}
                    value={form.base_price_iqd ?? 0}
                    onChange={(e) => setForm((f) => ({ ...f, base_price_iqd: Number(e.target.value) }))}
                    className="input font-mono"
                  />
                </FieldLabel>
              ) : null}
              <label className="inline-flex items-center gap-2 text-[12px] font-medium text-ink-2">
                <input
                  type="checkbox"
                  checked={form.dye_supported}
                  onChange={(e) => setForm((f) => ({ ...f, dye_supported: e.target.checked }))}
                  className="h-4 w-4 accent-ink"
                />
                <span>{t("admin.check_types.dye_supported", { defaultValue: "Supports dye" })}</span>
              </label>
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
              <th>{t("admin.check_types.name", { defaultValue: "Name" })}</th>
              <th>{t("admin.check_types.mode", { defaultValue: "Mode" })}</th>
              <th className="text-end">{t("admin.check_types.base_price_short", { defaultValue: "Base" })}</th>
              <th>{t("admin.check_types.flags", { defaultValue: "Flags" })}</th>
              <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((ct) => (
              <tr key={ct.id}>
                <td className="font-medium text-ink">{resolveLocaleName(ct, locale)}</td>
                <td>
                  <span className={`status-pill ${ct.has_subtypes ? "is-info" : ""}`}>
                    {ct.has_subtypes
                      ? t("admin.check_types.subtyped", { defaultValue: "Subtyped" })
                      : t("admin.check_types.flat", { defaultValue: "Flat" })}
                  </span>
                </td>
                <td className="text-end font-mono">
                  {ct.base_price_iqd != null ? ct.base_price_iqd.toLocaleString() : "—"}
                </td>
                <td className="text-[12px] text-ink-3">
                  {ct.dye_supported ? t("admin.check_types.dye", { defaultValue: "Dye" }) : "—"}
                </td>
                <td className="text-end">
                  <Link
                    to={`/admin/check-types/${ct.id}`}
                    className="inline-flex items-center text-[12px] font-medium text-ink-2 underline-offset-4 transition-colors hover:text-crimson hover:underline"
                  >
                    {t("admin.edit", { defaultValue: "Edit" })}
                  </Link>
                </td>
              </tr>
            ))}
            {total === 0 ? (
              <EmptyRow colSpan={5} message={t("admin.check_types.empty", { defaultValue: "No check types yet" })} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
