import { useState } from "react"
import { useTranslation } from "react-i18next"
import { X } from "lucide-react"

import { useShiftEdit, useShiftHistoryToday } from "@/features/shifts/queries"
import type { ShiftWithMetaRecord } from "@/lib/ipc"
import { ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

interface Props {
  shiftId: string | null
  onClose: () => void
}

function toLocalInput (iso: string): string {
  // <input type="datetime-local"> wants `YYYY-MM-DDTHH:mm` in the user's
  // local zone, without seconds or offset.
  const d = new Date(iso)
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function fromLocalInput (value: string): string {
  if (!value) return ""
  return new Date(value).toISOString()
}

/**
 * Outer wrapper resolves the shift from `useShiftHistoryToday` and unmounts
 * the body when the id changes so the form state initialises cleanly from
 * the new shift without an effect-driven reset.
 */
export function RetroactiveShiftEditor ({ shiftId, onClose }: Props) {
  const history = useShiftHistoryToday()
  if (!shiftId) return null
  const shift = (history.data ?? []).find((s) => s.id === shiftId) ?? null
  if (!shift) return null
  return <Body shift={shift} onClose={onClose} />
}

function Body ({ shift, onClose }: { shift: ShiftWithMetaRecord; onClose: () => void }) {
  const { t } = useTranslation()
  const edit = useShiftEdit()
  const [checkIn, setCheckIn] = useState<string>(toLocalInput(shift.check_in_at))
  const [checkOut, setCheckOut] = useState<string>(
    shift.check_out_at ? toLocalInput(shift.check_out_at) : ""
  )
  const [note, setNote] = useState<string>(shift.note ?? "")
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    const inIso = fromLocalInput(checkIn)
    const outIso = checkOut ? fromLocalInput(checkOut) : null
    try {
      await edit.mutateAsync({
        shift_id: shift.id,
        check_in_at: inIso,
        check_out_at: outIso,
        note: { value: note.trim() ? note.trim() : null },
      })
      onClose()
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-40 flex items-center justify-center bg-ink/40 p-6"
    >
      <form onSubmit={submit} className="panel w-full max-w-md bg-surface">
        <div className="panel-head flex items-center justify-between">
          <span className="panel-title">
            {t("reception.shifts.edit.title", { defaultValue: "Edit shift" })}
          </span>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("admin.cancel", { defaultValue: "Cancel" })}
            className="flex h-7 w-7 items-center justify-center rounded text-ink-3 hover:bg-paper-2 hover:text-ink"
          >
            <X className="h-3.5 w-3.5" strokeWidth={1.8} />
          </button>
        </div>
        <div className="panel-body space-y-4">
          <FieldLabel label={t("reception.shifts.check_in", { defaultValue: "In" })}>
            <input
              type="datetime-local"
              value={checkIn}
              onChange={(e) => setCheckIn(e.target.value)}
              required
              className="input font-mono"
            />
          </FieldLabel>
          <FieldLabel label={t("reception.shifts.check_out", { defaultValue: "Out" })}>
            <input
              type="datetime-local"
              value={checkOut}
              onChange={(e) => setCheckOut(e.target.value)}
              className="input font-mono"
            />
            <p className="mt-1 text-[11px] text-ink-3">
              {t("reception.shifts.edit.out_hint", {
                defaultValue: "Leave blank to keep the shift open.",
              })}
            </p>
          </FieldLabel>
          <FieldLabel label={t("reception.shifts.clock_in.note", { defaultValue: "Note (optional)" })}>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              maxLength={1024}
              className="input"
            />
          </FieldLabel>
          <ErrorBanner message={error} />
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} className="btn btn-ghost btn-sm">
              {t("admin.cancel", { defaultValue: "Cancel" })}
            </button>
            <button type="submit" disabled={edit.isPending} className="btn btn-primary btn-sm">
              {t("admin.save", { defaultValue: "Save" })}
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
