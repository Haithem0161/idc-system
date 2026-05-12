import { useTranslation } from "react-i18next"

/**
 * Skip-to-content link (WCAG 2.4.1). Visually hidden until focused; targets
 * `<main id="main-content">`.
 */
export function SkipToContent() {
  const { t } = useTranslation()
  return (
    <a
      href="#main-content"
      className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-md focus:bg-ink focus:px-3 focus:py-2 focus:text-paper focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30 focus-visible:ring-offset-2"
    >
      {t("a11y.skip_to_content", { defaultValue: "Skip to content" })}
    </a>
  )
}
