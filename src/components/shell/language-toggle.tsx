import { useTranslation } from "react-i18next"
import { Languages } from "lucide-react"

/**
 * Pill-shaped language toggle. Matches the header chrome voice -- 999px
 * radius, line-2 border, 11px uppercase tracking.
 */
export function LanguageToggle() {
  const { i18n, t } = useTranslation()
  const next = i18n.language === "ar" ? "en" : "ar"
  const currentLabel = i18n.language === "ar" ? "العربية" : "English"

  return (
    <button
      type="button"
      onClick={() => {
        void i18n.changeLanguage(next)
      }}
      className="inline-flex h-8 items-center gap-1.5 rounded-full border border-line-2 bg-paper px-3 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
      aria-label={t("language.toggle_aria", { defaultValue: "Toggle language" })}
    >
      <Languages className="h-3.5 w-3.5" strokeWidth={1.8} />
      <span>{currentLabel}</span>
    </button>
  )
}
