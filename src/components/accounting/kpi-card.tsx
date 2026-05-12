import { useTranslation } from "react-i18next"

import { formatIqd, formatPermille } from "@/lib/format/money"
import { cn } from "@/lib/utils"

interface KpiCardProps {
  label: string
  amount: number
  deltaPermille?: number
  arabicNumerals?: boolean
  tone?: "ink" | "default"
}

/** Single KPI tile (PRD §7.2.1; phase-07 §7.1). */
export function KpiCard ({ label, amount, deltaPermille, arabicNumerals, tone }: KpiCardProps) {
  const { i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const value = formatIqd(amount, { arabicNumerals, locale })
  const isInk = tone === "ink"
  return (
    <div
      className={cn(
        "rounded-lg p-5 transition-colors",
        isInk
          ? "bg-ink text-paper"
          : "bg-surface border border-line hover:bg-paper"
      )}
    >
      <div
        className={cn(
          "text-[10.5px] font-semibold uppercase tracking-[0.12em]",
          isInk ? "text-paper/70" : "text-ink-3"
        )}
      >
        {label}
      </div>
      <div
        className={cn(
          "mt-2 font-mono text-[30px] font-bold tracking-tight tabular-nums",
          isInk ? "text-paper" : "text-ink"
        )}
      >
        {value}
        <span
          className={cn(
            "ms-1 text-[14px] font-medium",
            isInk ? "text-paper/60" : "text-ink-3"
          )}
        >
          IQD
        </span>
      </div>
      {typeof deltaPermille === "number" ? (
        <div
          className={cn(
            "mt-1 text-[11px] font-medium tabular-nums",
            deltaPermille > 0
              ? "text-success"
              : deltaPermille < 0
                ? "text-crimson"
                : "text-ink-3"
          )}
        >
          {formatPermille(deltaPermille, { arabicNumerals })}
        </div>
      ) : null}
    </div>
  )
}
