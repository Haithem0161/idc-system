import { Link, useSearchParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft } from "lucide-react"

import { AdjustForm } from "@/components/inventory/adjust-form"

export default function InventoryAdjustPage () {
  const { t } = useTranslation()
  const [params] = useSearchParams()
  const initialItemId = params.get("item")

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <header className="space-y-2">
        <Link
          to="/inventory"
          className="inline-flex items-center gap-1 text-[12px] font-medium text-ink-3 hover:text-ink"
        >
          <ArrowLeft className="h-3.5 w-3.5" strokeWidth={1.8} />
          {t("inventory.adjust.back")}
        </Link>
        <div>
          <div className="eyebrow">{t("inventory.eyebrow")}</div>
          <h1 className="text-2xl font-bold tracking-tight text-ink">
            {t("inventory.adjust.title")}
          </h1>
        </div>
      </header>

      <AdjustForm initialItemId={initialItemId} />
    </div>
  )
}
