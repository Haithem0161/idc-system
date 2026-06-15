import { useEffect, useState } from "react"
import { Navigate, useLocation, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import { Logo } from "@/components/shell/logo"
import { useLogin } from "@/features/auth/queries"
import { useAuthStore } from "@/stores/auth-store"
import { invoke, isTauri } from "@/lib/ipc"
import { formatIpcError } from "@/lib/errors"
import { AppErrorSchema } from "@/lib/schemas/error"

export default function LoginPage () {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const state = useAuthStore((s) => s.state)
  const login = useLogin()

  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)

  // First-launch is gated on whether THIS machine has a sync server configured,
  // not on a local user count: every fresh machine boots with an empty local DB,
  // so gating on "has any local user" made each one think it was the first. A
  // machine with no sync URL has never been set up -> send it to first-run,
  // which asks the SERVER whether the clinic already has an admin.
  // Resolves to true only in Tauri with no sync URL set. Seeded false outside
  // Tauri (web/dev) so the effect can stay async-only and never setState in a
  // synchronous early-return path.
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(() =>
    isTauri() ? null : false,
  )
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    invoke("config_get_sync_server_url")
      .then((url) => {
        if (!cancelled) setNeedsSetup(!url || url.trim().length === 0)
      })
      .catch(() => {
        // A transient lookup failure is NOT "no URL" -- don't bounce to setup on
        // a bootstrap hiccup; assume configured and let login surface real errors.
        if (!cancelled) setNeedsSetup(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  if (state.kind === "authenticated") {
    return <Navigate to={(location.state as { from?: string } | null)?.from ?? "/"} replace />
  }
  if (needsSetup === true) {
    return <Navigate to="/setup/first-run" replace />
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await login.mutateAsync({ email, password })
      navigate("/", { replace: true })
    } catch (err) {
      // A typed AppError gets a localized code message; anything else falls
      // back to a generic "login failed" notice rather than a raw English
      // Rust string.
      setError(
        AppErrorSchema.safeParse(err).success
          ? formatIpcError(err, t)
          : t("auth.login_failed", { defaultValue: "Login failed" }),
      )
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-paper px-6 py-10">
      <div className="w-full max-w-md">
        <div className="mb-7 flex flex-col items-center text-center">
          <Logo size={56} className="mb-4" />
          <span className="eyebrow mb-3">{t("auth.eyebrow", { defaultValue: "IDC Clinic" })}</span>
          <h1 className="text-[26px] font-bold leading-[1.1] tracking-[-0.022em] text-ink">
            {t("auth.title", { defaultValue: "Sign in to continue" })}
          </h1>
          <p className="mt-2 max-w-xs text-[13px] text-ink-3">
            {t("auth.subtitle", { defaultValue: "Use your clinic account. Offline access is preserved after the first sign in." })}
          </p>
        </div>

        <form
          onSubmit={submit}
          className="panel"
        >
          <div className="panel-body space-y-5">
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
            <Field label={t("auth.password_label", { defaultValue: "Password" })}>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="input"
                autoComplete="current-password"
                minLength={8}
                required
              />
            </Field>
            {error ? (
              <div role="alert" className="status-pill is-danger w-full justify-center">
                {error}
              </div>
            ) : null}
            <button
              type="submit"
              disabled={login.isPending}
              className="btn btn-primary w-full"
            >
              {login.isPending
                ? t("auth.signing_in", { defaultValue: "Signing in..." })
                : t("auth.sign_in", { defaultValue: "Sign in" })}
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
