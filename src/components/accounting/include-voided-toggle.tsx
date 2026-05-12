import { useTranslation } from "react-i18next"

import { cn } from "@/lib/utils"
import { useAccountingFiltersStore } from "@/stores/accounting-filters-store"

/** Phase-07 §7.2 status toggle: locked-only default; include voided. */
export function IncludeVoidedToggle () {
  const { t } = useTranslation()
  const value = useAccountingFiltersStore((s) => s.includeVoided)
  const set = useAccountingFiltersStore((s) => s.setIncludeVoided)
  return (
    <div
      role="tablist"
      aria-label={t("accounting.toggle.status_aria", { defaultValue: "Visit status" })}
      className="inline-flex gap-1 rounded-md border border-line bg-paper-2 p-1"
    >
      <button
        type="button"
        role="tab"
        aria-selected={!value}
        onClick={() => set(false)}
        className={cn(
          "rounded px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] transition-colors",
          !value
            ? "bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
            : "text-ink-3 hover:text-ink"
        )}
      >
        {t("accounting.toggle.locked", { defaultValue: "Locked" })}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={value}
        onClick={() => set(true)}
        className={cn(
          "rounded px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] transition-colors",
          value
            ? "bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
            : "text-ink-3 hover:text-ink"
        )}
      >
        {t("accounting.toggle.include_voided", { defaultValue: "Include voided" })}
      </button>
    </div>
  )
}
