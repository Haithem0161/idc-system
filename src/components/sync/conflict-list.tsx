import { useTranslation } from "react-i18next"

import type { Conflict } from "@/lib/schemas/sync"
import { cn } from "@/lib/utils"

interface Props {
  conflicts: Conflict[]
  selectedOpId: string | null
  onSelect: (c: Conflict) => void
}

/**
 * List of unresolved conflicts (phase-08 §3 Frontend). One row per parked
 * conflict; clicking selects it for the right-side resolver panel.
 */
export function ConflictList({ conflicts, selectedOpId, onSelect }: Props) {
  const { t } = useTranslation()
  if (conflicts.length === 0) {
    return (
      <div className="rounded-md border border-line bg-surface px-6 py-12 text-center text-[13px] text-ink-3">
        {t("sync_conflicts.empty", {
          defaultValue: "No unresolved conflicts. The queue is clear.",
        })}
      </div>
    )
  }
  return (
    <ul className="divide-y divide-line rounded-md border border-line bg-surface">
      {conflicts.map((c) => {
        const isSelected = selectedOpId === c.opId
        return (
          <li key={c.opId}>
            <button
              type="button"
              onClick={() => onSelect(c)}
              aria-current={isSelected ? "true" : undefined}
              className={cn(
                "flex w-full items-start gap-3 px-4 py-3 text-start transition-colors hover:bg-paper-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20",
                isSelected && "bg-paper-2"
              )}
            >
              <span
                className={cn(
                  "mt-1 h-2 w-2 shrink-0 rounded-full",
                  c.reason.includes("version") ? "bg-gold" : "bg-crimson"
                )}
                aria-hidden
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-[13px] font-semibold text-ink">
                    {t(`audit.entities.${c.entity}`, { defaultValue: c.entity })}
                  </span>
                  <span className="font-mono text-[11px] text-ink-3">
                    {c.entityId.slice(0, 8)}
                  </span>
                </div>
                <p className="mt-0.5 truncate text-[11px] text-ink-3">
                  {t(`sync_conflicts.reason.${c.reason}`, {
                    defaultValue: c.reason,
                  })}
                </p>
                <p className="mt-0.5 truncate font-mono text-[10px] text-ink-4">
                  {c.opId}
                </p>
              </div>
            </button>
          </li>
        )
      })}
    </ul>
  )
}
