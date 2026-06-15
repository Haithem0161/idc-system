import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { Check } from "lucide-react"

import {
  getSettingByKey,
  settingValueAsBool,
  settingValueAsNumber,
  settingValueAsText,
  useSettings,
  useSettingUpdate,
} from "@/features/settings/queries"
import { useUpdater } from "@/features/updater/use-updater"
import type { SettingValueWire } from "@/lib/ipc"
import { cn } from "@/lib/utils"

// Every setting key the UI exposes, with its value type and a default. The
// default is what the seed migration writes under the 'unscoped' row; we carry
// it here so a tenant with NO row yet still renders an editable field
// pre-filled with the default, rather than a dead "not seeded" label. Saving
// creates the tenant's own row (upsert). `unit` is a small trailing hint
// (e.g. "IQD", "%", "min"); `min`/`max` bound numeric inputs.
type SettingType = "int" | "text" | "bool"

interface SettingSpec {
  type: SettingType
  default: string | number | boolean
  unit?: string
  min?: number
  max?: number
}

const SETTING_SPECS: Record<string, SettingSpec> = {
  clinic_display_name_ar: { type: "text", default: "" },
  clinic_display_name_en: { type: "text", default: "" },
  currency_symbol: { type: "text", default: "د.ع" },
  dye_cost_iqd: { type: "int", default: 10000, unit: "IQD", min: 0 },
  report_cost_iqd: { type: "int", default: 10000, unit: "IQD", min: 0 },
  internal_doctor_pct: { type: "int", default: 30, unit: "%", min: 0, max: 100 },
  idle_lock_minutes: { type: "int", default: 10, unit: "min", min: 1 },
  arabic_numerals: { type: "bool", default: false },
  thermal_width: { type: "int", default: 32, unit: "cols", min: 16 },
  thermal_printer_name: { type: "text", default: "" },
}

interface SettingGroup {
  key: string
  defaultTitle: string
  keys: readonly string[]
}

const GROUPS: SettingGroup[] = [
  {
    key: "identity",
    defaultTitle: "Clinic identity",
    keys: ["clinic_display_name_ar", "clinic_display_name_en", "currency_symbol"],
  },
  {
    key: "pricing",
    defaultTitle: "Pricing",
    keys: ["dye_cost_iqd", "report_cost_iqd", "internal_doctor_pct"],
  },
  {
    key: "security",
    defaultTitle: "Security",
    keys: ["idle_lock_minutes"],
  },
  {
    key: "display",
    defaultTitle: "Display",
    keys: ["arabic_numerals"],
  },
  {
    key: "printing",
    defaultTitle: "Receipt printing",
    keys: ["thermal_width", "thermal_printer_name"],
  },
]

export default function SettingsPage () {
  const { t } = useTranslation()
  const { data: settings, isLoading } = useSettings()
  const update = useSettingUpdate()
  const [error, setError] = useState<string | null>(null)
  const [savingKey, setSavingKey] = useState<string | null>(null)
  const [recentlySaved, setRecentlySaved] = useState<string | null>(null)

  useEffect(() => {
    if (!recentlySaved) return
    const id = window.setTimeout(() => setRecentlySaved(null), 1800)
    return () => window.clearTimeout(id)
  }, [recentlySaved])

  const save = async (key: string, value: SettingValueWire) => {
    setError(null)
    setSavingKey(key)
    try {
      await update.mutateAsync({ key, value })
      setRecentlySaved(key)
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    } finally {
      setSavingKey(null)
    }
  }

  if (isLoading) {
    return <p className="text-[13px] text-ink-3">{t("common.loading", { defaultValue: "Loading..." })}</p>
  }

  return (
    <div className="mx-auto max-w-3xl space-y-7">
      <header className="border-b border-line pb-5">
        <span className="eyebrow">{t("admin.eyebrow", { defaultValue: "Administration" })}</span>
        <h1 className="mt-2 text-[28px] font-bold leading-[1.05] tracking-[-0.024em] text-ink">
          {t("admin.settings.title", { defaultValue: "Settings" })}
        </h1>
        <p className="mt-1 text-[13px] text-ink-3">
          {t("admin.settings.subtitle", { defaultValue: "Configure clinic operations." })}
        </p>
      </header>

      {error ? (
        <div role="alert" className="status-pill is-danger w-fit">{error}</div>
      ) : null}

      {GROUPS.map((group) => (
        <section key={group.key} className="panel">
          <div className="panel-head">
            <span className="panel-title">
              {t(`admin.settings.group.${group.key}`, { defaultValue: group.defaultTitle })}
            </span>
          </div>
          <div className="divide-y divide-line">
            {group.keys.map((key) => {
              const setting = getSettingByKey(settings, key)
              return (
                <SettingRow
                  key={key}
                  keyName={key}
                  setting={setting}
                  busy={savingKey === key}
                  saved={recentlySaved === key}
                  onSave={save}
                />
              )
            })}
          </div>
        </section>
      ))}

      <UpdatesPanel />
    </div>
  )
}

function UpdatesPanel () {
  const { t } = useTranslation()
  const { state, runCheck, runInstall, canInstall } = useUpdater()
  const busy = state.status === "checking" || state.status === "installing"

  return (
    <section className="panel">
      <div className="panel-head">
        <span className="panel-title">
          {t("admin.settings.updates.title", { defaultValue: "App updates" })}
        </span>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-5 px-5 py-4">
        <div className="min-w-0 flex-1">
          <p className="text-[13px] text-ink-3">
            {t("admin.settings.updates.description", {
              defaultValue: "Check for a newer version of the desktop app.",
            })}
          </p>
          <p className="mt-1.5 text-[12px] text-ink" role="status" aria-live="polite">
            {state.status === "checking"
              ? t("admin.settings.updates.checking", { defaultValue: "Checking..." })
              : state.status === "current"
                ? t("admin.settings.updates.current", { defaultValue: "You are on the latest version." })
                : state.status === "unsupported"
                  ? t("admin.settings.updates.unsupported", {
                      defaultValue: "Updates are not available on this build.",
                    })
                  : state.status === "available"
                    ? t("admin.settings.updates.available", {
                        version: state.version,
                        defaultValue: "An update is available.",
                      })
                    : state.status === "installing"
                      ? t("admin.settings.updates.installing", {
                          version: state.version,
                          defaultValue: "Installing...",
                        })
                      : state.status === "error"
                        ? t("admin.settings.updates.error", {
                            defaultValue: "Could not check for updates.",
                          })
                        : null}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {state.status === "available" && canInstall ? (
            <button type="button" className="btn btn-primary btn-sm" onClick={() => void runInstall()} disabled={busy}>
              {t("admin.settings.updates.install", { defaultValue: "Download and restart" })}
            </button>
          ) : (
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => void runCheck()} disabled={busy}>
              {state.status === "checking"
                ? t("admin.settings.updates.checking", { defaultValue: "Checking..." })
                : t("admin.settings.updates.check", { defaultValue: "Check for updates" })}
            </button>
          )}
        </div>
      </div>
    </section>
  )
}

interface RowProps {
  keyName: string
  setting: ReturnType<typeof getSettingByKey>
  busy: boolean
  saved: boolean
  onSave: (key: string, value: SettingValueWire) => Promise<void>
}

function SettingRow ({ keyName, setting, busy, saved, onSave }: RowProps) {
  const { t } = useTranslation()
  const spec = SETTING_SPECS[keyName]
  // A row exists for this tenant once it has been saved at least once; until
  // then the field shows the default and is still fully editable.
  const isOverride = Boolean(setting)
  return (
    <div className="flex flex-wrap items-center justify-between gap-5 px-5 py-4">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-semibold text-ink">
          {t(`admin.settings.key.${keyName}`, { defaultValue: humanize(keyName) })}
        </div>
        <div className="mt-0.5 flex items-center gap-2 text-[11px] uppercase tracking-[0.06em] text-ink-3">
          <span className="font-mono normal-case tracking-normal">{keyName}</span>
          {spec ? <span className="text-ink-4">·</span> : null}
          {spec ? <span>{spec.type}</span> : null}
          {!isOverride ? (
            <>
              <span className="text-ink-4">·</span>
              <span className="text-ink-4">
                {t("admin.settings.default", { defaultValue: "default" })}
              </span>
            </>
          ) : null}
        </div>
      </div>
      <div className="flex items-center gap-2">
        {saved ? (
          <span className="status-pill is-success">
            <Check className="h-3 w-3" strokeWidth={2.4} aria-hidden />
            {t("admin.settings.saved", { defaultValue: "Saved" })}
          </span>
        ) : null}
        {spec ? (
          <SettingInput keyName={keyName} setting={setting} busy={busy} saved={saved} onSave={onSave} />
        ) : null}
      </div>
    </div>
  )
}

function SettingInput ({ keyName, setting, busy, onSave }: RowProps) {
  const { t } = useTranslation()
  const spec = SETTING_SPECS[keyName]!
  // Seed each control from the saved row when present, else from the spec
  // default -- so a tenant with no row yet still gets a pre-filled, editable
  // field instead of an empty/dead control.
  const [textValue, setTextValue] = useState(
    settingValueAsText(setting, spec.type === "text" ? String(spec.default) : "")
  )
  const [intValue, setIntValue] = useState(
    settingValueAsNumber(setting, spec.type === "int" ? Number(spec.default) : 0)
  )
  const [boolValue, setBoolValue] = useState(
    settingValueAsBool(setting, spec.type === "bool" ? Boolean(spec.default) : false)
  )

  if (spec.type === "bool") {
    return (
      <Toggle
        checked={boolValue}
        onChange={(next) => {
          setBoolValue(next)
          void onSave(keyName, { valueType: "bool", value: next })
        }}
      />
    )
  }
  if (spec.type === "int") {
    return (
      <div className="flex items-center gap-2">
        <input
          type="number"
          value={intValue}
          min={spec.min}
          max={spec.max}
          onChange={(e) => setIntValue(Number(e.target.value))}
          className="input input-sm w-28 font-mono"
        />
        {spec.unit ? (
          <span className="w-8 text-[11px] text-ink-3">{spec.unit}</span>
        ) : null}
        <button
          type="button"
          disabled={busy}
          onClick={() => onSave(keyName, { valueType: "int", value: intValue })}
          className="btn btn-ghost btn-sm"
        >
          {busy ? "..." : t("admin.settings.save", { defaultValue: "Save" })}
        </button>
      </div>
    )
  }
  return (
    <div className="flex items-center gap-2">
      <input
        type="text"
        value={textValue}
        onChange={(e) => setTextValue(e.target.value)}
        className="input input-sm w-56"
      />
      <button
        type="button"
        disabled={busy}
        onClick={() => onSave(keyName, { valueType: "text", value: textValue })}
        className="btn btn-ghost btn-sm"
      >
        {busy ? "..." : t("admin.settings.save", { defaultValue: "Save" })}
      </button>
    </div>
  )
}

function Toggle ({ checked, onChange }: { checked: boolean; onChange: (next: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-11 items-center rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20",
        checked ? "bg-ink border-ink" : "bg-paper-2 border-line-2"
      )}
    >
      <span
        className={cn(
          "inline-block h-4 w-4 transform rounded-full bg-white shadow-[0_1px_2px_rgba(10,18,48,0.2)] transition-transform",
          checked ? "translate-x-6 rtl:-translate-x-6" : "translate-x-1 rtl:-translate-x-1"
        )}
      />
    </button>
  )
}

function humanize (key: string): string {
  return key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())
}
