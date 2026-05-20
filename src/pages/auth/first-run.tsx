import { useState } from "react"
import { Navigate, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import { Logo } from "@/components/shell/logo"
import { invoke } from "@/lib/ipc"
import { useFirstAdmin, useHasAnyUser } from "@/features/auth/queries"

export default function FirstRunPage () {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const hasAnyUser = useHasAnyUser()
  const firstAdmin = useFirstAdmin()
  const [email, setEmail] = useState("")
  const [name, setName] = useState("")
  const [password, setPassword] = useState("")
  const [entityId, setEntityId] = useState("")
  const [syncUrl, setSyncUrl] = useState("http://localhost:3161")
  const [error, setError] = useState<string | null>(null)

  if (hasAnyUser.data === true) {
    return <Navigate to="/login" replace />
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      const trimmedUrl = syncUrl.trim()
      if (trimmedUrl) {
        await invoke("config_set_sync_server_url", { url: trimmedUrl })
      }
      await firstAdmin.mutateAsync({
        email,
        name,
        password,
        entity_id: entityId.trim() || undefined,
      })
      navigate("/", { replace: true })
    } catch (err) {
      setError(
        (err as { message?: string }).message ??
          t("auth.first_run_failed", { defaultValue: "Could not create admin" })
      )
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-paper px-6 py-10">
      <div className="w-full max-w-lg">
        <div className="mb-7 flex flex-col items-center text-center">
          <Logo size={56} className="mb-4" />
          <span className="eyebrow mb-3">{t("auth.first_run_eyebrow", { defaultValue: "First launch" })}</span>
          <h1 className="text-[26px] font-bold leading-[1.1] tracking-[-0.022em] text-ink">
            {t("auth.first_run_title", { defaultValue: "Create the first administrator" })}
          </h1>
          <p className="mt-2 max-w-md text-[13px] text-ink-3">
            {t("auth.first_run_body", {
              defaultValue: "This account becomes the superadmin and unlocks the rest of the app.",
            })}
          </p>
        </div>

        <form onSubmit={submit} className="panel">
          <div className="panel-body space-y-5">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field label={t("auth.email_label", { defaultValue: "Email" })}>
                <input
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  className="input"
                  required
                  autoFocus
                />
              </Field>
              <Field label={t("auth.name_label", { defaultValue: "Name" })}>
                <input
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  className="input"
                  required
                />
              </Field>
              <Field label={t("auth.password_label", { defaultValue: "Password" })}>
                <input
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  minLength={8}
                  className="input"
                  required
                />
              </Field>
              <Field label={t("auth.entity_id_label", { defaultValue: "Tenant ID (optional)" })}>
                <input
                  type="text"
                  value={entityId}
                  onChange={(e) => setEntityId(e.target.value)}
                  placeholder="unscoped"
                  className="input"
                />
              </Field>
              <div className="sm:col-span-2">
                <Field label={t("auth.sync_url_label", { defaultValue: "Sync server URL" })}>
                  <input
                    type="url"
                    value={syncUrl}
                    onChange={(e) => setSyncUrl(e.target.value)}
                    placeholder="http://localhost:3161"
                    className="input"
                  />
                </Field>
              </div>
            </div>

            {error ? (
              <div role="alert" className="status-pill is-danger w-full justify-center">
                {error}
              </div>
            ) : null}
            <button
              type="submit"
              disabled={firstAdmin.isPending}
              className="btn btn-primary w-full"
            >
              {firstAdmin.isPending
                ? t("auth.creating", { defaultValue: "Creating..." })
                : t("auth.create_admin", { defaultValue: "Create administrator" })}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function Field ({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="field-label">{label}</span>
      {children}
    </label>
  )
}
