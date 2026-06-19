import { cn } from "@/lib/utils"

export interface StatItem {
  label: string
  /** Pre-formatted primary value (money, count, hours, …). */
  value: string
  /** Optional small unit suffix rendered muted after the value (e.g. "M"). */
  unit?: string
}

/**
 * The 4-up financial stat strip used at the top of every explorer detail pane.
 * Renders as a hairline-gap zebra plate (design-system §5.5 KPI tile); the
 * final tile flips to the dark ink scheme to anchor the focal metric (one
 * dark surface per pane, per the "one hero per screen" rule).
 */
export function StatStrip ({ items }: { items: StatItem[] }) {
  return (
    <div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
      {items.map((it, i) => {
        const isInk = i === items.length - 1
        return (
          <div
            key={it.label}
            className={cn("p-4", isInk ? "bg-ink" : "bg-surface")}
          >
            <div
              className={cn(
                "text-[10px] font-semibold uppercase tracking-[0.1em]",
                isInk ? "text-paper/60" : "text-ink-3"
              )}
            >
              {it.label}
            </div>
            <div
              className={cn(
                "mt-1.5 font-mono text-[23px] font-bold tracking-tight tabular-nums",
                isInk ? "text-paper" : "text-ink"
              )}
            >
              {it.value}
              {it.unit ? (
                <span
                  className={cn(
                    "ms-1 text-[12px] font-medium",
                    isInk ? "text-paper/50" : "text-ink-3"
                  )}
                >
                  {it.unit}
                </span>
              ) : null}
            </div>
          </div>
        )
      })}
    </div>
  )
}
