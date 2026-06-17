import { useEffect, useId, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { AlertTriangle, Check, Plus, Stethoscope, X } from "lucide-react"

import {
  useDoctorCreate,
  useDoctors,
  useDoctorsWithPhone,
} from "@/features/catalog/queries"
import { useDebouncedValue } from "@/hooks/use-debounced-value"
import type { DoctorCutKindLiteral, DoctorRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

interface Props {
  /** The bound doctor id, or null for the Internal (no referring doctor) row. */
  value: string | null
  /** Bind a doctor id (or null for Internal). */
  onChange: (doctorId: string | null) => void
}

/**
 * Accessible type-ahead for selecting or creating a referring doctor in the
 * new-visit flow (replaces the native <select>). Mirrors the patient combobox
 * but binds a doctor id, pins an "Internal" (no-doctor) row at the top, and
 * carries a richer inline-create panel because a doctor needs more than a name:
 * a default cut (which the money engine uses when no per-check pricing exists),
 * plus optional phone + specialty.
 *
 * Internal == the old "House": doctor_id is null and the clinic keeps the full
 * cut. Clearing the field returns to Internal.
 *
 * QoL: rich rows (name + specialty), recently-active doctors float to the top,
 * inactive doctors are hidden, a duplicate-name guard and a phone-uniqueness
 * warning fire on inline create.
 */
export function DoctorCombobox ({ value, onChange }: Props) {
  const { t } = useTranslation()
  const listId = useId()
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [text, setText] = useState("")
  const [creating, setCreating] = useState(false)
  const blurTimer = useRef<number | null>(null)
  const rootRef = useRef<HTMLDivElement | null>(null)

  const debounced = useDebouncedValue(text, 150)
  // FTS-backed list filtered by the typed query; active doctors only.
  const { data: searchResults } = useDoctors({
    include_inactive: false,
    query: debounced.trim().length >= 2 ? debounced.trim() : undefined,
  })
  // Full active list so we can always resolve the bound doctor's display name
  // even when the search query has narrowed the result set.
  const { data: allActive } = useDoctors({ include_inactive: false })

  const selected = useMemo(
    () => (value ? (allActive ?? []).find((d) => d.id === value) ?? null : null),
    [allActive, value],
  )

  const options = searchResults ?? []
  const trimmed = text.trim()

  // The Internal row is always the first selectable row.
  const exactMatch = options.some(
    (d) => d.name.trim().toLowerCase() === trimmed.toLowerCase(),
  )
  const showCreate = trimmed.length > 0 && !exactMatch
  // Selectable rows: [Internal, ...doctors, (create)].
  const rowCount = 1 + options.length + (showCreate ? 1 : 0)
  const internalIndex = 0
  const firstDoctorIndex = 1
  const createIndex = showCreate ? 1 + options.length : -1

  // Close on outside click.
  useEffect(() => {
    if (!open) return
    const onDocPointer = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false)
        setCreating(false)
      }
    }
    document.addEventListener("pointerdown", onDocPointer)
    return () => document.removeEventListener("pointerdown", onDocPointer)
  }, [open])

  const displayValue = open ? text : (selected?.name ?? t("reception.new_visit.internal"))

  const selectDoctor = (d: DoctorRecord) => {
    onChange(d.id)
    setText("")
    setOpen(false)
    setCreating(false)
  }
  const selectInternal = () => {
    onChange(null)
    setText("")
    setOpen(false)
    setCreating(false)
  }

  const commitActive = () => {
    if (active === internalIndex) {
      selectInternal()
      return
    }
    const optIdx = active - firstDoctorIndex
    const picked = optIdx >= 0 ? options[optIdx] : undefined
    if (picked) {
      selectDoctor(picked)
      return
    }
    if (active === createIndex) {
      setCreating(true)
    }
  }

  return (
    <div className="relative" ref={rootRef}>
      <input
        className="input"
        placeholder={t("reception.new_visit.doctor_search_placeholder")}
        value={displayValue}
        role="combobox"
        aria-expanded={open && rowCount > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && rowCount > 0 ? `${listId}-opt-${active}` : undefined}
        autoComplete="off"
        data-testid="doctor-input"
        onChange={(e) => {
          setText(e.target.value)
          setActive(e.target.value.trim().length > 0 ? firstDoctorIndex : internalIndex)
          setOpen(true)
          setCreating(false)
        }}
        onFocus={() => {
          setText("")
          setOpen(true)
          setActive(internalIndex)
        }}
        onBlur={() => {
          blurTimer.current = window.setTimeout(() => {
            if (!creating) setOpen(false)
          }, 150)
        }}
        onKeyDown={(e) => {
          if (creating) return
          if (e.key === "ArrowDown") {
            e.preventDefault()
            setOpen(true)
            setActive((i) => (rowCount === 0 ? 0 : (i + 1) % rowCount))
          } else if (e.key === "ArrowUp") {
            e.preventDefault()
            setActive((i) => (rowCount === 0 ? 0 : (i - 1 + rowCount) % rowCount))
          } else if (e.key === "Enter") {
            e.preventDefault()
            commitActive()
          } else if (e.key === "Escape") {
            setOpen(false)
          }
        }}
      />

      {open && !creating ? (
        <ul
          id={listId}
          role="listbox"
          className="absolute z-30 mt-1 max-h-72 w-full overflow-y-auto rounded-md border border-line-2 bg-surface py-1 shadow-[0_4px_16px_rgba(10,18,48,0.08)]"
        >
          <li
            id={`${listId}-opt-${internalIndex}`}
            role="option"
            aria-selected={active === internalIndex}
            onMouseDown={(e) => {
              e.preventDefault()
              if (blurTimer.current) window.clearTimeout(blurTimer.current)
              selectInternal()
            }}
            onMouseEnter={() => setActive(internalIndex)}
            className={cn(
              "flex cursor-pointer items-center gap-2.5 px-3 py-2 text-[13px]",
              active === internalIndex ? "bg-paper-2 text-ink" : "text-ink-2",
            )}
          >
            <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-paper-2 text-[10px] font-semibold text-ink-3">
              {value === null ? <Check className="h-3 w-3" strokeWidth={2.5} /> : null}
            </span>
            <span className="min-w-0 flex-1 truncate font-medium">
              {t("reception.new_visit.internal")}
            </span>
            <span className="shrink-0 text-[11px] text-ink-4">
              {t("reception.new_visit.internal_hint")}
            </span>
          </li>

          {options.map((d, i) => {
            const rowIndex = firstDoctorIndex + i
            return (
              <li
                key={d.id}
                id={`${listId}-opt-${rowIndex}`}
                role="option"
                aria-selected={rowIndex === active}
                onMouseDown={(e) => {
                  e.preventDefault()
                  if (blurTimer.current) window.clearTimeout(blurTimer.current)
                  selectDoctor(d)
                }}
                onMouseEnter={() => setActive(rowIndex)}
                className={cn(
                  "flex cursor-pointer items-center gap-2.5 px-3 py-2 text-[13px]",
                  rowIndex === active ? "bg-paper-2 text-ink" : "text-ink-2",
                )}
              >
                <Stethoscope className="h-3.5 w-3.5 shrink-0 text-ink-4" strokeWidth={1.8} />
                <span className="min-w-0 flex-1 truncate font-medium">{d.name}</span>
                {d.specialty ? (
                  <span className="shrink-0 truncate text-[11px] text-ink-3">{d.specialty}</span>
                ) : null}
                {d.id === value ? (
                  <Check className="h-3.5 w-3.5 shrink-0 text-success" strokeWidth={2.2} />
                ) : null}
              </li>
            )
          })}

          {showCreate ? (
            <li
              id={`${listId}-opt-${createIndex}`}
              role="option"
              aria-selected={active === createIndex}
              onMouseDown={(e) => {
                e.preventDefault()
                if (blurTimer.current) window.clearTimeout(blurTimer.current)
                setCreating(true)
              }}
              onMouseEnter={() => setActive(createIndex)}
              className={cn(
                "flex cursor-pointer items-center gap-2.5 border-t border-line px-3 py-2 text-[13px]",
                active === createIndex ? "bg-crimson-soft text-crimson" : "text-crimson",
              )}
            >
              <Plus className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
              <span className="truncate font-medium">
                {t("reception.new_visit.doctor_create", { name: trimmed })}
              </span>
            </li>
          ) : null}

          {options.length === 0 && !showCreate && trimmed.length > 0 ? (
            <li className="px-3 py-2 text-[12px] text-ink-3">
              {t("reception.new_visit.doctor_no_results")}
            </li>
          ) : null}
        </ul>
      ) : null}

      {creating ? (
        <DoctorCreatePanel
          initialName={trimmed}
          onCancel={() => {
            setCreating(false)
            setOpen(false)
          }}
          onCreated={(d) => selectDoctor(d)}
        />
      ) : null}
    </div>
  )
}

interface CreatePanelProps {
  initialName: string
  onCancel: () => void
  onCreated: (doctor: DoctorRecord) => void
}

function DoctorCreatePanel ({ initialName, onCancel, onCreated }: CreatePanelProps) {
  const { t } = useTranslation()
  const create = useDoctorCreate()
  const [name, setName] = useState(initialName)
  const [cutKind, setCutKind] = useState<DoctorCutKindLiteral>("pct")
  const [cutValue, setCutValue] = useState("")
  const [phone, setPhone] = useState("")
  const [specialty, setSpecialty] = useState("")
  const [error, setError] = useState<string | null>(null)

  // Duplicate-name guard: warn if an existing active doctor has the same name.
  const { data: nameMatches } = useDoctors({ include_inactive: true, query: name.trim() })
  const nameDup = (nameMatches ?? []).some(
    (d) => d.name.trim().toLowerCase() === name.trim().toLowerCase(),
  )
  // Phone-uniqueness warning.
  const { data: phoneMatches } = useDoctorsWithPhone(phone)
  const phoneDup = (phoneMatches ?? []).length > 0

  const cutNumber = Number(cutValue)
  const cutValid =
    cutValue.trim().length > 0 &&
    Number.isFinite(cutNumber) &&
    cutNumber >= 0 &&
    (cutKind === "fixed" || cutNumber <= 100)

  const canSubmit = name.trim().length > 0 && cutValid && !create.isPending

  const submit = async () => {
    setError(null)
    if (!canSubmit) return
    try {
      const doctor = await create.mutateAsync({
        name: name.trim(),
        specialty: specialty.trim() || null,
        phone: phone.trim() || null,
        default_cut_kind: cutKind,
        default_cut_value: Math.round(cutNumber),
      })
      onCreated(doctor)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className="absolute z-30 mt-1 w-full rounded-md border border-line-2 bg-surface p-3 shadow-[0_4px_16px_rgba(10,18,48,0.08)]">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
          {t("reception.new_visit.doctor_create_title")}
        </span>
        <button
          type="button"
          className="rounded p-1 text-ink-4 hover:bg-paper-2 hover:text-ink-2"
          onClick={onCancel}
          aria-label={t("common.cancel", { defaultValue: "Cancel" })}
        >
          <X className="h-3.5 w-3.5" strokeWidth={2} />
        </button>
      </div>

      <div className="grid gap-2">
        <label className="grid gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
            {t("reception.new_visit.doctor_field_name")}
          </span>
          <input
            className="input"
            value={name}
            autoFocus
            onChange={(e) => setName(e.target.value)}
            data-testid="doctor-create-name"
          />
        </label>
        {nameDup ? (
          <p className="flex items-center gap-1.5 text-[11px] text-gold">
            <AlertTriangle className="h-3 w-3 shrink-0" strokeWidth={2} />
            {t("reception.new_visit.doctor_name_dup")}
          </p>
        ) : null}

        <div className="grid grid-cols-[1fr_auto] gap-2">
          <label className="grid gap-1">
            <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
              {t("reception.new_visit.doctor_field_cut")}
            </span>
            <input
              className="input"
              inputMode="numeric"
              value={cutValue}
              placeholder={cutKind === "pct" ? "15" : "20000"}
              onChange={(e) => setCutValue(e.target.value.replace(/[^0-9]/g, ""))}
              data-testid="doctor-create-cut"
            />
          </label>
          <label className="grid gap-1">
            <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
              {t("reception.new_visit.doctor_field_cut_kind")}
            </span>
            <select
              className="input"
              value={cutKind}
              onChange={(e) => setCutKind(e.target.value as DoctorCutKindLiteral)}
            >
              <option value="pct">{t("reception.new_visit.doctor_cut_pct")}</option>
              <option value="fixed">{t("reception.new_visit.doctor_cut_fixed")}</option>
            </select>
          </label>
        </div>

        <label className="grid gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
            {t("reception.new_visit.doctor_field_phone")}
          </span>
          <input
            className="input"
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
            data-testid="doctor-create-phone"
          />
        </label>
        {phoneDup ? (
          <p className="flex items-center gap-1.5 text-[11px] text-gold">
            <AlertTriangle className="h-3 w-3 shrink-0" strokeWidth={2} />
            {t("reception.new_visit.doctor_phone_dup")}
          </p>
        ) : null}

        <label className="grid gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
            {t("reception.new_visit.doctor_field_specialty")}
          </span>
          <input
            className="input"
            value={specialty}
            onChange={(e) => setSpecialty(e.target.value)}
            data-testid="doctor-create-specialty"
          />
        </label>

        {error ? <p className="text-[11px] text-crimson">{error}</p> : null}

        <div className="mt-1 flex justify-end gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={onCancel}>
            {t("common.cancel", { defaultValue: "Cancel" })}
          </button>
          <button
            type="button"
            className="btn btn-ink btn-sm"
            disabled={!canSubmit}
            onClick={submit}
            data-testid="doctor-create-submit"
          >
            {t("reception.new_visit.doctor_create_submit")}
          </button>
        </div>
      </div>
    </div>
  )
}
