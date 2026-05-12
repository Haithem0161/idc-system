import { useState } from "react"
import { Navigate, useLocation, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import { Logo } from "@/components/shell/logo"
import { useHasAnyUser, useLogin } from "@/features/auth/queries"
import { useAuthStore } from "@/stores/auth-store"

export default function LoginPage () {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const state = useAuthStore((s) => s.state)
  const login = useLogin()
  const hasAnyUser = useHasAnyUser()

  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)

  if (state.kind === "authenticated") {
    return <Navigate to={(location.state as { from?: string } | null)?.from ?? "/"} replace />
  }
  if (hasAnyUser.data === false) {
    return <Navigate to="/setup/first-run" replace />
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await login.mutateAsync({ email, password })
      navigate("/", { replace: true })
    } catch (err) {
      setError((err as { message?: string }).message ?? t("auth.login_failed", { defaultValue: "Login failed" }))
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
