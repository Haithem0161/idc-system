import { useState } from "react"
import { useTranslation } from "react-i18next"
import { X } from "lucide-react"

import {
  useShiftClockOut,
  useShiftOverlaps,
  useShiftSoftDelete,
} from "@/features/shifts/queries"
import type { ShiftRecord } from "@/lib/ipc"
import { ErrorBanner } from "@/components/admin/admin-panel"
import { formatTime } from "@/lib/format/duration"

interface Props {
  operatorId: string | null
  onClose: () => void
}

export function ResolveOverlappingShifts ({ operatorId, onClose }: Props) {
  const { t } = useTranslation()
  const overlaps = useShiftOverlaps(operatorId ?? undefined)
  const clockOut = useShiftClockOut()
  const softDelete = useShiftSoftDelete()
  const [error, setError] = useState<string | null>(null)

  if (!operatorId) return null

  const shiftMap = new Map<string, ShiftRecord>()
  for (const p of overlaps.data ?? []) {
    shiftMap.set(p.left.id, p.left)
    shiftMap.set(p.right.id, p.right)
  }
  const shifts = [...shiftMap.values()].sort((a, b) =>
    a.check_in_at.localeCompare(b.check_in_at)
  )

  const close = async (id: string) => {
    setError(null)
    try {
      await clockOut.mutateAsync({ shift_id: id })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const remove = async (id: string) => {
    setError(null)
    try {
      await softDelete.mutateAsync({
        shift_id: id,
        reason: "overlap resolution",
      })
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
      <div className="panel w-full max-w-lg bg-surface">
        <div className="panel-head flex items-center justify-between">
          <span className="panel-title">
            {t("reception.shifts.overlap.title", { defaultValue: "Resolve overlap" })}
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
        <div className="panel-body space-y-3">
          <p className="text-[12px] text-ink-3">
            {t("reception.shifts.overlap.modal_hint", {
              defaultValue:
                "Pick the canonical shift; close or delete the duplicates.",
            })}
          </p>
          {shifts.length === 0 ? (
            <p className="rounded-md border border-line bg-paper-2 p-3 text-[12px] text-ink-3">
              {t("reception.shifts.overlap.cleared", {
                defaultValue: "Overlaps cleared.",
              })}
            </p>
          ) : (
            <ul className="space-y-2">
              {shifts.map((s) => (
                <li
                  key={s.id}
                  className="flex items-center justify-between gap-2 rounded-md border border-line bg-paper-2 p-3 text-[12px]"
                >
                  <div className="font-mono">
                    <div>
                      <span className="text-ink-3">in </span>
                      {formatTime(s.check_in_at)}
                      <span className="text-ink-3"> / out </span>
                      {s.check_out_at ? formatTime(s.check_out_at) : "—"}
                    </div>
                    <div className="text-[11px] text-ink-3">{s.id.slice(0, 8)}</div>
                  </div>
                  <div className="flex items-center gap-2">
                    {s.check_out_at == null ? (
                      <button
                        type="button"
                        onClick={() => close(s.id)}
                        disabled={clockOut.isPending}
                        className="btn btn-ghost btn-sm"
                      >
                        {t("reception.shifts.overlap.close_now", {
                          defaultValue: "Close now",
                        })}
                      </button>
                    ) : null}
                    <button
                      type="button"
                      onClick={() => remove(s.id)}
                      disabled={softDelete.isPending}
                      className="btn btn-danger btn-sm"
                    >
                      {t("reception.shifts.overlap.delete", { defaultValue: "Delete" })}
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          <ErrorBanner message={error} />
          <div className="flex justify-end">
            <button type="button" onClick={onClose} className="btn btn-ghost btn-sm">
              {t("admin.close", { defaultValue: "Close" })}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
