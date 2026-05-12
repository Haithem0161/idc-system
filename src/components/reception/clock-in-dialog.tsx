import { useState } from "react"
import { useTranslation } from "react-i18next"
import { X } from "lucide-react"

import { useOperators } from "@/features/catalog/queries"
import { useOpenShifts, useShiftClockIn } from "@/features/shifts/queries"
import { ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"

interface Props {
  open: boolean
  onClose: () => void
}

/**
 * Dialog wrapper unmounts the body when closed so local form state is reset
 * implicitly on each open. This sidesteps `react-hooks/set-state-in-effect`
 * and keeps the open/close lifecycle aligned with React's mount cycle.
 */
export function ClockInDialog ({ open, onClose }: Props) {
  if (!open) return null
  return <ClockInDialogBody onClose={onClose} />
}

function ClockInDialogBody ({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  const operators = useOperators({ include_inactive: false })
  const openShifts = useOpenShifts()
  const clockIn = useShiftClockIn()
  const [operatorId, setOperatorId] = useState<string>("")
  const [note, setNote] = useState<string>("")
  const [error, setError] = useState<string | null>(null)

  const openSet = new Set((openShifts.data ?? []).map((s) => s.operator_id))
  const candidates = (operators.data ?? []).filter((op) => !openSet.has(op.id))

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    if (!operatorId) {
      setError(t("reception.shifts.errors.choose_operator", { defaultValue: "Choose an operator" }))
      return
    }
    try {
      await clockIn.mutateAsync({ operator_id: operatorId, note: note.trim() || null })
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
            {t("reception.shifts.clock_in.title", { defaultValue: "Clock in operator" })}
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
          <FieldLabel label={t("reception.shifts.clock_in.operator", { defaultValue: "Operator" })}>
            <select
              value={operatorId}
              onChange={(e) => setOperatorId(e.target.value)}
              required
              className="input"
            >
              <option value="">
                {t("reception.shifts.clock_in.placeholder", { defaultValue: "Select an operator" })}
              </option>
              {candidates.map((op) => (
                <option key={op.id} value={op.id}>
                  {op.name}
                </option>
              ))}
            </select>
            {candidates.length === 0 ? (
              <p className="mt-1 text-[11px] text-ink-3">
                {t("reception.shifts.clock_in.all_on_shift", {
                  defaultValue: "All active operators are on shift.",
                })}
              </p>
            ) : null}
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
            <button
              type="submit"
              disabled={clockIn.isPending || !operatorId}
              className="btn btn-primary btn-sm"
            >
              {t("reception.shifts.clock_in.submit", { defaultValue: "Clock in" })}
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
