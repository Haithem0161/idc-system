import { useState } from "react"
import { Navigate, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"
import { Lock as LockIcon } from "lucide-react"

import { Logo } from "@/components/shell/logo"
import { useLogout, useUnlock } from "@/features/auth/queries"
import { useAuthStore } from "@/stores/auth-store"

export default function LockPage () {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const state = useAuthStore((s) => s.state)
  const unlock = useUnlock()
  const logout = useLogout()
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)

  if (state.kind !== "authenticated") {
    return <Navigate to="/login" replace />
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await unlock.mutateAsync(password)
      navigate("/", { replace: true })
    } catch (err) {
      setError((err as { message?: string }).message ?? t("auth.unlock_failed", { defaultValue: "Wrong password" }))
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-paper px-6 py-10">
      <div className="w-full max-w-md">
        <div className="mb-7 flex flex-col items-center text-center">
          <Logo size={48} className="mb-4 opacity-70" />
          <span className="eyebrow mb-3">{t("auth.lock_eyebrow", { defaultValue: "Session paused" })}</span>
          <h1 className="text-[24px] font-bold leading-[1.1] tracking-[-0.02em] text-ink">
            {t("auth.lock_title", { defaultValue: "Session locked" })}
          </h1>
          <p className="mt-2 text-[13px] text-ink-3">{state.user.email}</p>
        </div>

        <form onSubmit={submit} className="panel">
          <div className="panel-body space-y-5">
            <Field label={t("auth.password_label", { defaultValue: "Password" })}>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="input"
                autoComplete="current-password"
                minLength={8}
                required
                autoFocus
              />
            </Field>
            {error ? (
              <div role="alert" className="status-pill is-danger w-full justify-center">
                {error}
              </div>
            ) : null}
            <button
              type="submit"
              disabled={unlock.isPending}
              className="btn btn-ink w-full"
            >
              <LockIcon className="h-3.5 w-3.5" strokeWidth={1.8} />
              {unlock.isPending
                ? t("auth.unlocking", { defaultValue: "Unlocking..." })
                : t("auth.unlock", { defaultValue: "Unlock" })}
            </button>
            <button
              type="button"
              onClick={() => logout.mutate()}
              className="block w-full text-center text-[11.5px] font-medium uppercase tracking-[0.06em] text-ink-3 transition-colors hover:text-crimson"
            >
              {t("auth.sign_out_instead", { defaultValue: "Sign out instead" })}
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
