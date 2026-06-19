import type { ReactNode } from "react"

import { cn } from "@/lib/utils"

/**
 * The header at the top of an explorer detail pane: an eyebrow rule (joined
 * with middots), the entity title, and an optional trailing actions slot
 * (e.g. "Open full page" / "Export CSV"). The `muted` flag renders the title
 * in the house/internal tone.
 */
export function DetailHeader ({
  eyebrow,
  title,
  actions,
  muted,
}: {
  eyebrow: string[]
  title: string
  actions?: ReactNode
  muted?: boolean
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0">
        <div className="eyebrow">{eyebrow.filter(Boolean).join(" · ")}</div>
        <h2
          className={cn(
            "mt-1 truncate text-[24px] font-bold tracking-tight",
            muted ? "text-ink-4" : "text-ink"
          )}
        >
          {title}
        </h2>
      </div>
      {actions ? <div className="flex flex-none items-center gap-2">{actions}</div> : null}
    </div>
  )
}

/**
 * A labelled section within a detail pane: a small uppercase title with an
 * optional muted meta string on the trailing side, then the section body.
 */
export function DetailSection ({
  title,
  meta,
  children,
}: {
  title: string
  meta?: string
  children: ReactNode
}) {
  return (
    <section className="space-y-2.5">
      <div className="flex items-center justify-between">
        <h3 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">
          {title}
        </h3>
        {meta ? <span className="text-[11px] text-ink-3">{meta}</span> : null}
      </div>
      {children}
    </section>
  )
}
