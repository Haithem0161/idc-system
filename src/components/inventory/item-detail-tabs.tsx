import { useTranslation } from "react-i18next"

export type ItemDetailTab =
  | "overview"
  | "consumption_map"
  | "adjustments"
  | "audit"

interface Props {
  active: ItemDetailTab
  onChange: (tab: ItemDetailTab) => void
}

const TABS: ItemDetailTab[] = [
  "overview",
  "consumption_map",
  "adjustments",
  "audit",
]

export function ItemDetailTabs ({ active, onChange }: Props) {
  const { t } = useTranslation()
  return (
    <div
      role="tablist"
      aria-label={t("inventory.item.tabs.overview")}
      className="inline-flex items-center gap-1 rounded-md border border-line bg-paper-2 p-1"
    >
      {TABS.map((tab) => {
        const isActive = tab === active
        return (
          <button
            type="button"
            role="tab"
            key={tab}
            aria-selected={isActive}
            onClick={() => onChange(tab)}
            className={
              "rounded-sm px-3 py-1.5 text-[12px] font-medium transition-colors " +
              (isActive
                ? "bg-surface text-ink shadow-sm"
                : "text-ink-3 hover:text-ink-2")
            }
          >
            {t(`inventory.item.tabs.${tab}` as const)}
          </button>
        )
      })}
    </div>
  )
}
