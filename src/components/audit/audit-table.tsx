import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { ChevronDown, ChevronRight } from "lucide-react"

import { DirtyDot } from "@/components/ui/dirty-dot"
import { entityDetailRoute } from "@/lib/audit/entity-routes"
import type { AuditPage, AuditRow } from "@/lib/schemas/audit"
import { cn } from "@/lib/utils"

import { DeltaViewer } from "./delta-viewer"
import { ServerBackedBadge } from "./server-backed-badge"

/**
 * Audit table with inline expandable delta. Phase-08 §3 Frontend.
 * - `Entity` cell links to the source-row detail page when one exists
 *   (phase-08 §7.7); else plain text.
 * - `Pending sync` column renders `<DirtyDot dirty={row.dirty} />`
 *   (phase-08 §7.15).
 * - `<ServerBackedBadge>` appears in the header when `mode !== local`
 *   (phase-08 §7.25).
 */
export function AuditTable({ page }: { page: AuditPage | undefined }) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState<string | null>(null)
  const dirIcon = (id: string) =>
    expanded === id ? (
      <ChevronDown className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
    ) : (
      <ChevronRight
        className="h-3.5 w-3.5 rtl:rotate-180"
        strokeWidth={1.8}
        aria-hidden
      />
    )

  if (!page) {
    return (
      <div className="rounded-md border border-line bg-surface px-6 py-12 text-center text-[13px] text-ink-3">
        {t("common.loading", { defaultValue: "Loading..." })}
      </div>
    )
  }
  if (page.rows.length === 0) {
    return (
      <div className="rounded-md border border-line bg-surface px-6 py-12 text-center text-[13px] text-ink-3">
        {t("audit.empty", { defaultValue: "No audit rows match these filters." })}
      </div>
    )
  }

  return (
    <div className="rounded-md border border-line bg-surface">
      <div className="flex items-center justify-between border-b border-line px-4 py-3">
        <span className="text-[11px] uppercase tracking-[0.1em] text-ink-3">
          {t("audit.columns.results", {
            defaultValue: "{{count}} results",
            count: page.rows.length,
          })}
        </span>
        <ServerBackedBadge mode={page.mode} />
      </div>
      <table className="data-table w-full">
        <thead>
          <tr>
            <th aria-hidden style={{ width: 28 }} />
            <th>{t("audit.columns.at", { defaultValue: "At (UTC)" })}</th>
            <th>{t("audit.columns.actor", { defaultValue: "Actor" })}</th>
            <th>{t("audit.columns.action", { defaultValue: "Action" })}</th>
            <th>{t("audit.columns.entity", { defaultValue: "Entity" })}</th>
            <th>{t("audit.columns.entity_id", { defaultValue: "Entity ID" })}</th>
            <th>{t("audit.columns.device", { defaultValue: "Device" })}</th>
            <th>
              {t("audit.columns.pending_sync", { defaultValue: "Pending sync" })}
            </th>
          </tr>
        </thead>
        <tbody>
          {page.rows.map((row) => (
            <RowGroup
              key={row.id}
              row={row}
              expanded={expanded === row.id}
              onToggle={() =>
                setExpanded((cur) => (cur === row.id ? null : row.id))
              }
              icon={dirIcon(row.id)}
            />
          ))}
        </tbody>
      </table>
    </div>
  )
}

function RowGroup({
  row,
  expanded,
  onToggle,
  icon,
}: {
  row: AuditRow
  expanded: boolean
  onToggle: () => void
  icon: React.ReactNode
}) {
  const { t } = useTranslation()
  const target = entityDetailRoute(row.entity, row.entity_id)
  return (
    <>
      <tr
        className={cn(expanded && "bg-paper-2")}
        onClick={onToggle}
        style={{ cursor: "pointer" }}
      >
        <td>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation()
              onToggle()
            }}
            aria-label={t(
              expanded ? "a11y.icons.collapse" : "a11y.icons.expand",
              { defaultValue: expanded ? "Collapse" : "Expand" }
            )}
            className="flex h-6 w-6 items-center justify-center rounded text-ink-3 transition-colors hover:bg-paper-2 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
          >
            {icon}
          </button>
        </td>
        <td className="font-mono text-[11px]">{row.at}</td>
        <td>{row.actor_user_id.slice(0, 8)}</td>
        <td>
          <span className="status-pill">
            {t(`audit.actions.${row.action}`, { defaultValue: row.action })}
          </span>
        </td>
        <td>{t(`audit.entities.${row.entity}`, { defaultValue: row.entity })}</td>
        <td className="font-mono text-[11px]">
          {target ? (
            <Link
              onClick={(e) => e.stopPropagation()}
              to={target}
              className="text-info hover:underline"
            >
              {row.entity_id.slice(0, 8)}
            </Link>
          ) : (
            row.entity_id.slice(0, 8)
          )}
        </td>
        <td className="font-mono text-[11px]">{row.device_id.slice(0, 8)}</td>
        <td>
          <DirtyDot dirty={row.dirty} />
        </td>
      </tr>
      {expanded ? (
        <tr className="bg-paper">
          <td colSpan={8} className="border-t border-line bg-paper-2 p-3">
            <DeltaViewer delta={row.delta} />
          </td>
        </tr>
      ) : null}
    </>
  )
}
