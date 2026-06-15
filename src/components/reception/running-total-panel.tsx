import { useTranslation } from "react-i18next"

import { cn } from "@/lib/utils"
import { useMoneyDisplay } from "@/features/settings/queries"

/**
 * Itemized running-total panel for the reception new-visit screen.
 *
 * Shows the patient-facing price as line items -- the resolved check price,
 * then dye and report surcharges when toggled on -- and a bold grand total.
 * The math mirrors the canonical Rust `money_math::compute`:
 *   total = price + dye_cost + report_cost
 * The `price` passed in is the authoritative effective price from
 * `pricing_effective` (subtype/base + doctor override). Dye/report costs come
 * from settings and are passed in already gated by whether the toggle is on.
 *
 * Numeric columns are mono + tnum and right-aligned (design-system §7, §11),
 * flipping to the leading edge under RTL via `text-end`. The currency unit
 * renders smaller and in `--ink-3` per §5.5.
 */
export interface RunningTotalLine {
  /** i18n-resolved label for the line (e.g. the check name, "Dye"). */
  label: string
  amountIqd: number
  /** Emphasize as the resolved base price (slightly stronger than surcharges). */
  emphasis?: boolean
}

export interface RunningTotalPanelProps {
  /** The resolved line items, in display order. Empty while nothing priced yet. */
  lines: RunningTotalLine[]
  totalIqd: number
  /**
   * True once a price is known (a check type with a base price, or a chosen
   * subtype). While false the panel shows a muted placeholder instead of 0.
   */
  hasPrice: boolean
  /** The authoritative price is still resolving; show a subtle pending hint. */
  estimating?: boolean
  /** The Finish button and autosave indicator, rendered below the total. */
  children?: React.ReactNode
}

export function RunningTotalPanel ({
  lines,
  totalIqd,
  hasPrice,
  estimating = false,
  children,
}: RunningTotalPanelProps) {
  const { t } = useTranslation(["reception"])
  const money = useMoneyDisplay()

  return (
    <aside className="panel" data-testid="running-total-panel">
      <div className="panel-head">
        <span className="panel-title">
          {t("reception.new_visit.total_label")}
        </span>
        {estimating ? (
          <span
            className="text-[10px] font-semibold uppercase tracking-[0.06em] text-ink-4"
            aria-live="polite"
          >
            {t("reception.new_visit.estimating")}
          </span>
        ) : null}
      </div>
      <div className="panel-body space-y-4">
        {hasPrice ? (
          <>
            <ul className="space-y-2" data-testid="running-total-lines">
              {lines.map((line, i) => (
                <li
                  key={`${line.label}-${i}`}
                  className="flex items-baseline justify-between gap-3"
                >
                  <span
                    className={cn(
                      "min-w-0 truncate text-[13px]",
                      line.emphasis ? "text-ink-2" : "text-ink-3"
                    )}
                    title={line.label}
                  >
                    {line.label}
                  </span>
                  <span className="shrink-0 font-mono text-[13px] tabular-nums text-ink-2">
                    {money.format(line.amountIqd)}
                  </span>
                </li>
              ))}
            </ul>

            <div className="border-t border-line pt-3">
              <div className="flex items-baseline justify-between gap-3">
                <span className="text-[10px] font-semibold uppercase tracking-[0.1em] text-ink-3">
                  {t("reception.new_visit.total")}
                </span>
                <span className="flex items-baseline gap-1.5">
                  <span
                    className="font-mono text-[28px] font-bold tabular-nums text-ink"
                    data-testid="running-total"
                  >
                    {money.format(totalIqd)}
                  </span>
                  <span className="font-mono text-[13px] font-medium text-ink-3">
                    {money.currencySymbol}
                  </span>
                </span>
              </div>
            </div>
          </>
        ) : (
          <p
            className="font-mono text-[28px] font-bold tabular-nums text-ink-4"
            data-testid="running-total"
          >
            {"—"}
          </p>
        )}

        {children}
      </div>
    </aside>
  )
}
