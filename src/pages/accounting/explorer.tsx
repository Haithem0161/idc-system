import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Navigate, useParams } from "react-router"

import { AccountingToolbar } from "@/components/accounting/accounting-toolbar"
import { ExplorerMaster } from "@/components/accounting/explorer-master"
import { useMasterRows } from "@/components/accounting/use-master-rows"
import { DoctorDetailPane } from "@/components/accounting/doctor-detail-pane"
import { OperatorDetailPane } from "@/components/accounting/operator-detail-pane"
import { CheckDetailPane } from "@/components/accounting/check-detail-pane"
import { VisitDetailPane } from "@/components/accounting/visit-detail-pane"
import { isExplorerEntity } from "@/components/accounting/explorer-types"

/**
 * The accounting Explorer: a two-pane master/detail terminal. The URL is the
 * source of truth -- `/accounting/explore/:entity/:id?` -- so selection is
 * deep-linkable, the back button works, and the shell breadcrumb is driven by
 * the route handle. The left list scrolls independently of the detail pane.
 */
export default function AccountingExplorerPage () {
  const { t } = useTranslation()
  const params = useParams<{ entity?: string; id?: string }>()

  // Validate the entity segment; fall back to doctors for the data hook so the
  // hook order stays stable, then redirect below if the URL was invalid.
  const entityValid = isExplorerEntity(params.entity)
  const entity = entityValid ? params.entity : "doctors"
  const selectedId = params.id

  // Search is local to a single entity tab. Reset it when the entity changes
  // (switching tabs is a navigation, not a remount) by tracking the entity the
  // current search was typed against -- the idiomatic "reset state on prop
  // change during render" pattern.
  const [search, setSearch] = useState("")
  const [searchEntity, setSearchEntity] = useState(entity)
  if (searchEntity !== entity) {
    setSearchEntity(entity)
    setSearch("")
  }

  // Hooks must run unconditionally (rules of hooks) -- call before any return.
  const master = useMasterRows(entity, searchEntity === entity ? search : "")

  if (!entityValid) {
    return <Navigate to="/accounting/explore/doctors" replace />
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <header className="flex-none">
        <div className="eyebrow">{t("accounting.explorer.eyebrow", { defaultValue: "Accounting" })}</div>
        <h1 className="mt-1 text-[26px] font-bold tracking-tight text-ink">
          {t("accounting.explorer.title", { defaultValue: "Explorer" })}
        </h1>
      </header>

      <div className="flex-none">
        <AccountingToolbar />
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-hidden rounded-lg border border-line lg:grid-cols-[400px_1fr]">
        <ExplorerMaster
          entity={entity}
          rows={master.rows}
          selectedId={selectedId}
          isLoading={master.isLoading}
          search={search}
          onSearchChange={setSearch}
          sortLabel={master.sortLabel}
          footTotal={master.footTotal}
        />

        <div className="min-h-0 overflow-y-auto bg-paper">
          {selectedId ? (
            <div className="space-y-4 p-6">
              {entity === "doctors" ? (
                <DoctorDetailPane segment={selectedId} />
              ) : entity === "operators" ? (
                <OperatorDetailPane operatorId={selectedId} />
              ) : entity === "checks" ? (
                <CheckDetailPane checkTypeId={selectedId} />
              ) : (
                // visits: stay in the master/detail layout -- the slim
                // accounting breakdown renders inline; the full read-only page
                // (print + reception tabs) is reachable from the pane itself.
                <VisitDetailPane visitId={selectedId} />
              )}
            </div>
          ) : (
            <EmptyDetail />
          )}
        </div>
      </div>
    </div>
  )
}

function EmptyDetail () {
  const { t } = useTranslation()
  return (
    <div className="flex h-full items-center justify-center p-10 text-center">
      <div className="max-w-xs">
        <div className="text-[13px] font-semibold text-ink-2">
          {t("accounting.explorer.empty_detail_title", { defaultValue: "Select a row" })}
        </div>
        <div className="mt-1 text-[12px] text-ink-3">
          {t("accounting.explorer.empty_detail_body", {
            defaultValue: "Pick an item from the list to see its breakdown here.",
          })}
        </div>
      </div>
    </div>
  )
}
