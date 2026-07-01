import { useEffect, useState } from "react"
import { Navigate, useLocation, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"
import { Server } from "lucide-react"

import { Logo } from "@/components/shell/logo"
import { LanguageToggle } from "@/components/shell/language-toggle"
import { useLogin, useBootstrapJwtKey } from "@/features/auth/queries"
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
        {isTauri() ? (
          <div className="mb-4 flex justify-center">
            <LanguageToggle />
          </div>
        ) : null}
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

        {isTauri() ? <ChangeServer /> : null}
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

/**
 * Pre-login "change sync server" affordance. A device can be pointed at the
 * wrong clinic server (or need re-pointing before anyone can sign in), so this
 * lives on the login screen -- not just behind the superadmin settings page.
 * Uses the unguarded bootstrap IPC (`config_set_sync_server_url`), the same
 * path first-launch setup uses, and re-pins the server's JWT key (TOFU) so
 * offline verification works against the new server.
 */
function ChangeServer () {
  const { t } = useTranslation()
  const bootstrapJwtKey = useBootstrapJwtKey()
  const [open, setOpen] = useState(false)
  const [url, setUrl] = useState("")
  const [current, setCurrent] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    let cancelled = false
    invoke("config_get_sync_server_url")
      .then((u) => {
        if (cancelled) return
        setCurrent(u ?? null)
        if (u) setUrl(u)
      })
      .catch(() => {
        /* transient bootstrap failure -- leave current unknown */
      })
    return () => {
      cancelled = true
    }
  }, [])

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const trimmed = url.trim()
    if (!trimmed) {
      setError(t("setup.url_required", { defaultValue: "Sync server URL is required" }))
      return
    }
    setSaving(true)
    setError(null)
    setSaved(false)
    try {
      await invoke("config_set_sync_server_url", { url: trimmed })
      // Re-pin the new server's RS256 public key for offline JWT verification.
      // Best-effort: a network hiccup must not block the change.
      try {
        await bootstrapJwtKey.mutateAsync({ server_url: trimmed })
      } catch {
        /* non-fatal: re-pinned on a later online action */
      }
      setCurrent(trimmed)
      setSaved(true)
      setOpen(false)
    } catch (err) {
      setError(
        (err as { message?: string }).message ??
          t("setup.url_invalid", { defaultValue: "Failed to save sync server URL" }),
      )
    } finally {
      setSaving(false)
    }
  }

  if (!open) {
    return (
      <div className="mt-4 flex flex-col items-center gap-1.5">
        <button
          type="button"
          onClick={() => {
            setSaved(false)
            setOpen(true)
          }}
          className="inline-flex items-center gap-1.5 text-[12px] font-medium text-ink-3 transition-colors hover:text-ink"
        >
          <Server className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
          <span>{t("setup.change_server", { defaultValue: "Change sync server" })}</span>
        </button>
        {saved ? (
          <span className="text-[11px] text-success">
            {t("setup.server_saved", { defaultValue: "Sync server updated." })}
          </span>
        ) : current ? (
          <span className="max-w-full truncate text-[11px] text-ink-4" title={current}>
            {t("setup.current_server", { defaultValue: "Current server" })}: {current}
          </span>
        ) : null}
      </div>
    )
  }

  return (
    <form onSubmit={submit} className="panel mt-4">
      <div className="panel-body space-y-4">
        <label className="block">
          <span className="field-label">
            {t("setup.url_label", { defaultValue: "Sync server URL" })}
          </span>
          <input
            type="url"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://idc-sync.madebyhaithem.com"
            className="input"
            required
            autoFocus
          />
        </label>
        {error ? (
          <div role="alert" className="status-pill is-danger w-full justify-center">
            {error}
          </div>
        ) : null}
        <div className="flex gap-2">
          <button type="submit" disabled={saving} className="btn btn-ink flex-1">
            {saving
              ? t("setup.saving", { defaultValue: "Saving..." })
              : t("setup.save", { defaultValue: "Save" })}
          </button>
          <button
            type="button"
            onClick={() => {
              setOpen(false)
              setError(null)
              setUrl(current ?? "")
            }}
            className="btn btn-ghost"
          >
            {t("setup.cancel", { defaultValue: "Cancel" })}
          </button>
        </div>
      </div>
    </form>
  )
}
