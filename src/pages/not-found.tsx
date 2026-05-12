import { Link } from "react-router"
import { useTranslation } from "react-i18next"

import { Logo } from "@/components/shell/logo"

export default function NotFoundPage() {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-svh flex-col items-center justify-center gap-5 bg-paper px-6 py-12 text-center">
      <Logo size={44} className="opacity-60" />
      <span className="eyebrow">{t("not_found.eyebrow", { defaultValue: "Lost" })}</span>
      <div className="space-y-2">
        <p className="font-mono text-[44px] font-semibold leading-none tracking-[-0.02em] text-ink">404</p>
        <p className="text-[14px] text-ink-3">{t("not_found.body", { defaultValue: "We could not find that page." })}</p>
      </div>
      <Link to="/" className="btn btn-ink btn-sm">
        {t("nav.home", { defaultValue: "Home" })}
      </Link>
    </div>
  )
}
