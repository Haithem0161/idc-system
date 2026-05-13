import { Cloud } from "lucide-react"
import { useTranslation } from "react-i18next"

import type { AuditQueryMode } from "@/lib/schemas/audit"

/**
 * Shown in the `<AuditTable>` header whenever a query crosses (or fully
 * predates) the 90-day local retention cliff -- phase-08 §3 Frontend +
 * §7.25. Mode `local` hides the badge.
 */
export function ServerBackedBadge({ mode }: { mode: AuditQueryMode }) {
  const { t } = useTranslation()
  if (mode === "local") return null
  return (
    <span
      className="status-pill is-info"
      role="status"
      aria-live="polite"
      title={t(`audit.server_backed.${mode}_tooltip`, {
        defaultValue:
          mode === "merged"
            ? "Range spans the 90-day local retention -- merging local + server results."
            : "Range predates the 90-day local retention -- showing server results.",
      })}
    >
      <Cloud className="h-3 w-3 -ms-1" strokeWidth={2} aria-hidden />
      <span>{t(`audit.server_backed.${mode}`, { defaultValue: mode })}</span>
    </span>
  )
}
