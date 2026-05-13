import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "@/lib/utils"

interface Props {
  local: unknown
  server: unknown
  onChange: (merged: Record<string, unknown> | null) => void
}

/**
 * Per-field merge editor (phase-08 §3 Frontend `<MergeEditor>`).
 *
 * For each top-level field present in either payload, the user picks
 * `local`, `server`, or `manual` (free-text edit). The merged object is
 * propagated upward; if any field is "manual" with empty value, the
 * editor reports `null` to disable Submit.
 */
export function MergeEditor({ local, server, onChange }: Props) {
  const { t } = useTranslation()
  const fields = useMemo(() => allFields(local, server), [local, server])
  const [choices, setChoices] = useState<Record<string, "local" | "server" | "manual">>(
    () => Object.fromEntries(fields.map((f) => [f, "local" as const]))
  )
  const [manualValues, setManualValues] = useState<Record<string, string>>({})

  useEffect(() => {
    const merged: Record<string, unknown> = {}
    let blocked = false
    for (const f of fields) {
      const choice = choices[f] ?? "local"
      if (choice === "manual") {
        const v = manualValues[f]
        if (v == null || v.length === 0) {
          blocked = true
          continue
        }
        try {
          merged[f] = JSON.parse(v)
        } catch {
          merged[f] = v
        }
      } else {
        const src = (choice === "local" ? local : server) as Record<string, unknown>
        merged[f] = src && typeof src === "object" ? (src as Record<string, unknown>)[f] : null
      }
    }
    onChange(blocked ? null : merged)
  }, [choices, manualValues, fields, local, server, onChange])

  return (
    <div className="space-y-3">
      <div className="text-[11px] font-semibold uppercase tracking-[0.1em] text-ink-3">
        {t("sync_conflicts.merge.title", { defaultValue: "Per-field merge" })}
      </div>
      <table className="w-full border-collapse text-[12px]">
        <thead>
          <tr className="bg-paper-2 text-[10px] uppercase tracking-[0.1em] text-ink-3">
            <th className="px-3 py-2 text-start font-semibold">
              {t("sync_conflicts.merge.field", { defaultValue: "Field" })}
            </th>
            <th className="px-3 py-2 text-start font-semibold">
              {t("sync_conflicts.merge.choice", { defaultValue: "Source" })}
            </th>
            <th className="px-3 py-2 text-start font-semibold">
              {t("sync_conflicts.merge.value", { defaultValue: "Value" })}
            </th>
          </tr>
        </thead>
        <tbody>
          {fields.map((f) => {
            const choice = choices[f] ?? "local"
            const localV = stringify((local as Record<string, unknown>)?.[f])
            const serverV = stringify((server as Record<string, unknown>)?.[f])
            return (
              <tr key={f} className="border-t border-line">
                <td className="px-3 py-2 font-medium text-ink-2">{f}</td>
                <td className="px-3 py-2">
                  <select
                    className="input"
                    value={choice}
                    onChange={(e) =>
                      setChoices((c) => ({
                        ...c,
                        [f]: e.target.value as "local" | "server" | "manual",
                      }))
                    }
                  >
                    <option value="local">
                      {t("sync_conflicts.merge.from_local", {
                        defaultValue: "Local",
                      })}
                    </option>
                    <option value="server">
                      {t("sync_conflicts.merge.from_server", {
                        defaultValue: "Server",
                      })}
                    </option>
                    <option value="manual">
                      {t("sync_conflicts.merge.from_manual", {
                        defaultValue: "Manual",
                      })}
                    </option>
                  </select>
                </td>
                <td
                  className={cn(
                    "px-3 py-2 font-mono text-[11px]",
                    choice === "local" && "text-info",
                    choice === "server" && "text-success",
                    choice === "manual" && "text-ink"
                  )}
                >
                  {choice === "manual" ? (
                    <input
                      type="text"
                      className="input w-full"
                      value={manualValues[f] ?? ""}
                      onChange={(e) =>
                        setManualValues((m) => ({ ...m, [f]: e.target.value }))
                      }
                      placeholder={t("sync_conflicts.merge.manual_placeholder", {
                        defaultValue: "JSON or string",
                      })}
                    />
                  ) : choice === "local" ? (
                    <span title={localV}>{truncate(localV)}</span>
                  ) : (
                    <span title={serverV}>{truncate(serverV)}</span>
                  )}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

function allFields(local: unknown, server: unknown): string[] {
  const set = new Set<string>()
  if (local && typeof local === "object" && !Array.isArray(local)) {
    Object.keys(local as Record<string, unknown>).forEach((k) => set.add(k))
  }
  if (server && typeof server === "object" && !Array.isArray(server)) {
    Object.keys(server as Record<string, unknown>).forEach((k) => set.add(k))
  }
  return [...set].sort()
}

function stringify(v: unknown): string {
  if (v == null) return "—"
  if (typeof v === "string") return v
  return JSON.stringify(v)
}

function truncate(s: string, max = 60): string {
  if (s.length <= max) return s
  return s.slice(0, max - 1) + "…"
}
