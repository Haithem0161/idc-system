import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { ChevronDown, ChevronRight } from "lucide-react"

import { DirtyDot } from "@/components/ui/dirty-dot"
import { entityDetailRoute } from "@/lib/audit/entity-routes"
import type { AuditPage, AuditRow } from "@/lib/schemas/audit"
import { useDeviceStore } from "@/stores/device-store"
import { cn } from "@/lib/utils"

import { DeltaViewer } from "./delta-viewer"
import { ServerBackedBadge } from "./server-backed-badge"

/** Short, human-glanceable form of a UUID (first 8 chars) for fallbacks. */
function shortId (id: string): string {
  return id.slice(0, 8)
}

/**
 * Format an audit timestamp as a readable UTC date+time in DD/MM/YYYY form (the
 * column is labelled "(UTC)", so we keep UTC rather than local to stay
 * forensically honest). Falls back to the raw string if it cannot be parsed.
 */
function formatAuditTime (iso: string, locale: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return new Intl.DateTimeFormat(locale, {
    timeZone: "UTC",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(d)
}

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
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const target = entityDetailRoute(row.entity, row.entity_id)
  const currentDeviceId = useDeviceStore((s) => s.device?.deviceId)

  // Actor: resolved name (incl. "System"), else the short id. Full UUID on hover.
  const actorDisplay = row.actor_name ?? shortId(row.actor_user_id)
  // Entity: resolved label, else short id. Full UUID on hover.
  const entityDisplay = row.entity_label ?? shortId(row.entity_id)
  // Device: mark the current device; else short id. Full UUID on hover.
  const isThisDevice = currentDeviceId === row.device_id
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
        <td className="font-mono text-[11px]" title={row.at}>
          {formatAuditTime(row.at, locale)}
        </td>
        <td title={row.actor_user_id}>
          <span className={cn(!row.actor_name && "font-mono text-[11px] text-ink-3")}>
            {actorDisplay}
          </span>
        </td>
        <td>
          <span className="status-pill">
            {t(`audit.actions.${row.action}`, { defaultValue: row.action })}
          </span>
        </td>
        <td>{t(`audit.entities.${row.entity}`, { defaultValue: row.entity })}</td>
        <td title={row.entity_id}>
          {target ? (
            <Link
              onClick={(e) => e.stopPropagation()}
              to={target}
              className={cn(
                "text-info hover:underline",
                !row.entity_label && "font-mono text-[11px]"
              )}
            >
              {entityDisplay}
            </Link>
          ) : (
            <span className={cn(!row.entity_label && "font-mono text-[11px] text-ink-3")}>
              {entityDisplay}
            </span>
          )}
        </td>
        <td title={row.device_id} className="text-[12px]">
          {isThisDevice ? (
            <span className="text-ink-2">
              {t("audit.this_device", { defaultValue: "This device" })}
            </span>
          ) : (
            <span className="font-mono text-[11px] text-ink-3">
              {shortId(row.device_id)}
            </span>
          )}
        </td>
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
