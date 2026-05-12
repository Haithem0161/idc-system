import { useTranslation } from "react-i18next"
import { AlertTriangle } from "lucide-react"

import { useShiftOverlaps } from "@/features/shifts/queries"

interface Props {
  onResolve: (operatorId: string) => void
}

export function OpenShiftConflictBanner ({ onResolve }: Props) {
  const { t } = useTranslation()
  const overlaps = useShiftOverlaps()
  const pairs = overlaps.data ?? []
  if (pairs.length === 0) return null

  const operatorIds = Array.from(
    new Set(pairs.map((p) => p.left.operator_id))
  )

  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-md border border-crimson/30 bg-crimson-soft px-4 py-3 text-[12px] text-crimson-dark"
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.8} />
      <div className="flex-1">
        <p className="font-medium">
          {t("reception.shifts.overlap.title", {
            defaultValue: "Overlapping shifts detected",
            count: pairs.length,
          })}
        </p>
        <p className="mt-1 text-ink-3">
          {t("reception.shifts.overlap.body", {
            defaultValue:
              "Two devices recorded overlapping shifts for {{count}} operator(s). Resolve before locking visits.",
            count: operatorIds.length,
          })}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {operatorIds.map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => onResolve(id)}
            className="btn btn-danger btn-sm"
          >
            {t("reception.shifts.overlap.resolve", { defaultValue: "Resolve" })}
          </button>
        ))}
      </div>
    </div>
  )
}
