import type { ReactNode } from "react"
import { useNavigate } from "react-router"
import { ChevronRight } from "lucide-react"

import { cn } from "@/lib/utils"

export interface LeaderboardRow {
  id: string
  name: string
  sub: string
  primary: string
  secondary: string
  href: string
  house?: boolean
}

/**
 * A top-N leaderboard card used on the dashboard for doctors, operators, and
 * check types. The header links to the full explorer list; each row drills
 * straight into that entity's detail pane. Ranks 1 get the gold accent.
 */
export function LeaderboardCard ({
  title,
  icon,
  allHref,
  allLabel,
  rows,
  emptyLabel,
}: {
  title: string
  icon: ReactNode
  allHref: string
  allLabel: string
  rows: LeaderboardRow[]
  emptyLabel: string
}) {
  const navigate = useNavigate()

  return (
    <div className="flex flex-col overflow-hidden rounded-lg border border-line bg-surface">
      <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
        <div className="flex items-center gap-2.5 text-[13px] font-semibold">
          <span className="text-ink-3">{icon}</span>
          {title}
        </div>
        <button
          type="button"
          onClick={() => navigate(allHref)}
          className="flex items-center gap-0.5 text-[11px] font-semibold text-crimson transition-colors hover:text-crimson-dark"
        >
          {allLabel}
          <ChevronRight aria-hidden className="h-3 w-3 rtl:rotate-180" strokeWidth={2.5} />
        </button>
      </div>
      {rows.length === 0 ? (
        <div className="px-5 py-8 text-center text-[12px] text-ink-3">{emptyLabel}</div>
      ) : (
        <ul>
          {rows.map((row, i) => (
            <li key={row.id}>
              <button
                type="button"
                onClick={() => navigate(row.href)}
                className="group flex w-full items-center gap-3 border-t border-line px-5 py-2.5 text-start transition-colors first:border-t-0 hover:bg-paper"
              >
                <span
                  className={cn(
                    "grid h-5 w-5 flex-none place-items-center rounded-full text-[10px] font-bold",
                    i === 0 ? "bg-gold-soft text-gold" : "bg-paper-2 text-ink-3"
                  )}
                >
                  {i + 1}
                </span>
                <span className="min-w-0 flex-1">
                  <span
                    className={cn(
                      "block truncate font-medium transition-colors group-hover:text-crimson",
                      row.house ? "text-ink-4" : "text-ink"
                    )}
                  >
                    {row.name}
                  </span>
                  <span className="block truncate text-[11px] text-ink-3">{row.sub}</span>
                </span>
                <span className="flex-none text-end">
                  <span className="block font-mono text-[13px] font-semibold tabular-nums text-ink">
                    {row.primary}
                  </span>
                  <span className="block font-mono text-[11px] tabular-nums text-ink-3">
                    {row.secondary}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
