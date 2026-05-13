import { useMemo } from "react"
import { useTranslation } from "react-i18next"

/**
 * Renders an audit `delta` (`{ field: { from, to } }` shape) as a colored
 * two-column diff. Identical fields are omitted, matching phase-08
 * `<DeltaViewer>`.
 *
 * Edge cases:
 * - `delta = null` or non-object: shows the eyebrow with no rows.
 * - `delta = { foo: "bar" }` (flat, not `{from,to}`): rendered as a single
 *   "to" column (the audit row was synthetic, e.g. vacuum self-audit).
 */
export function DeltaViewer({ delta }: { delta: unknown }) {
  const { t } = useTranslation()
  const rows = useMemo(() => parseDelta(delta), [delta])

  if (rows.length === 0) {
    return (
      <div className="rounded-md bg-paper-2 px-4 py-3 text-[12px] text-ink-3">
        {t("audit.delta.empty", { defaultValue: "No changes recorded" })}
      </div>
    )
  }

  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="bg-paper-2 text-[10px] uppercase tracking-[0.1em] text-ink-3">
          <th className="px-3 py-2 text-start font-semibold">
            {t("audit.delta.field", { defaultValue: "Field" })}
          </th>
          <th className="px-3 py-2 text-start font-semibold">
            {t("audit.delta.from", { defaultValue: "From" })}
          </th>
          <th className="px-3 py-2 text-start font-semibold">
            {t("audit.delta.to", { defaultValue: "To" })}
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.field} className="border-t border-line text-[12px]">
            <td className="px-3 py-2 font-medium text-ink-2">{r.field}</td>
            <td className="px-3 py-2 font-mono text-[11px] text-crimson">
              {r.from}
            </td>
            <td className="px-3 py-2 font-mono text-[11px] text-success">
              {r.to}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

interface DeltaRow {
  field: string
  from: string
  to: string
}

function parseDelta(delta: unknown): DeltaRow[] {
  if (!delta || typeof delta !== "object" || Array.isArray(delta)) return []
  const obj = delta as Record<string, unknown>
  const rows: DeltaRow[] = []
  for (const [field, value] of Object.entries(obj)) {
    if (value && typeof value === "object" && !Array.isArray(value)) {
      const v = value as Record<string, unknown>
      if ("from" in v || "to" in v) {
        rows.push({
          field,
          from: stringify(v.from),
          to: stringify(v.to),
        })
        continue
      }
    }
    rows.push({ field, from: "—", to: stringify(value) })
  }
  return rows
}

function stringify(v: unknown): string {
  if (v === null || v === undefined) return "—"
  if (typeof v === "string") return v
  return JSON.stringify(v)
}
