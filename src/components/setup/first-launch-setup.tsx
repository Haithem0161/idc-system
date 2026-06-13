import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { invoke, isTauri } from "@/lib/ipc"
import { Logo } from "@/components/shell/logo"

/**
 * First-launch modal (phase-01 §7.22). Captures the sync server URL via the
 * Tauri config IPC; stays closed once the URL is set or the env override is
 * present.
 */
export function FirstLaunchSetup() {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [url, setUrl] = useState("")
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    // Only open first-launch setup when the URL is genuinely absent. A failed
    // lookup (e.g. the Rust state isn't managed yet during bootstrap) is a
    // transient error, NOT "no URL configured" -- opening the modal then would
    // mask the real failure and prompt the user to re-enter a URL they may
    // already have. Retry once, then leave setup closed and log the error.
    const probe = (attempt: number) => {
      invoke("config_get_sync_server_url")
        .then((existing) => {
          if (cancelled) return
          setOpen(!existing || existing.trim().length === 0)
        })
        .catch((err) => {
          if (cancelled) return
          if (attempt < 1) {
            window.setTimeout(() => {
              if (!cancelled) probe(attempt + 1)
            }, 500)
            return
          }
          console.error("config_get_sync_server_url failed; not opening first-launch setup", err)
        })
    }
    probe(0)
    return () => {
      cancelled = true
    }
  }, [])

  if (!open) return null

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!url.trim()) {
      setError(t("setup.url_required", { defaultValue: "Sync server URL is required" }))
      return
    }
    setSubmitting(true)
    setError(null)
    try {
      await invoke("config_set_sync_server_url", { url: url.trim() })
      setOpen(false)
    } catch (err) {
      setError(
        (err as { message?: string }).message ??
          t("setup.url_invalid", { defaultValue: "Failed to save sync server URL" })
      )
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="setup-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 px-6 py-10 backdrop-blur-sm"
    >
      <form
        onSubmit={submit}
        className="panel w-full max-w-md shadow-[0_20px_60px_rgba(10,18,48,0.18)]"
      >
        <div className="panel-body space-y-5">
          <div className="flex flex-col items-center gap-3 text-center">
            <Logo size={44} />
            <span className="eyebrow">{t("setup.eyebrow", { defaultValue: "Setup" })}</span>
            <div className="space-y-1">
              <h2 id="setup-title" className="text-[22px] font-bold leading-[1.15] tracking-[-0.02em] text-ink">
                {t("setup.title", { defaultValue: "First-launch setup" })}
              </h2>
              <p className="text-[13px] text-ink-3">
                {t("setup.subtitle", {
                  defaultValue: "Enter the URL of your sync server. You can change this later from settings.",
                })}
              </p>
            </div>
          </div>
          <label className="block">
            <span className="field-label">
              {t("setup.url_label", { defaultValue: "Sync server URL" })}
            </span>
            <input
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://sync.example.com"
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
          <button
            type="submit"
            disabled={submitting}
            className="btn btn-primary w-full"
          >
            {submitting
              ? t("setup.saving", { defaultValue: "Saving..." })
              : t("setup.save", { defaultValue: "Save" })}
          </button>
        </div>
      </form>
    </div>
  )
}
