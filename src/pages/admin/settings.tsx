import { useCallback, useEffect, useId, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useBlocker } from "react-router"
import { Check, RotateCcw } from "lucide-react"

import {
  getSettingByKey,
  settingValueAsBool,
  settingValueAsNumber,
  settingValueAsText,
  useSettings,
  useSettingsUpdateBatch,
  useSyncServerUrl,
  useUpdateSyncServerUrl,
} from "@/features/settings/queries"
import { useUpdater } from "@/features/updater/use-updater"
import { formatIqd } from "@/lib/format/money"
import type { SettingRecord, SettingValueWire } from "@/lib/ipc"
import { cn } from "@/lib/utils"

// Every setting key the UI exposes, with its value type and a default. The
// default is what the seed migration writes under the 'unscoped' row; we carry
// it here so a tenant with NO row yet still renders an editable field
// pre-filled with the default, rather than a dead "not seeded" label. Saving
// creates the tenant's own row (upsert). `unit` is a small trailing hint
// (translated); `min`/`max` bound numeric inputs and are enforced before save
// (mirroring the Rust `validate_value_for_key`); `options` makes an int a
// discrete segmented choice (the backend rejects anything off the list).
type SettingType = "int" | "text" | "bool"

interface SettingSpec {
  type: SettingType
  default: string | number | boolean
  unit?: string
  min?: number
  max?: number
  /** Discrete allowed values -> rendered as a segmented control, not a field. */
  options?: number[]
  /** Render a live grouped-money preview beside the input (cost fields). */
  moneyPreview?: boolean
  /** Force a script direction on the text input (clinic name pair). */
  dir?: "rtl" | "ltr"
}

const SETTING_SPECS: Record<string, SettingSpec> = {
  clinic_display_name_ar: { type: "text", default: "", dir: "rtl" },
  clinic_display_name_en: { type: "text", default: "", dir: "ltr" },
  currency_symbol: { type: "text", default: "د.ع" },
  report_pct: { type: "int", default: 20, unit: "pct", min: 0, max: 100 },
  reporting_doctor_name: { type: "text", default: "" },
  internal_doctor_pct: { type: "int", default: 30, unit: "pct", min: 0, max: 100 },
  idle_lock_minutes: { type: "int", default: 10, unit: "min", min: 1 },
  arabic_numerals: { type: "bool", default: false },
  // The backend accepts ONLY 32 or 48 -- expose it as a discrete choice so an
  // invalid width is unrepresentable rather than a save-time error.
  thermal_width: { type: "int", default: 32, options: [32, 48] },
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
    keys: ["report_pct", "reporting_doctor_name", "internal_doctor_pct"],
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

// ---- draft model ----------------------------------------------------------

type DraftValue =
  | { type: "int"; value: number | "" }
  | { type: "text"; value: string }
  | { type: "bool"; value: boolean }

type Drafts = Record<string, DraftValue>

/** Seed a draft for one key from its saved row, else the spec default. */
function seedDraft (key: string, settings: SettingRecord[] | undefined): DraftValue {
  const spec = SETTING_SPECS[key]!
  const row = getSettingByKey(settings, key)
  if (spec.type === "bool") {
    return { type: "bool", value: settingValueAsBool(row, Boolean(spec.default)) }
  }
  if (spec.type === "int") {
    return { type: "int", value: settingValueAsNumber(row, Number(spec.default)) }
  }
  return { type: "text", value: settingValueAsText(row, String(spec.default)) }
}

function seedAllDrafts (settings: SettingRecord[] | undefined): Drafts {
  const out: Drafts = {}
  for (const key of Object.keys(SETTING_SPECS)) out[key] = seedDraft(key, settings)
  return out
}

/** Compare a draft to the saved/default baseline; true when it differs. */
function isDirty (key: string, draft: DraftValue, settings: SettingRecord[] | undefined): boolean {
  const baseline = seedDraft(key, settings)
  return draft.value !== baseline.value
}

/** Validate a single int draft against its spec; null when ok, else an error id. */
function intError (spec: SettingSpec, value: number | ""): string | null {
  if (value === "" || !Number.isInteger(value)) return "required"
  if (spec.min != null && value < spec.min) return "min"
  if (spec.max != null && value > spec.max) return "max"
  if (spec.options && !spec.options.includes(value)) return "options"
  return null
}

/** Convert a draft to the IPC wire shape for the batch save. */
function toWire (draft: DraftValue): SettingValueWire {
  if (draft.type === "bool") return { valueType: "bool", value: draft.value }
  if (draft.type === "int") return { valueType: "int", value: Number(draft.value) }
  return { valueType: "text", value: draft.value }
}

export default function SettingsPage () {
  const { t } = useTranslation()
  const { data: settings, isLoading } = useSettings()
  const batch = useSettingsUpdateBatch()
  const [drafts, setDrafts] = useState<Drafts>(() => seedAllDrafts(settings))
  const [error, setError] = useState<string | null>(null)
  const [savedFlash, setSavedFlash] = useState(false)

  // Reconcile drafts with server state whenever `settings` changes identity
  // (initial load, a successful save, or an external `settings:changed` pull).
  // Done during render via React's documented "adjust state when a prop
  // changes" pattern (previous value held in STATE, not a ref) so it never
  // clobbers an in-progress edit: only non-dirty keys are re-seeded.
  const [seenSettings, setSeenSettings] = useState<SettingRecord[] | undefined>(settings)
  if (settings && settings !== seenSettings) {
    setSeenSettings(settings)
    setDrafts((prev) => {
      let changed = false
      const next: Drafts = { ...prev }
      for (const key of Object.keys(SETTING_SPECS)) {
        if (!prev[key] || !isDirty(key, prev[key], settings)) {
          const seeded = seedDraft(key, settings)
          if (!prev[key] || prev[key].value !== seeded.value) {
            next[key] = seeded
            changed = true
          }
        }
      }
      return changed ? next : prev
    })
  }

  useEffect(() => {
    if (!savedFlash) return
    const id = window.setTimeout(() => setSavedFlash(false), 1800)
    return () => window.clearTimeout(id)
  }, [savedFlash])

  const dirtyKeys = useMemo(
    () => Object.keys(drafts).filter((k) => isDirty(k, drafts[k], settings)),
    [drafts, settings]
  )
  const errors = useMemo(() => {
    const out: Record<string, string> = {}
    for (const key of dirtyKeys) {
      const spec = SETTING_SPECS[key]!
      const d = drafts[key]
      if (spec.type === "int" && d.type === "int") {
        const e = intError(spec, d.value)
        if (e) out[key] = e
      }
    }
    return out
  }, [dirtyKeys, drafts])

  const hasErrors = Object.keys(errors).length > 0
  const canSave = dirtyKeys.length > 0 && !hasErrors && !batch.isPending

  // Warn before navigating away with unsaved edits (SPA navigations only).
  const blocker = useBlocker(
    useCallback(
      () => dirtyKeys.length > 0 && !batch.isPending,
      [dirtyKeys.length, batch.isPending]
    )
  )
  useEffect(() => {
    if (blocker.state !== "blocked") return
    const ok = window.confirm(
      t("admin.settings.unsaved_prompt", {
        defaultValue: "You have unsaved changes. Leave without saving?",
      })
    )
    if (ok) blocker.proceed()
    else blocker.reset()
  }, [blocker, t])

  const setDraft = useCallback((key: string, value: DraftValue) => {
    setDrafts((prev) => ({ ...prev, [key]: value }))
  }, [])

  const resetKey = useCallback((key: string) => {
    setDrafts((prev) => ({ ...prev, [key]: seedDraft(key, undefined) }))
  }, [])

  const discard = useCallback(() => {
    setError(null)
    setDrafts(seedAllDrafts(settings))
  }, [settings])

  const saveAll = useCallback(async () => {
    if (!canSave) return
    setError(null)
    try {
      const entries = dirtyKeys.map((key) => ({ key, value: toWire(drafts[key]) }))
      await batch.mutateAsync({ entries })
      setSavedFlash(true)
    } catch (err) {
      setError((err as { message?: string }).message ?? t("common.error", { defaultValue: "Something went wrong" }))
    }
  }, [canSave, dirtyKeys, drafts, batch, t])

  // Cmd/Ctrl+Enter saves from anywhere on the page.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault()
        void saveAll()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [saveAll])

  if (isLoading) {
    return <p className="text-[13px] text-ink-3">{t("common.loading", { defaultValue: "Loading..." })}</p>
  }

  return (
    <div className="mx-auto max-w-3xl space-y-7 pb-28">
      <header className="border-b border-line pb-5">
        <span className="eyebrow">{t("admin.eyebrow", { defaultValue: "Administration" })}</span>
        <h1 className="mt-2 text-[28px] font-bold leading-[1.05] tracking-[-0.024em] text-ink">
          {t("admin.settings.title", { defaultValue: "Settings" })}
        </h1>
        <p className="mt-1 text-[13px] text-ink-3">
          {t("admin.settings.subtitle", { defaultValue: "Configure clinic operations." })}
        </p>
      </header>

      <div>
        <span className="eyebrow">
          {t("admin.settings.zone.config", { defaultValue: "Clinic configuration" })}
        </span>
      </div>

      {GROUPS.map((group) => {
        const groupDesc = t(`admin.settings.group_desc.${group.key}`, { defaultValue: "" })
        return (
        <section key={group.key} className="panel">
          <div className={cn("panel-head", groupDesc && "flex-col items-start gap-0.5")}>
            <span className="panel-title">
              {t(`admin.settings.group.${group.key}`, { defaultValue: group.defaultTitle })}
            </span>
            {groupDesc ? (
              <span className="text-[12px] font-normal normal-case tracking-normal text-ink-3">
                {groupDesc}
              </span>
            ) : null}
          </div>
          <div className="divide-y divide-line">
            {group.keys.map((key) => (
              <SettingRow
                key={key}
                keyName={key}
                draft={drafts[key]}
                saved={getSettingByKey(settings, key)}
                dirty={isDirty(key, drafts[key], settings)}
                errorId={errors[key] ?? null}
                onChange={setDraft}
                onReset={resetKey}
              />
            ))}
          </div>
        </section>
        )
      })}

      <div className="pt-2">
        <span className="eyebrow">
          {t("admin.settings.zone.maintenance", { defaultValue: "Maintenance" })}
        </span>
      </div>
      <ConnectionPanel />
      <UpdatesPanel />

      <SaveBar
        dirtyCount={dirtyKeys.length}
        canSave={canSave}
        saving={batch.isPending}
        error={error}
        savedFlash={savedFlash}
        onSave={saveAll}
        onDiscard={discard}
      />
    </div>
  )
}

// ---- save action bar ------------------------------------------------------

function SaveBar ({
  dirtyCount,
  canSave,
  saving,
  error,
  savedFlash,
  onSave,
  onDiscard,
}: {
  dirtyCount: number
  canSave: boolean
  saving: boolean
  error: string | null
  savedFlash: boolean
  onSave: () => void
  onDiscard: () => void
}) {
  const { t } = useTranslation()
  const idle = dirtyCount === 0 && !error && !savedFlash
  if (idle) return null
  return (
    <div className="fixed inset-x-0 bottom-0 z-20 border-t border-line bg-paper/95 backdrop-blur-sm">
      <div className="mx-auto flex max-w-3xl flex-wrap items-center justify-between gap-3 px-6 py-3">
        <div className="flex items-center gap-3" role="status" aria-live="polite">
          {error ? (
            <span className="status-pill is-danger">{error}</span>
          ) : savedFlash ? (
            <span className="status-pill is-success">
              <Check className="h-3 w-3" strokeWidth={2.4} aria-hidden />
              {t("admin.settings.saved", { defaultValue: "Saved" })}
            </span>
          ) : (
            <span className="text-[12px] text-ink-3">
              {t("admin.settings.unsaved_count", {
                defaultValue: "{{count}} unsaved change",
                defaultValue_plural: "{{count}} unsaved changes",
                count: dirtyCount,
              })}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={onDiscard}
            disabled={saving || dirtyCount === 0}
          >
            {t("admin.settings.discard", { defaultValue: "Discard" })}
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={onSave}
            disabled={!canSave}
            aria-busy={saving}
          >
            {saving
              ? t("admin.settings.saving", { defaultValue: "Saving..." })
              : t("admin.settings.save_changes", { defaultValue: "Save changes" })}
          </button>
        </div>
      </div>
    </div>
  )
}

// ---- one setting row ------------------------------------------------------

interface RowProps {
  keyName: string
  draft: DraftValue
  saved: SettingRecord | undefined
  dirty: boolean
  errorId: string | null
  onChange: (key: string, value: DraftValue) => void
  onReset: (key: string) => void
}

function SettingRow ({ keyName, draft, saved, dirty, errorId, onChange, onReset }: RowProps) {
  const { t } = useTranslation()
  const spec = SETTING_SPECS[keyName]
  const inputId = useId()
  const descId = useId()
  const errId = useId()
  const isOverride = Boolean(saved)
  if (!spec) return null

  const description = t(`admin.settings.desc.${keyName}`, { defaultValue: "" })

  return (
    <div className="flex flex-wrap items-start justify-between gap-x-5 gap-y-2 px-5 py-4">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <label htmlFor={inputId} className="text-[13px] font-semibold text-ink">
            {t(`admin.settings.key.${keyName}`, { defaultValue: humanize(keyName) })}
          </label>
          {dirty ? (
            <span className="status-pill is-warn">
              {t("admin.settings.modified", { defaultValue: "Modified" })}
            </span>
          ) : !isOverride ? (
            <span className="status-pill is-info">
              {t("admin.settings.default", { defaultValue: "Default" })}
            </span>
          ) : null}
        </div>
        {description ? (
          <p id={descId} className="mt-1 max-w-prose text-[12px] leading-snug text-ink-3">
            {description}
          </p>
        ) : null}
        {errorId ? (
          <p id={errId} role="alert" className="mt-1 text-[12px] text-crimson">
            {validationMessage(t, keyName, spec, errorId)}
          </p>
        ) : null}
      </div>
      <div className="flex items-center gap-2 pt-0.5">
        <SettingControl
          keyName={keyName}
          spec={spec}
          draft={draft}
          inputId={inputId}
          describedBy={[description ? descId : null, errorId ? errId : null].filter(Boolean).join(" ") || undefined}
          invalid={Boolean(errorId)}
          onChange={onChange}
        />
        {isOverride && spec.type !== "bool" && !spec.options ? (
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            title={t("admin.settings.reset", { defaultValue: "Reset to default" })}
            aria-label={t("admin.settings.reset", { defaultValue: "Reset to default" })}
            onClick={() => onReset(keyName)}
          >
            <RotateCcw className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
          </button>
        ) : null}
      </div>
    </div>
  )
}

// ---- per-type controls ----------------------------------------------------

function SettingControl ({
  keyName,
  spec,
  draft,
  inputId,
  describedBy,
  invalid,
  onChange,
}: {
  keyName: string
  spec: SettingSpec
  draft: DraftValue
  inputId: string
  describedBy?: string
  invalid: boolean
  onChange: (key: string, value: DraftValue) => void
}) {
  const { t } = useTranslation()

  if (spec.type === "bool" && draft.type === "bool") {
    return (
      <Toggle
        id={inputId}
        labelKey={`admin.settings.key.${keyName}`}
        checked={draft.value}
        onChange={(next) => onChange(keyName, { type: "bool", value: next })}
      />
    )
  }

  if (spec.options && draft.type === "int") {
    return (
      <Segmented
        id={inputId}
        options={spec.options}
        value={typeof draft.value === "number" ? draft.value : spec.options[0]}
        labelFor={(opt) =>
          t(`admin.settings.option.${keyName}.${opt}`, { defaultValue: String(opt) })
        }
        onChange={(next) => onChange(keyName, { type: "int", value: next })}
      />
    )
  }

  if (spec.type === "int" && draft.type === "int") {
    return (
      <div className="flex items-center gap-2">
        {spec.moneyPreview ? <MoneyPreview value={draft.value} /> : null}
        <input
          id={inputId}
          type="number"
          inputMode="numeric"
          step={1}
          value={draft.value}
          min={spec.min}
          max={spec.max}
          aria-describedby={describedBy}
          aria-invalid={invalid || undefined}
          onChange={(e) => {
            const raw = e.target.value
            onChange(keyName, { type: "int", value: raw === "" ? "" : Number(raw) })
          }}
          className={cn("input input-sm w-28 font-mono", invalid && "border-crimson")}
        />
        {spec.unit ? <UnitLabel unit={spec.unit} /> : null}
      </div>
    )
  }

  if (draft.type === "text") {
    return (
      <input
        id={inputId}
        type="text"
        dir={spec.dir}
        lang={spec.dir === "rtl" ? "ar" : spec.dir === "ltr" ? "en" : undefined}
        value={draft.value}
        aria-describedby={describedBy}
        onChange={(e) => onChange(keyName, { type: "text", value: e.target.value })}
        className={cn("input input-sm", keyName === "currency_symbol" ? "w-24 text-center" : "w-56")}
      />
    )
  }
  return null
}

function MoneyPreview ({ value }: { value: number | "" }) {
  const n = typeof value === "number" && Number.isFinite(value) ? value : 0
  return (
    <span className="font-mono text-[12px] tabular-nums text-ink-3" aria-hidden>
      {formatIqd(n)}
    </span>
  )
}

function UnitLabel ({ unit }: { unit: string }) {
  const { t } = useTranslation()
  if (unit === "pct") return <span className="text-[12px] text-ink-3">%</span>
  return (
    <span className="text-[12px] text-ink-3">
      {t(`admin.settings.unit.${unit}`, { defaultValue: unit })}
    </span>
  )
}

function Segmented ({
  id,
  options,
  value,
  labelFor,
  onChange,
}: {
  id: string
  options: number[]
  value: number
  labelFor: (opt: number) => string
  onChange: (next: number) => void
}) {
  return (
    <div id={id} role="radiogroup" className="inline-flex gap-1 rounded-md border border-line bg-paper-2 p-0.5">
      {options.map((opt) => {
        const active = opt === value
        return (
          <button
            key={opt}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(opt)}
            className={cn(
              "rounded-[5px] px-3 py-1.5 text-[12px] font-semibold transition-colors duration-150",
              active
                ? "bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
                : "text-ink-3 hover:text-ink"
            )}
          >
            {labelFor(opt)}
          </button>
        )
      })}
    </div>
  )
}

function Toggle ({
  id,
  labelKey,
  checked,
  onChange,
}: {
  id: string
  labelKey: string
  checked: boolean
  onChange: (next: boolean) => void
}) {
  const { t } = useTranslation()
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={t(labelKey, { defaultValue: "" })}
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

// ---- connection (sync server) ---------------------------------------------

/** Client-side mirror of the Rust `validate_sync_server_url`: absolute http(s)
 *  URL with a host and no whitespace. Server re-validates regardless. */
function isValidSyncUrl (raw: string): boolean {
  const v = raw.trim()
  if (!v || /\s/.test(v)) return false
  const m = /^https?:\/\/([^/?#]+)/i.exec(v)
  return Boolean(m && m[1])
}

/**
 * Sync server URL editor (superadmin-only screen). Repointing the server is a
 * security-relevant change -- it decides which server the app trusts for auth
 * and where all data is pushed -- so it commits through the gated
 * `config_update_sync_server_url` command and re-pins the new server's RS256
 * key. Kept separate from the batched clinic settings: it is one field, has its
 * own confirm + warning, and a different commit path.
 */
function ConnectionPanel () {
  const { t } = useTranslation()
  const { data: current, isLoading } = useSyncServerUrl()
  const update = useUpdateSyncServerUrl()
  const inputId = useId()
  const hintId = useId()
  const [value, setValue] = useState("")
  const [confirming, setConfirming] = useState(false)
  const [saved, setSaved] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Seed the field from the persisted URL once it loads / changes (when not
  // mid-edit). `current` is null until first-launch setup has run.
  const [seen, setSeen] = useState<string | null | undefined>(undefined)
  if (current !== seen && !confirming) {
    setSeen(current)
    setValue(current ?? "")
  }

  useEffect(() => {
    if (!saved) return
    const id = window.setTimeout(() => setSaved(false), 2200)
    return () => window.clearTimeout(id)
  }, [saved])

  const trimmed = value.trim()
  const dirty = trimmed !== (current ?? "").trim()
  const valid = isValidSyncUrl(trimmed)

  const commit = async () => {
    setError(null)
    try {
      await update.mutateAsync(trimmed)
      setConfirming(false)
      setSaved(true)
    } catch (err) {
      setError((err as { message?: string }).message ?? t("common.error", { defaultValue: "Something went wrong" }))
    }
  }

  return (
    <section className="panel">
      <div className="panel-head flex-col items-start gap-0.5">
        <span className="panel-title">
          {t("admin.settings.connection.title", { defaultValue: "Sync server" })}
        </span>
        <span className="text-[12px] font-normal normal-case tracking-normal text-ink-3">
          {t("admin.settings.connection.desc", {
            defaultValue: "The server this device syncs and signs in against.",
          })}
        </span>
      </div>
      <div className="px-5 py-4">
        <label htmlFor={inputId} className="block text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
          {t("admin.settings.connection.url_label", { defaultValue: "Server URL" })}
        </label>
        <div className="mt-1.5 flex flex-wrap items-center gap-2">
          <input
            id={inputId}
            type="url"
            dir="ltr"
            inputMode="url"
            placeholder="https://idc-sync.example.com"
            value={value}
            disabled={isLoading || update.isPending}
            aria-describedby={hintId}
            aria-invalid={trimmed.length > 0 && !valid ? true : undefined}
            onChange={(e) => { setValue(e.target.value); setConfirming(false); setError(null) }}
            className={cn(
              "input input-sm w-full max-w-md font-mono",
              trimmed.length > 0 && !valid && "border-crimson"
            )}
          />
          {!confirming ? (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              disabled={!dirty || !valid || update.isPending}
              onClick={() => setConfirming(true)}
            >
              {t("admin.settings.connection.change", { defaultValue: "Change server" })}
            </button>
          ) : null}
        </div>

        <p id={hintId} className="mt-1.5 text-[12px] text-ink-3">
          {trimmed.length > 0 && !valid
            ? t("admin.settings.connection.invalid", { defaultValue: "Enter a full http:// or https:// address." })
            : t("admin.settings.connection.hint", {
                defaultValue: "Use the address your clinic was given. Changing it re-pins the new server's security key.",
              })}
        </p>

        {confirming ? (
          <div className="mt-3 rounded-md border border-crimson/30 bg-crimson-soft px-3.5 py-3">
            <p className="text-[12px] leading-snug text-ink-2">
              {t("admin.settings.connection.confirm_warning", {
                defaultValue:
                  "Point this device at a different sync server? It will trust that server for sign-in and send all data there. Only do this if you were told to.",
              })}
            </p>
            <div className="mt-2.5 flex items-center gap-2">
              <button type="button" className="btn btn-danger btn-sm" disabled={update.isPending} onClick={() => void commit()} aria-busy={update.isPending}>
                {update.isPending
                  ? t("admin.settings.connection.changing", { defaultValue: "Changing..." })
                  : t("admin.settings.connection.confirm", { defaultValue: "Yes, change server" })}
              </button>
              <button type="button" className="btn btn-ghost btn-sm" disabled={update.isPending} onClick={() => { setConfirming(false); setValue(current ?? "") }}>
                {t("admin.settings.discard", { defaultValue: "Discard" })}
              </button>
            </div>
          </div>
        ) : null}

        <div className="mt-2 min-h-[20px]" role="status" aria-live="polite">
          {error ? (
            <span className="status-pill is-danger">{error}</span>
          ) : saved ? (
            <span className="status-pill is-success">
              <Check className="h-3 w-3" strokeWidth={2.4} aria-hidden />
              {t("admin.settings.connection.saved", { defaultValue: "Server updated" })}
            </span>
          ) : null}
        </div>
      </div>
    </section>
  )
}

// ---- updates (maintenance) ------------------------------------------------

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

// ---- helpers --------------------------------------------------------------

function validationMessage (
  t: ReturnType<typeof useTranslation>["t"],
  keyName: string,
  spec: SettingSpec,
  errorId: string
): string {
  if (keyName === "internal_doctor_pct" || keyName === "report_pct") {
    return t("admin.settings.error.pct", { defaultValue: "Enter a whole number from 0 to 100." })
  }
  if (keyName === "idle_lock_minutes") {
    return t("admin.settings.error.idle", { defaultValue: "Enter at least 1 minute." })
  }
  if (errorId === "required") {
    return t("admin.settings.error.required", { defaultValue: "Enter a whole number." })
  }
  if (errorId === "min" && spec.min != null) {
    return t("admin.settings.error.min", { defaultValue: "Must be at least {{min}}.", min: spec.min })
  }
  if (errorId === "max" && spec.max != null) {
    return t("admin.settings.error.max", { defaultValue: "Must be at most {{max}}.", max: spec.max })
  }
  return t("admin.settings.error.required", { defaultValue: "Enter a whole number." })
}

function humanize (key: string): string {
  return key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())
}
