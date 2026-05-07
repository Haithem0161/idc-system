import { useTranslation } from "react-i18next"

export default function HomePage() {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-svh flex-col items-center justify-center gap-4">
      <h1 className="text-4xl font-bold tracking-tight">
        {t("app.title")}
      </h1>
      <p className="text-muted-foreground">
        {t("app.description")}
      </p>
    </div>
  )
}
