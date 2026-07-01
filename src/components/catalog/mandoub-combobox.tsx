import { useEffect, useId, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { Check, Contact, Plus, X } from "lucide-react"

import { useMandoubCreate, useMandoubs } from "@/features/catalog/queries"
import { useDebouncedValue } from "@/hooks/use-debounced-value"
import type { MandoubRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

interface Props {
  /** The bound mandoub id, or null for "None" (no referring representative). */
  value: string | null
  /** Bind a mandoub id (or null for None). */
  onChange: (mandoubId: string | null) => void
}

/**
 * Accessible type-ahead for selecting or creating a referring representative
 * (مندوب / mandoub) in the new-visit flow. Mirrors the doctor combobox but is
 * simpler: a mandoub carries no cut of its own (the 500/1000 cut is chosen
 * separately per-visit), and there is no "dalal" substitute row. A "None" row
 * pins at the top so the field can always be cleared.
 *
 * The mandoub is only ever surfaced alongside a real referring doctor; the
 * parent form owns that gating and auto-clears the bound value when the doctor
 * is removed or switched to house/dalal.
 */
export function MandoubCombobox ({ value, onChange }: Props) {
  const { t } = useTranslation()
  const listId = useId()
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [text, setText] = useState("")
  const [creating, setCreating] = useState(false)
  const blurTimer = useRef<number | null>(null)
  const rootRef = useRef<HTMLDivElement | null>(null)

  const debounced = useDebouncedValue(text, 150)
  // FTS-backed list filtered by the typed query; active representatives only.
  const { data: searchResults } = useMandoubs({
    include_inactive: false,
    query: debounced.trim().length >= 2 ? debounced.trim() : undefined,
  })
  // Full active list so we can always resolve the bound representative's display
  // name even when the search query has narrowed the result set.
  const { data: allActive } = useMandoubs({ include_inactive: false })

  const selected = useMemo(
    () => (value ? (allActive ?? []).find((m) => m.id === value) ?? null : null),
    [allActive, value],
  )

  const options = searchResults ?? []
  const trimmed = text.trim()

  const exactMatch = options.some(
    (m) => m.name.trim().toLowerCase() === trimmed.toLowerCase(),
  )
  const showCreate = trimmed.length > 0 && !exactMatch
  // Selectable rows: [None, ...mandoubs, (create)].
  const rowCount = 1 + options.length + (showCreate ? 1 : 0)
  const noneIndex = 0
  const firstMandoubIndex = 1
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

  const displayValue = open
    ? text
    : (selected?.name ?? t("reception.new_visit.mandoub_none"))

  const selectMandoub = (m: MandoubRecord) => {
    onChange(m.id)
    setText("")
    setOpen(false)
    setCreating(false)
  }
  const selectNone = () => {
    onChange(null)
    setText("")
    setOpen(false)
    setCreating(false)
  }

  const commitActive = () => {
    if (active === noneIndex) {
      selectNone()
      return
    }
    const optIdx = active - firstMandoubIndex
    const picked = optIdx >= 0 ? options[optIdx] : undefined
    if (picked) {
      selectMandoub(picked)
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
        placeholder={t("reception.new_visit.mandoub_placeholder")}
        value={displayValue}
        role="combobox"
        aria-expanded={open && rowCount > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && rowCount > 0 ? `${listId}-opt-${active}` : undefined}
        autoComplete="off"
        data-testid="mandoub-input"
        onChange={(e) => {
          setText(e.target.value)
          setActive(e.target.value.trim().length > 0 ? firstMandoubIndex : noneIndex)
          setOpen(true)
          setCreating(false)
        }}
        onFocus={() => {
          setText("")
          setOpen(true)
          setActive(noneIndex)
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
            id={`${listId}-opt-${noneIndex}`}
            role="option"
            aria-selected={active === noneIndex}
            onMouseDown={(e) => {
              e.preventDefault()
              if (blurTimer.current) window.clearTimeout(blurTimer.current)
              selectNone()
            }}
            onMouseEnter={() => setActive(noneIndex)}
            className={cn(
              "flex cursor-pointer items-center gap-2.5 px-3 py-2 text-[13px]",
              active === noneIndex ? "bg-paper-2 text-ink" : "text-ink-2",
            )}
          >
            <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-paper-2 text-[10px] font-semibold text-ink-3">
              {value === null ? <Check className="h-3 w-3" strokeWidth={2.5} /> : null}
            </span>
            <span className="min-w-0 flex-1 truncate font-medium">
              {t("reception.new_visit.mandoub_none")}
            </span>
            <span className="shrink-0 text-[11px] text-ink-4">
              {t("reception.new_visit.mandoub_none_hint")}
            </span>
          </li>

          {options.map((m, i) => {
            const rowIndex = firstMandoubIndex + i
            return (
              <li
                key={m.id}
                id={`${listId}-opt-${rowIndex}`}
                role="option"
                aria-selected={rowIndex === active}
                onMouseDown={(e) => {
                  e.preventDefault()
                  if (blurTimer.current) window.clearTimeout(blurTimer.current)
                  selectMandoub(m)
                }}
                onMouseEnter={() => setActive(rowIndex)}
                className={cn(
                  "flex cursor-pointer items-center gap-2.5 px-3 py-2 text-[13px]",
                  rowIndex === active ? "bg-paper-2 text-ink" : "text-ink-2",
                )}
              >
                <Contact className="h-3.5 w-3.5 shrink-0 text-ink-4" strokeWidth={1.8} />
                <span className="min-w-0 flex-1 truncate font-medium">{m.name}</span>
                {m.phone ? (
                  <span className="shrink-0 truncate font-mono text-[11px] text-ink-3">{m.phone}</span>
                ) : null}
                {m.id === value ? (
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
                {t("reception.new_visit.mandoub_create", { name: trimmed })}
              </span>
            </li>
          ) : null}

          {options.length === 0 && !showCreate && trimmed.length > 0 ? (
            <li className="px-3 py-2 text-[12px] text-ink-3">
              {t("reception.new_visit.mandoub_no_results")}
            </li>
          ) : null}
        </ul>
      ) : null}

      {creating ? (
        <MandoubCreatePanel
          initialName={trimmed}
          onCancel={() => {
            setCreating(false)
            setOpen(false)
          }}
          onCreated={(m) => selectMandoub(m)}
        />
      ) : null}
    </div>
  )
}

interface CreatePanelProps {
  initialName: string
  onCancel: () => void
  onCreated: (mandoub: MandoubRecord) => void
}

function MandoubCreatePanel ({ initialName, onCancel, onCreated }: CreatePanelProps) {
  const { t } = useTranslation()
  const create = useMandoubCreate()
  const [name, setName] = useState(initialName)
  const [phone, setPhone] = useState("")
  const [notes, setNotes] = useState("")
  const [error, setError] = useState<string | null>(null)

  const canSubmit = name.trim().length > 0 && !create.isPending

  const submit = async () => {
    setError(null)
    if (!canSubmit) return
    try {
      const mandoub = await create.mutateAsync({
        name: name.trim(),
        phone: phone.trim() || null,
        notes: notes.trim() || null,
      })
      onCreated(mandoub)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className="absolute z-30 mt-1 w-full rounded-md border border-line-2 bg-surface p-3 shadow-[0_4px_16px_rgba(10,18,48,0.08)]">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
          {t("reception.new_visit.mandoub_create_title")}
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
            {t("reception.new_visit.mandoub_field_name")}
          </span>
          <input
            className="input"
            value={name}
            autoFocus
            onChange={(e) => setName(e.target.value)}
            data-testid="mandoub-create-name"
          />
        </label>

        <label className="grid gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
            {t("reception.new_visit.mandoub_field_phone")}
          </span>
          <input
            className="input"
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
            data-testid="mandoub-create-phone"
          />
        </label>

        <label className="grid gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
            {t("reception.new_visit.mandoub_field_notes")}
          </span>
          <input
            className="input"
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            data-testid="mandoub-create-notes"
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
            data-testid="mandoub-create-submit"
          >
            {t("reception.new_visit.mandoub_create_submit")}
          </button>
        </div>
      </div>
    </div>
  )
}
