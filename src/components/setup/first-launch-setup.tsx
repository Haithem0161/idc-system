import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { invoke, isTauri } from "@/lib/ipc"
import { Logo } from "@/components/shell/logo"

/**
 * Phase-1 first-launch modal (phase-01 §7.22).
 *
 * Prompts for the sync server URL and writes it via the Tauri config IPC.
 * If the URL is already set (or the env override is present) the modal stays
 * closed.
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
    invoke("config_get_sync_server_url")
      .then((existing) => {
        if (cancelled) return
        setOpen(!existing || existing.trim().length === 0)
      })
      .catch(() => {
        if (cancelled) return
        setOpen(true)
      })
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
      className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm"
    >
      <form
        onSubmit={submit}
        className="w-full max-w-md space-y-4 rounded-lg border border-border bg-card p-6 shadow-lg"
      >
        <div className="flex flex-col items-center gap-3 text-center">
          <Logo size={56} />
          <div className="space-y-1">
            <h2 id="setup-title" className="text-lg font-semibold">
              {t("setup.title", { defaultValue: "First-launch setup" })}
            </h2>
            <p className="text-sm text-muted-foreground">
              {t("setup.subtitle", {
                defaultValue:
                  "Enter the URL of your sync server. You can change this later from settings.",
              })}
            </p>
          </div>
        </div>
        <label className="block space-y-2">
          <span className="text-sm font-medium">
            {t("setup.url_label", { defaultValue: "Sync server URL" })}
          </span>
          <input
            type="url"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://sync.example.com"
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
            required
            autoFocus
          />
        </label>
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <button
          type="submit"
          disabled={submitting}
          className="w-full rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        >
          {submitting
            ? t("setup.saving", { defaultValue: "Saving..." })
            : t("setup.save", { defaultValue: "Save" })}
        </button>
      </form>
    </div>
  )
}
