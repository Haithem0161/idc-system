import { useTranslation } from "react-i18next"

/**
 * Toggles the i18n locale between `en` and `ar`. The persistence is handled
 * by i18next-browser-languagedetector + `localStorage`.
 */
export function LanguageToggle() {
  const { i18n, t } = useTranslation()
  const next = i18n.language === "ar" ? "en" : "ar"
  const label =
    next === "ar"
      ? t("language.switch_to_arabic", { defaultValue: "العربية" })
      : t("language.switch_to_english", { defaultValue: "English" })

  return (
    <button
      type="button"
      onClick={() => {
        void i18n.changeLanguage(next)
      }}
      className="inline-flex h-8 items-center rounded-md border border-border bg-background px-3 text-xs font-medium hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
      aria-label={t("language.toggle_aria", { defaultValue: "Toggle language" })}
    >
      {label}
    </button>
  )
}
