import { useMemo, useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"

import { AdminHeader, ErrorBanner } from "@/components/admin/admin-panel"
import {
  useChecksGrid,
  useVisitsTodayByCheck,
} from "@/features/visits/queries"
import { useVisitTabsStore, VISIT_TAB_CAP } from "@/stores/visit-tabs-store"
import type { VisitStatusLiteral } from "@/lib/schemas/visit"

// "draft" is intentionally absent: drafts are now tab-shaped and live in the
// reception tab strip, not as a workspace status. Legacy draft rows still
// surface under "all" so they remain triagable until they're voided / locked.
const FILTER_KEYS: Array<{ key: "all" | VisitStatusLiteral; label: string }> = [
  { key: "all", label: "reception.workspace.filters.all" },
  { key: "locked", label: "reception.workspace.filters.locked" },
  { key: "voided", label: "reception.workspace.filters.voided" },
]

export default function CheckWorkspacePage () {
  const { t, i18n } = useTranslation(["reception", "common"])
  const navigate = useNavigate()
  const params = useParams()
  const slug = params.slug ?? ""
  const [filter, setFilter] = useState<"all" | VisitStatusLiteral>("all")

  const { data: cards } = useChecksGrid()
  const checkType = useMemo(
    () => (cards ?? []).find((c) => c.check_type_id === slug),
    [cards, slug]
  )
  const checkTypeId = checkType?.check_type_id ?? null
  const localized =
    (i18n.language === "en" ? checkType?.name_en : checkType?.name_ar) ??
    checkType?.name_ar ??
    "—"

  const { data: visits, error } = useVisitsTodayByCheck(checkTypeId)

  const filtered = (visits ?? []).filter((v) =>
    filter === "all" ? true : v.status === filter
  )

  const openTab = useVisitTabsStore((s) => s.openTab)
  const tabsCount = useVisitTabsStore((s) => s.tabs.length)

  function handleNewVisit () {
    if (!checkTypeId) return
    if (tabsCount >= VISIT_TAB_CAP) {
      window.alert(t("reception.tabs.cap_reached"))
      return
    }
    openTab(checkTypeId)
    navigate("/reception/new")
  }

  return (
    <div className="space-y-6">
      <Link to="/reception" className="text-[11px] uppercase tracking-[0.1em] text-ink-3 hover:text-ink">
        ← {t("reception.workspace.back_to_grid")}
      </Link>
      <AdminHeader
        eyebrow={t("reception.eyebrow")}
        title={t("reception.workspace.title", { check: localized })}
        subtitle={t("reception.workspace.subtitle")}
        actions={(
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleNewVisit}
            disabled={!checkTypeId}
          >
            {t("reception.workspace.new_visit")}
          </button>
        )}
      />
      <ErrorBanner message={error ? String(error.message ?? error) : null} />

      <div className="flex flex-wrap gap-2">
        {FILTER_KEYS.map((f) => (
          <button
            key={f.key}
            type="button"
            className={
              filter === f.key
                ? "rounded-md border border-line-2 bg-surface px-3 py-1.5 text-[12px] font-semibold text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
                : "rounded-md border border-transparent bg-paper-2 px-3 py-1.5 text-[12px] font-medium text-ink-3 hover:bg-paper"
            }
            onClick={() => setFilter(f.key)}
          >
            {t(f.label)}
          </button>
        ))}
      </div>

      <div className="panel">
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("reception.workspace.columns.row")}</th>
              <th>{t("reception.workspace.columns.created")}</th>
              <th>{t("reception.workspace.columns.patient")}</th>
              <th>{t("reception.workspace.columns.doctor")}</th>
              <th>{t("reception.workspace.columns.operator")}</th>
              <th className="text-right">
                {t("reception.workspace.columns.total")}
              </th>
              <th>{t("reception.workspace.columns.status")}</th>
              <th>{t("reception.workspace.columns.sync")}</th>
              <th className="text-right">
                {t("reception.workspace.columns.actions")}
              </th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((v, idx) => (
              <tr key={v.id}>
                <td className="font-mono tabular-nums">{idx + 1}</td>
                <td className="font-mono tabular-nums">
                  {new Date(v.created_at).toLocaleTimeString()}
                </td>
                <td>
                  {v.snapshots?.patient_name ?? "—"}
                </td>
                <td>
                  {v.snapshots?.doctor_name ?? t("reception.new_visit.house")}
                </td>
                <td>
                  {v.snapshots?.operator_name ?? "—"}
                </td>
                <td className="text-right font-mono tabular-nums">
                  {v.snapshots?.total_amount_iqd?.toLocaleString() ?? "—"}
                </td>
                <td>
                  <span
                    className={
                      v.status === "locked"
                        ? "status-pill is-success"
                        : v.status === "voided"
                          ? "status-pill is-danger"
                          : "status-pill is-info"
                    }
                  >
                    {t(`reception.visit_detail.status_pill.${v.status}`)}
                  </span>
                </td>
                <td>
                  {v.dirty ? (
                    <span
                      title={t("reception.workspace.columns.sync")}
                      className="inline-block h-2 w-2 rounded-full bg-gold"
                    />
                  ) : (
                    <span className="inline-block h-2 w-2 rounded-full bg-success" />
                  )}
                </td>
                <td className="text-right">
                  <Link
                    to={`/reception/visits/${v.id}`}
                    className="btn btn-ghost btn-sm"
                  >
                    {t("reception.workspace.open_detail")}
                  </Link>
                </td>
              </tr>
            ))}
            {filtered.length === 0 ? (
              <tr>
                <td
                  colSpan={9}
                  className="py-10 text-center text-[13px] text-ink-3"
                >
                  {t("reception.workspace.empty")}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}
