import { useTranslation } from "react-i18next"

import type { QualifiedOperatorRecord } from "@/lib/ipc"

interface OperatorPickerDialogProps {
  open: boolean
  operators: QualifiedOperatorRecord[] | undefined
  onPick: (operatorId: string) => void
  onClose: () => void
  /** Disable confirm buttons while the lock IPC is in-flight. */
  busy?: boolean
}

/**
 * Modal that lists qualified, on-shift operators and lets the receptionist
 * pick one to finish the visit. Shared by the tabbed editor and any other
 * surface that needs to drive a `visits_lock` call (today: just the editor).
 */
export function OperatorPickerDialog ({
  open,
  operators,
  onPick,
  onClose,
  busy,
}: OperatorPickerDialogProps) {
  const { t } = useTranslation(["reception"])
  if (!open) return null
  const list = operators ?? []
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 p-4">
      <div className="panel w-full max-w-md">
        <div className="panel-head flex items-center justify-between">
          <span className="panel-title">
            {t("reception.new_visit.operator_picker.title")}
          </span>
          <button
            type="button"
            className="text-ink-3 hover:text-ink"
            onClick={onClose}
            aria-label={t("common.cancel", { defaultValue: "Close" })}
          >
            ×
          </button>
        </div>
        <div className="panel-body space-y-3">
          <p className="text-[12px] text-ink-3">
            {t("reception.new_visit.operator_picker.subtitle")}
          </p>
          {list.length === 0 ? (
            <p className="text-[13px] text-crimson">
              {t("reception.new_visit.operator_picker.no_qualified")}
            </p>
          ) : (
            <ul className="divide-y divide-line">
              {list.map((op) => (
                <li
                  key={op.id}
                  className="flex items-center justify-between py-2"
                >
                  <span className="text-[13px] text-ink-2">{op.name}</span>
                  <button
                    type="button"
                    className="btn btn-ink btn-sm"
                    disabled={busy}
                    onClick={() => onPick(op.id)}
                  >
                    {t("reception.new_visit.operator_picker.confirm")}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  )
}
