import { useDeferredValue, useId, useRef, useState } from "react"
import { Plus, User } from "lucide-react"

import { usePatientSearch } from "@/features/visits/queries"
import type { PatientRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

interface Props {
  /** The current free-text name value (controlled by the parent form). */
  value: string
  /** True once a concrete patient row is bound (suppresses the create row). */
  hasSelection: boolean
  placeholder: string
  /** Hint shown under the field (e.g. "Press Enter to add a new patient."). */
  hint: string
  /** Label for the inline create row ("Add {{name}}"). */
  createLabel: (name: string) => string
  /** Label when the dropdown has no matches. */
  noResultsLabel: string
  /** Typing: invalidates any committed patient and updates the name. */
  onType: (name: string) => void
  /** A concrete suggestion was chosen: bind this patient id + name. */
  onSelectPatient: (patient: PatientRecord) => void
  /** Commit the raw name (create-or-resolve) -- on Enter/blur with no pick. */
  onCommit: (name: string) => void
}

/**
 * Accessible type-ahead for selecting or creating a patient in the new-visit
 * flow (replaces the native <datalist>, which gave no keyboard navigation, no
 * rich rows, and no exact patient-id binding -- so two patients with the same
 * name were indistinguishable).
 *
 * NAME-ONLY: this captures just the patient name. Demographics are edited from
 * the Patients archive, never forced at visit creation.
 *
 * Contract preserved from the datalist: typing calls `onType` (which clears the
 * bound id), selecting a row calls `onSelectPatient` (binds the exact id),
 * Enter/blur with no active selection calls `onCommit` (create-or-resolve).
 * Built with no extra deps; combobox a11y per WAI-ARIA (listbox + activedescendant).
 */
export function PatientCombobox ({
  value,
  hasSelection,
  placeholder,
  hint,
  createLabel,
  noResultsLabel,
  onType,
  onSelectPatient,
  onCommit,
}: Props) {
  const listId = useId()
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const blurTimer = useRef<number | null>(null)

  const deferred = useDeferredValue(value)
  const { data: matches } = usePatientSearch(deferred)
  const options = matches ?? []
  const trimmed = value.trim()

  // The create row appears when there's text, no exact-name match, and the
  // current value isn't already a bound selection.
  const exactMatch = options.some(
    (p) => p.name.trim().toLowerCase() === trimmed.toLowerCase()
  )
  const showCreate = trimmed.length > 0 && !exactMatch && !hasSelection
  // Total selectable rows: the option list plus an optional trailing create row.
  const rowCount = options.length + (showCreate ? 1 : 0)
  const createIndex = showCreate ? options.length : -1

  const openIfContent = () => {
    if (trimmed.length > 0) setOpen(true)
  }

  const commitActive = () => {
    if (open && active < options.length && options[active]) {
      onSelectPatient(options[active])
      setOpen(false)
      return
    }
    // Active create row, or no list -> commit the raw name (create-or-resolve).
    onCommit(value)
    setOpen(false)
  }

  return (
    <div className="relative">
      <input
        className="input"
        placeholder={placeholder}
        value={value}
        role="combobox"
        aria-expanded={open && rowCount > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={
          open && rowCount > 0 ? `${listId}-opt-${active}` : undefined
        }
        autoComplete="off"
        data-testid="patient-input"
        onChange={(e) => {
          onType(e.target.value)
          setActive(0)
          setOpen(e.target.value.trim().length > 0)
        }}
        onFocus={openIfContent}
        onBlur={() => {
          // Defer so a click on a row registers before the list unmounts.
          blurTimer.current = window.setTimeout(() => {
            setOpen(false)
            const v = value.trim()
            if (v.length > 0 && !hasSelection) onCommit(v)
          }, 120)
        }}
        onKeyDown={(e) => {
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

      {open && rowCount > 0 ? (
        <ul
          id={listId}
          role="listbox"
          className="absolute z-30 mt-1 max-h-64 w-full overflow-y-auto rounded-md border border-line-2 bg-surface py-1 shadow-[0_4px_16px_rgba(10,18,48,0.08)]"
        >
          {options.map((p, i) => (
            <li
              key={p.id}
              id={`${listId}-opt-${i}`}
              role="option"
              aria-selected={i === active}
              // onMouseDown (not onClick) so it fires before the input blur.
              onMouseDown={(e) => {
                e.preventDefault()
                if (blurTimer.current) window.clearTimeout(blurTimer.current)
                onSelectPatient(p)
                setOpen(false)
              }}
              onMouseEnter={() => setActive(i)}
              className={cn(
                "flex cursor-pointer items-center gap-2.5 px-3 py-2 text-[13px]",
                i === active ? "bg-paper-2 text-ink" : "text-ink-2"
              )}
            >
              <User className="h-3.5 w-3.5 shrink-0 text-ink-4" strokeWidth={1.8} />
              <span className="min-w-0 flex-1 truncate font-medium">{p.name}</span>
              {p.phone ? (
                <span className="shrink-0 font-mono text-[11px] text-ink-4">
                  {p.phone}
                </span>
              ) : null}
            </li>
          ))}

          {showCreate ? (
            <li
              id={`${listId}-opt-${createIndex}`}
              role="option"
              aria-selected={active === createIndex}
              onMouseDown={(e) => {
                e.preventDefault()
                if (blurTimer.current) window.clearTimeout(blurTimer.current)
                onCommit(value)
                setOpen(false)
              }}
              onMouseEnter={() => setActive(createIndex)}
              className={cn(
                "flex cursor-pointer items-center gap-2.5 border-t border-line px-3 py-2 text-[13px]",
                active === createIndex ? "bg-crimson-soft text-crimson" : "text-crimson"
              )}
            >
              <Plus className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
              <span className="truncate font-medium">{createLabel(trimmed)}</span>
            </li>
          ) : null}

          {options.length === 0 && !showCreate ? (
            <li className="px-3 py-2 text-[12px] text-ink-3">{noResultsLabel}</li>
          ) : null}
        </ul>
      ) : null}

      <p className="mt-1 text-[11px] text-ink-3">{hint}</p>
    </div>
  )
}
