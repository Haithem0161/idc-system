import { useMemo } from "react"
import { Link, useMatches } from "react-router"
import { useTranslation } from "react-i18next"

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
 * Breadcrumb strip auto-derived from React Router matched routes. Routes
 * declare a `handle.crumb(data) => string` to participate.
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

  if (crumbs.length === 0) return null

  return (
    <nav aria-label={t("a11y.breadcrumbs", { defaultValue: "Breadcrumbs" })} className="text-xs">
      <ol className="flex items-center gap-1 text-muted-foreground">
        {crumbs.map((crumb, i) => (
          <li key={crumb.pathname} className="flex items-center gap-1">
            {i > 0 ? <span aria-hidden>/</span> : null}
            <Link
              to={crumb.pathname}
              className="hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 rounded"
            >
              {crumb.label}
            </Link>
          </li>
        ))}
      </ol>
    </nav>
  )
}
