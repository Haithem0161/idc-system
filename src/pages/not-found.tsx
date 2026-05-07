import { Link } from "react-router"
import { useTranslation } from "react-i18next"

export default function NotFoundPage() {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-svh flex-col items-center justify-center gap-4">
      <h1 className="text-4xl font-bold">404</h1>
      <p className="text-muted-foreground">Page not found</p>
      <Link to="/" className="text-primary underline">
        {t("nav.home")}
      </Link>
    </div>
  )
}
