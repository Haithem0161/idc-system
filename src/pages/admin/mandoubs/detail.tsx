import { useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft } from "lucide-react"

import {
  useMandoub,
  useMandoubSetActive,
  useMandoubSoftDelete,
  useMandoubUpdate,
} from "@/features/catalog/queries"
import { AdminHeader, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

export default function MandoubDetailPage () {
  const { id = "" } = useParams<{ id: string }>()
  const { t } = useTranslation()
  const navigate = useNavigate()

  const detail = useMandoub(id)
  const update = useMandoubUpdate()
  const setActive = useMandoubSetActive()
  const softDelete = useMandoubSoftDelete()

  const [error, setError] = useState<string | null>(null)

  if (!detail.data) {
    return (
      <div className="mx-auto max-w-3xl py-12 text-center text-[13px] text-ink-3">
        {detail.isLoading ? t("admin.loading", { defaultValue: "Loading..." }) : t("admin.not_found", { defaultValue: "Not found" })}
      </div>
    )
  }

  const { mandoub } = detail.data

  const onSave = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const form = new FormData(e.currentTarget)
    try {
      await update.mutateAsync({
        id: mandoub.id,
        name: String(form.get("name") ?? ""),
        phone: (form.get("phone") as string) || null,
        notes: (form.get("notes") as string) || null,
      })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <Link to="/admin/mandoubs" className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink">
        <ArrowLeft className="h-3 w-3 rtl:rotate-180" strokeWidth={1.8} />
        <span>{t("admin.mandoubs.back", { defaultValue: "Back to representatives" })}</span>
      </Link>

      <AdminHeader
        title={mandoub.name}
        subtitle={mandoub.phone ?? undefined}
        actions={
          <>
            <button
              type="button"
              onClick={() => setActive.mutate({ id: mandoub.id, is_active: !mandoub.is_active })}
              className="btn btn-ghost btn-sm"
            >
              {mandoub.is_active
                ? t("admin.deactivate", { defaultValue: "Deactivate" })
                : t("admin.activate", { defaultValue: "Activate" })}
            </button>
            <button
              type="button"
              className="btn btn-danger btn-sm"
              onClick={() => {
                if (confirm(t("admin.mandoubs.confirm_delete", { defaultValue: "Delete this representative?" }) ?? "")) {
                  softDelete.mutate(mandoub.id, {
                    onSuccess: () => navigate("/admin/mandoubs"),
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
        <div className="panel-head"><span className="panel-title">{t("admin.mandoubs.details", { defaultValue: "Details" })}</span></div>
        <div className="panel-body space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FieldLabel label={t("admin.mandoubs.name", { defaultValue: "Name" })}>
              <input type="text" name="name" defaultValue={mandoub.name} required className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.mandoubs.phone", { defaultValue: "Phone" })}>
              <input type="tel" name="phone" defaultValue={mandoub.phone ?? ""} className="input" />
            </FieldLabel>
            <FieldLabel label={t("admin.mandoubs.notes", { defaultValue: "Notes" })}>
              <input type="text" name="notes" defaultValue={mandoub.notes ?? ""} className="input" />
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
    </div>
  )
}
