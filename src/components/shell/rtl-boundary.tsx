import { useEffect, type ReactNode } from "react"
import { useTranslation } from "react-i18next"

/**
 * Reflects the current i18n language onto `<html dir>` and `lang` attributes.
 *
 * Tailwind v4 uses logical properties so the same utility classes flip
 * automatically when `dir="rtl"`. Mount once at the top of the app tree.
 */
export function RtlBoundary({ children }: { children: ReactNode }) {
  const { i18n } = useTranslation()
  useEffect(() => {
    const dir = i18n.language === "ar" ? "rtl" : "ltr"
    document.documentElement.dir = dir
    document.documentElement.lang = i18n.language
  }, [i18n.language])
  return <>{children}</>
}
