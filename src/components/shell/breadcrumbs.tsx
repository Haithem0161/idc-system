import { useMemo } from "react"
import { Link, useMatches } from "react-router"
import { useTranslation } from "react-i18next"
import { ChevronRight } from "lucide-react"

interface CrumbHandle {
  crumb?: (data: unknown) => string
}

interface Match {
  id: string
  pathname: string
  handle?: CrumbHandle
  data: unknown
}

/**
 * Editorial breadcrumb strip. Routes participate via `handle.crumb(data)`.
 * Sits in the 64px header to the leading side; matches the eyebrow voice.
 */
export function Breadcrumbs() {
  const { t } = useTranslation()
  const matches = useMatches() as unknown as Match[]
  const crumbs = useMemo(
    () =>
      matches
        .map((m) => ({ pathname: m.pathname, label: m.handle?.crumb?.(m.data) }))
        .filter((c): c is { pathname: string; label: string } => Boolean(c.label)),
    [matches]
  )

  if (crumbs.length === 0) {
    return <span className="text-[12px] font-medium text-ink-3">{t("app.title", { defaultValue: "IDC" })}</span>
  }

  return (
    <nav
      aria-label={t("a11y.breadcrumbs", { defaultValue: "Breadcrumbs" })}
      className="text-[12px]"
    >
      <ol className="flex items-center gap-1.5 text-ink-3">
        {crumbs.map((crumb, i) => (
          <li key={crumb.pathname} className="flex items-center gap-1.5">
            {i > 0 ? <ChevronRight aria-hidden className="h-3 w-3 text-ink-4 rtl:rotate-180" strokeWidth={1.8} /> : null}
            <Link
              to={crumb.pathname}
              className="rounded font-medium transition-colors hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20 focus-visible:ring-offset-2"
            >
              {crumb.label}
            </Link>
          </li>
        ))}
      </ol>
    </nav>
  )
}
