import { useMemo } from "react"
import { useNavigate } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import { cn } from "@/lib/utils"
import { useChecksGrid, useVisitDiscard } from "@/features/visits/queries"
import type { ChecksGridCardRecord } from "@/lib/ipc"
import {
  useVisitTabsStore,
  VISIT_TAB_CAP,
  type VisitTab,
} from "@/stores/visit-tabs-store"

/**
 * Persistent reception tab strip. Pinned at the top of every /reception/*
 * page so receptionists can flip between in-progress visits across check
 * types. Tabs are backed by hidden draft rows (auto-saved on field change);
 * the strip itself just renders the list + handles open / activate / close.
 *
 * The `+ New visit` button navigates to the checks grid (the canonical
 * picker page); the actual tab gets created when the user clicks a card
 * there.
 */
export function VisitTabsStrip () {
  const { t, i18n } = useTranslation(["reception", "common"])
  const navigate = useNavigate()
  const tabs = useVisitTabsStore((s) => s.tabs)
  const activeTabId = useVisitTabsStore((s) => s.activeTabId)
  const closeTab = useVisitTabsStore((s) => s.closeTab)
  const setActiveTab = useVisitTabsStore((s) => s.setActiveTab)

  const { data: cards } = useChecksGrid()
  const visitDiscard = useVisitDiscard()

  const cardById = useMemo(() => {
    const m = new Map<string, ChecksGridCardRecord>()
    for (const c of cards ?? []) m.set(c.check_type_id, c)
    return m
  }, [cards])

  function localizedName (card: ChecksGridCardRecord | undefined): string {
    if (!card) return "—"
    return i18n.language === "en" ? (card.name_en ?? card.name_ar) : card.name_ar
  }

  function handleOpenNew () {
    if (tabs.length >= VISIT_TAB_CAP) {
      window.alert(t("reception.tabs.cap_reached"))
      return
    }
    navigate("/reception")
  }

  function handleActivate (tab: VisitTab) {
    setActiveTab(tab.tabId)
    navigate("/reception/new")
  }

  async function handleClose (tab: VisitTab, ev: React.MouseEvent) {
    ev.stopPropagation()
    const dirty =
      tab.form.patientName.trim().length > 0 ||
      tab.form.patientId !== null ||
      tab.draftVisitId !== null
    if (dirty && !window.confirm(t("reception.tabs.discard_confirm"))) return
    if (tab.draftVisitId) {
      try {
        await visitDiscard.mutateAsync({ visit_id: tab.draftVisitId })
      } catch {
        // Even if discard fails (already locked elsewhere, etc.), close the
        // tab so the UI doesn't get stuck. The row will surface in the
        // workspace if it still exists.
      }
    }
    closeTab(tab.tabId)
  }

  if (tabs.length === 0) {
    return (
      <div className="border-b border-line bg-paper-2/60 px-6 py-2">
        <button
          type="button"
          onClick={handleOpenNew}
          className="inline-flex items-center gap-1.5 rounded-md border border-line-2 bg-surface px-3 py-1.5 text-[12px] font-semibold text-ink-2 transition-colors hover:bg-paper"
        >
          <Plus className="h-3.5 w-3.5" strokeWidth={2} />
          {t("reception.tabs.new_visit")}
        </button>
      </div>
    )
  }

  return (
    <div className="border-b border-line bg-paper-2/60 px-6 py-2">
      <ul className="flex flex-wrap items-center gap-1.5">
        {tabs.map((tab) => {
          const card = cardById.get(tab.checkTypeId)
          const label =
            tab.form.patientName.trim().length > 0
              ? tab.form.patientName.trim()
              : t("reception.tabs.untitled")
          const isActive = tab.tabId === activeTabId
          return (
            <li key={tab.tabId}>
              <button
                type="button"
                onClick={() => handleActivate(tab)}
                aria-current={isActive ? "page" : undefined}
                className={cn(
                  "group inline-flex max-w-[260px] items-center gap-2 rounded-md border px-3 py-1.5 text-[12px] transition-colors",
                  isActive
                    ? "border-line-2 bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
                    : "border-transparent bg-paper-2 text-ink-3 hover:bg-paper hover:text-ink",
                )}
              >
                <span className="truncate font-semibold">{label}</span>
                <span className="shrink-0 rounded-sm bg-paper-2 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.04em] text-ink-3">
                  {localizedName(card)}
                </span>
                <span
                  role="button"
                  tabIndex={0}
                  aria-label={t("reception.tabs.close")}
                  onClick={(ev) => void handleClose(tab, ev)}
                  onKeyDown={(ev) => {
                    if (ev.key === "Enter" || ev.key === " ") {
                      ev.preventDefault()
                      void handleClose(tab, ev as unknown as React.MouseEvent)
                    }
                  }}
                  className="ml-1 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-ink-3 hover:bg-paper hover:text-crimson"
                >
                  <X className="h-3 w-3" strokeWidth={2} />
                </span>
              </button>
            </li>
          )
        })}
        <li>
          <button
            type="button"
            onClick={handleOpenNew}
            className="inline-flex items-center gap-1.5 rounded-md border border-dashed border-line-2 bg-transparent px-3 py-1.5 text-[12px] font-semibold text-ink-3 transition-colors hover:bg-paper hover:text-ink"
          >
            <Plus className="h-3.5 w-3.5" strokeWidth={2} />
            {t("reception.tabs.new_visit")}
          </button>
        </li>
      </ul>
    </div>
  )
}

