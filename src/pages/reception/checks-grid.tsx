import { Link, useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import { AdminHeader, ErrorBanner } from "@/components/admin/admin-panel"
import { useChecksGrid } from "@/features/visits/queries"
import { useVisitTabsStore, VISIT_TAB_CAP } from "@/stores/visit-tabs-store"

export default function ChecksGridPage () {
  const { t, i18n } = useTranslation(["reception", "common"])
  const navigate = useNavigate()
  const { data: cards, error } = useChecksGrid()
  const lang = i18n.language

  const openTab = useVisitTabsStore((s) => s.openTab)
  const tabsCount = useVisitTabsStore((s) => s.tabs.length)

  function startVisit (checkTypeId: string) {
    if (tabsCount >= VISIT_TAB_CAP) {
      window.alert(t("reception.tabs.cap_reached"))
      return
    }
    openTab(checkTypeId)
    navigate("/reception/new")
  }

  return (
    <div className="space-y-6 px-9 pb-12 pt-6">
      <AdminHeader
        eyebrow={t("reception.eyebrow")}
        title={t("reception.checks_grid.title")}
        subtitle={t("reception.checks_grid.subtitle")}
        actions={(
          <Link to="/reception/shifts" className="btn btn-ghost btn-sm">
            {t("reception.checks_grid.operator_shifts")}
          </Link>
        )}
      />
      <ErrorBanner message={error ? String(error.message ?? error) : null} />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        {(cards ?? []).map((card) => {
          const localized =
            (lang === "en" ? card.name_en : card.name_ar) ?? card.name_ar
          return (
            <button
              key={card.check_type_id}
              type="button"
              onClick={() => startVisit(card.check_type_id)}
              className="panel block text-start transition hover:-translate-y-px hover:shadow-[0_4px_12px_rgba(10,18,48,0.04)]"
            >
              <div className="panel-head">
                <span className="panel-title">{localized}</span>
                <span className="count-badge">{card.todays_visits}</span>
              </div>
              <div className="panel-body space-y-2">
                <p className="text-[13px] text-ink-3">
                  {t("reception.checks_grid.todays_visits", {
                    count: card.todays_visits,
                  })}
                </p>
                <div className="flex flex-wrap gap-2">
                  {card.dye_supported ? (
                    <span className="status-pill is-info">
                      {t("reception.new_visit.dye")}
                    </span>
                  ) : null}
                  {card.report_supported ? (
                    <span className="status-pill is-info">
                      {t("reception.new_visit.report")}
                    </span>
                  ) : null}
                  {card.has_subtypes ? (
                    <span className="status-pill is-success">
                      {t("reception.checks_grid.has_subtypes")}
                    </span>
                  ) : null}
                </div>
                <Link
                  to={`/reception/checks/${card.check_type_id}`}
                  className="mt-1 inline-block text-[11px] font-medium text-ink-3 hover:text-ink"
                  onClick={(ev) => ev.stopPropagation()}
                >
                  {t("reception.checks_grid.open_workspace")}
                </Link>
              </div>
            </button>
          )
        })}
        {(cards ?? []).length === 0 && !error ? (
          <p className="col-span-full text-center text-[13px] text-ink-3">
            {t("reception.checks_grid.empty")}
          </p>
        ) : null}
      </div>
    </div>
  )
}
