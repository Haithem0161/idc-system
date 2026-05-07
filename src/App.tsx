import { Outlet } from "react-router"
import { Helmet } from "@dr.pogodin/react-helmet"
import { useTranslation } from "react-i18next"

export default function App() {
  const { t } = useTranslation()

  return (
    <>
      <Helmet>
        <title>{t("app.title")}</title>
        <meta name="description" content={t("app.description")} />
      </Helmet>
      <Outlet />
    </>
  )
}
