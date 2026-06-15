import { useEffect, useState } from "react"
import { Navigate, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import { Logo } from "@/components/shell/logo"
import { invoke } from "@/lib/ipc"
import { AppErrorSchema } from "@/lib/schemas/error"
import { formatIpcError } from "@/lib/errors"
import {
  useBootstrapJwtKey,
  useBootstrapStatus,
  useFirstAdmin,
  useHasAnyUser,
} from "@/features/auth/queries"

// Production sync server. Overridable at build time (VITE_SYNC_SERVER_URL) for
// staging/dev so a local run can point at http://localhost:3161 without editing
// this file. The user can still change it in the field below.
const DEFAULT_SYNC_URL =
  import.meta.env.VITE_SYNC_SERVER_URL ?? "https://idc-sync.madebyhaithem.com"

// First-launch is server-authoritative. A clinic has ONE superadmin, created
// ONCE, living on the server; every machine is just a client that points at the
// server and logs in. So the flow is:
//   url      -> enter the sync server URL, pin its key, ask the server
//   checking -> waiting on GET /auth/bootstrap-status
//   blocked  -> server unreachable (a fresh client genuinely needs it once)
//   create   -> server has NO admin yet -> create the first one (server-side)
//   (initialized=true -> redirect to /login; this machine just signs in)
type Step = "url" | "checking" | "blocked" | "create"

export default function FirstRunPage () {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const hasAnyUser = useHasAnyUser()
  const firstAdmin = useFirstAdmin()
  const bootstrapJwtKey = useBootstrapJwtKey()
  const bootstrapStatus = useBootstrapStatus()

  const [step, setStep] = useState<Step>("url")
  const [syncUrl, setSyncUrl] = useState(DEFAULT_SYNC_URL)
  const [email, setEmail] = useState("")
  const [name, setName] = useState("")
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)

  // If this machine already has a local user, first-run is done -- go sign in.
  useEffect(() => {
    if (hasAnyUser.data === true) navigate("/login", { replace: true })
  }, [hasAnyUser.data, navigate])

  if (hasAnyUser.data === true) {
    return <Navigate to="/login" replace />
  }

  // Step 1: persist the URL, pin the server key, then ask the server whether
  // the clinic already has an admin. Reused by the "Retry" button on blocked.
  const probeServer = async (url: string) => {
    setError(null)
    setStep("checking")
    try {
      await invoke("config_set_sync_server_url", { url })
      // Pin the server's RS256 public key now (TOFU) so the client can verify
      // JWT signatures offline. Best-effort: a hiccup here must not block setup.
      try {
        await bootstrapJwtKey.mutateAsync({ server_url: url })
      } catch {
        // Non-fatal: pinning is retried on a later online action.
      }
      const initialized = await bootstrapStatus.mutateAsync(url)
      if (initialized) {
        // The clinic already has an admin: this machine just signs in.
        navigate("/login", { replace: true })
        return
      }
      // Genuine first machine: collect the admin's details.
      setStep("create")
    } catch (err) {
      // An unreachable server (NETWORK_OFFLINE / SERVER_UNAVAILABLE) blocks with
      // a retry -- a brand-new client truly cannot proceed offline. Any other
      // error is shown inline on the URL step so the user can correct it.
      const code = AppErrorSchema.safeParse(err)
      const offline =
        code.success &&
        (code.data.code === "NETWORK_OFFLINE" || code.data.code === "SERVER_UNAVAILABLE")
      setError(formatIpcError(err, t))
      setStep(offline ? "blocked" : "url")
    }
  }

  const submitUrl = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const url = syncUrl.trim()
    if (!url) {
      setError(t("setup.url_required", { defaultValue: "Sync server URL is required" }))
      return
    }
    void probeServer(url)
  }

  const submitAdmin = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      // No tenant: the server owns tenancy and stamps its DEFAULT_ENTITY_ID.
      await firstAdmin.mutateAsync({ email, name, password })
      navigate("/", { replace: true })
    } catch (err) {
      setError(
        AppErrorSchema.safeParse(err).success
          ? formatIpcError(err, t)
          : t("auth.first_run_failed", { defaultValue: "Could not create admin" }),
      )
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-paper px-6 py-10">
      <div className="w-full max-w-lg">
        <div className="mb-7 flex flex-col items-center text-center">
          <Logo size={56} className="mb-4" />
          <span className="eyebrow mb-3">{t("auth.first_run_eyebrow", { defaultValue: "First launch" })}</span>
          {step === "create" ? (
            <>
              <h1 className="text-[26px] font-bold leading-[1.1] tracking-[-0.022em] text-ink">
                {t("auth.first_run_title", { defaultValue: "Create the first administrator" })}
              </h1>
              <p className="mt-2 max-w-md text-[13px] text-ink-3">
                {t("auth.first_run_body", {
                  defaultValue: "This account becomes the superadmin and unlocks the rest of the app.",
                })}
              </p>
            </>
          ) : (
            <>
              <h1 className="text-[26px] font-bold leading-[1.1] tracking-[-0.022em] text-ink">
                {t("setup.title", { defaultValue: "Connect to your clinic" })}
              </h1>
              <p className="mt-2 max-w-md text-[13px] text-ink-3">
                {t("setup.subtitle_v2", {
                  defaultValue: "Enter the address of your clinic's sync server. You can change this later in settings.",
                })}
              </p>
            </>
          )}
        </div>

        {step === "create" ? (
          <form onSubmit={submitAdmin} className="panel">
            <div className="panel-body space-y-5">
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <Field label={t("auth.email_label", { defaultValue: "Email" })}>
                  <input
                    type="email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    className="input"
                    autoComplete="email"
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
                <div className="sm:col-span-2">
                  <Field label={t("auth.password_label", { defaultValue: "Password" })}>
                    <input
                      type="password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      minLength={8}
                      className="input"
                      autoComplete="new-password"
                      required
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
        ) : (
          <form onSubmit={submitUrl} className="panel">
            <div className="panel-body space-y-5">
              <Field label={t("setup.url_label", { defaultValue: "Sync server URL" })}>
                <input
                  type="url"
                  value={syncUrl}
                  onChange={(e) => setSyncUrl(e.target.value)}
                  placeholder="https://idc-sync.madebyhaithem.com"
                  className="input"
                  autoFocus
                  required
                  disabled={step === "checking"}
                />
              </Field>

              {step === "blocked" ? (
                <div role="alert" className="status-pill is-danger w-full justify-center">
                  {error ??
                    t("setup.server_unreachable", {
                      defaultValue: "Can't reach the sync server. Check the address and try again.",
                    })}
                </div>
              ) : error ? (
                <div role="alert" className="status-pill is-danger w-full justify-center">
                  {error}
                </div>
              ) : null}

              <button
                type="submit"
                disabled={step === "checking"}
                className="btn btn-primary w-full"
              >
                {step === "checking"
                  ? t("setup.connecting", { defaultValue: "Connecting..." })
                  : step === "blocked"
                    ? t("setup.retry", { defaultValue: "Retry" })
                    : t("setup.continue", { defaultValue: "Continue" })}
              </button>
            </div>
          </form>
        )}
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
