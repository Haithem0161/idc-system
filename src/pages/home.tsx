import { useTranslation } from "react-i18next"

import { Logo } from "@/components/shell/logo"

export default function HomePage() {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-full flex-col items-center justify-center gap-6 py-12">
      <Logo size={96} className="drop-shadow-sm" />
      <div className="text-center">
        <h1 className="text-4xl font-bold tracking-tight">
          {t("app.title")}
        </h1>
        <p className="mt-2 text-muted-foreground">
          {t("app.description")}
        </p>
      </div>
    </div>
  )
}
