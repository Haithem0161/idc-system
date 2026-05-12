import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { ShieldOff } from "lucide-react"

import { Logo } from "@/components/shell/logo"
import { useLogout } from "@/features/auth/queries"

export default function NoAccessPage () {
  const { t } = useTranslation()
  const logout = useLogout()
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-paper px-6 py-10 text-center">
      <Logo size={44} className="opacity-60" />
      <div className="flex h-12 w-12 items-center justify-center rounded-full bg-crimson-soft">
        <ShieldOff className="h-5 w-5 text-crimson" strokeWidth={1.8} />
      </div>
      <span className="eyebrow">{t("auth.no_access_eyebrow", { defaultValue: "Restricted" })}</span>
      <div className="max-w-md space-y-2">
        <h1 className="text-[24px] font-bold leading-[1.1] tracking-[-0.02em] text-ink">
          {t("auth.no_access_title", { defaultValue: "No access" })}
        </h1>
        <p className="text-[13px] text-ink-3">
          {t("auth.no_access_body", {
            defaultValue: "Your role does not have access to this section. Please contact your administrator.",
          })}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() => logout.mutate()}
        >
          {t("auth.sign_out", { defaultValue: "Sign out" })}
        </button>
        <Link to="/" className="btn btn-ink btn-sm">
          {t("nav.home", { defaultValue: "Home" })}
        </Link>
      </div>
    </div>
  )
}
