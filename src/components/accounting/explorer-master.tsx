import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"
import { Search, Activity, Stethoscope, Users, Contact, Boxes } from "lucide-react"

import {
  EXPLORER_ENTITIES,
  type ExplorerEntity,
  type MasterRow,
} from "@/components/accounting/explorer-types"
import { cn } from "@/lib/utils"

const ENTITY_ICONS: Record<ExplorerEntity, typeof Activity> = {
  visits: Activity,
  doctors: Stethoscope,
  operators: Users,
  mandoubs: Contact,
  checks: Boxes,
}

const ENTITY_LABEL_KEYS: Record<ExplorerEntity, { key: string; fallback: string }> = {
  visits: { key: "accounting.explorer.entity.visits", fallback: "Visits" },
  doctors: { key: "accounting.explorer.entity.doctors", fallback: "Doctors" },
  operators: { key: "accounting.explorer.entity.operators", fallback: "Operators" },
  mandoubs: { key: "accounting.explorer.entity.mandoubs", fallback: "Representatives" },
  checks: { key: "accounting.explorer.entity.checks", fallback: "Checks" },
}

/**
 * Left pane of the explorer: entity tabs + a live search box + the ranked
 * master list. Selecting a row navigates (the parent owns the route); switching
 * an entity tab navigates to that entity's bare list. The list scrolls
 * independently of the detail pane so siblings can be compared without losing
 * scroll position.
 */
export function ExplorerMaster ({
  entity,
  rows,
  selectedId,
  isLoading,
  search,
  onSearchChange,
  sortLabel,
  footTotal,
}: {
  entity: ExplorerEntity
  rows: MasterRow[]
  selectedId: string | undefined
  isLoading: boolean
  search: string
  onSearchChange: (value: string) => void
  sortLabel: string
  footTotal: string
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()

  return (
    <div className="flex min-h-0 flex-col border-e border-line bg-paper">
      {/* entity tabs */}
      <div
        role="tablist"
        aria-label={t("accounting.explorer.entity_tabs_aria", { defaultValue: "Explorer entity" })}
        className="flex flex-none gap-1 border-b border-line px-3.5 pt-2.5"
      >
        {EXPLORER_ENTITIES.map((e) => {
          const Icon = ENTITY_ICONS[e]
          const lbl = ENTITY_LABEL_KEYS[e]
          const active = e === entity
          return (
            <button
              key={e}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => navigate(`/accounting/explore/${e}`)}
              className={cn(
                "flex items-center gap-1.5 border-b-2 px-3 py-2 text-[12px] font-semibold transition-colors",
                active
                  ? "border-crimson text-ink"
                  : "border-transparent text-ink-3 hover:text-ink-2"
              )}
            >
              <Icon className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              {t(lbl.key, { defaultValue: lbl.fallback })}
            </button>
          )
        })}
      </div>

      {/* search + sort */}
      <div className="flex flex-none items-center gap-2.5 border-b border-line px-3.5 py-2.5">
        <div className="relative flex-1">
          <Search
            aria-hidden
            className="pointer-events-none absolute start-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-ink-3"
            strokeWidth={2}
          />
          <input
            type="search"
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={t("accounting.explorer.search_placeholder", {
              defaultValue: "Search {{entity}}…",
              entity: t(ENTITY_LABEL_KEYS[entity].key, {
                defaultValue: ENTITY_LABEL_KEYS[entity].fallback,
              }).toLowerCase(),
            })}
            className="input h-9 w-full ps-9 text-[12px]"
            aria-label={t("accounting.explorer.search_aria", { defaultValue: "Search" })}
          />
        </div>
        <span className="flex-none whitespace-nowrap rounded-md border border-line-2 bg-surface px-2.5 py-1.5 text-[11px] font-semibold text-ink-3">
          {sortLabel}
        </span>
      </div>

      {/* list */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="space-y-px p-2">
            {Array.from({ length: 8 }).map((_, i) => (
              <div key={i} className="h-[52px] animate-pulse rounded bg-paper-2" />
            ))}
          </div>
        ) : rows.length === 0 ? (
          <div className="p-10 text-center text-[12px] text-ink-3">
            {t("accounting.explorer.no_matches", { defaultValue: "No matches." })}
          </div>
        ) : (
          rows.map((row, i) => {
            const selected = row.id === selectedId
            return (
              <button
                key={row.id}
                type="button"
                aria-current={selected ? "true" : undefined}
                onClick={() => navigate(`/accounting/explore/${entity}/${row.id}`)}
                className={cn(
                  "flex w-full items-center gap-3 border-b border-line border-s-[3px] px-3.5 py-3 text-start transition-colors",
                  selected
                    ? "border-s-crimson bg-surface"
                    : "border-s-transparent hover:bg-paper-2"
                )}
              >
                <span
                  className={cn(
                    "grid h-5 w-5 flex-none place-items-center rounded-full text-[10px] font-bold",
                    selected ? "bg-crimson-soft text-crimson" : "bg-paper-2 text-ink-3"
                  )}
                >
                  {i + 1}
                </span>
                <span className="min-w-0 flex-1">
                  <span
                    className={cn(
                      "block truncate font-medium",
                      row.house ? "text-ink-4" : "text-ink"
                    )}
                  >
                    {row.name}
                  </span>
                  <span className="block truncate text-[11px] text-ink-3">{row.sub}</span>
                </span>
                <span className="flex-none text-end">
                  <span className="block font-mono text-[13px] font-semibold tabular-nums text-ink">
                    {row.primary}
                  </span>
                  <span className="block font-mono text-[11px] tabular-nums text-ink-3">
                    {row.secondary}
                  </span>
                </span>
              </button>
            )
          })
        )}
      </div>

      {/* footer total */}
      <div className="flex flex-none items-center justify-between border-t border-line bg-paper-2 px-3.5 py-2.5 text-[11px] text-ink-3">
        <span>
          {t("accounting.explorer.count", {
            defaultValue: "{{count}} results",
            count: rows.length,
          })}
        </span>
        <span className="font-mono tabular-nums">{footTotal}</span>
      </div>
    </div>
  )
}
