import { useTranslation } from "react-i18next"

import {
  AUDIT_ACTIONS,
  AUDIT_ENTITIES,
  type AuditFilter,
} from "@/lib/schemas/audit"

interface Props {
  value: AuditFilter
  onChange: (next: AuditFilter) => void
}

/**
 * Audit filters: actor / action / entity / entity_id_prefix / date range /
 * free text. Phase-08 §3 Frontend + §7.6 + §7.24.
 *
 * Inputs are plain HTML elements so the editorial token system carries them
 * without leaning on shadcn's heavier primitives.
 */
export function AuditFilters({ value, onChange }: Props) {
  const { t } = useTranslation()
  const set = <K extends keyof AuditFilter>(key: K, v: AuditFilter[K]) => {
    onChange({ ...value, [key]: v })
  }

  const actorId = "audit-filter-actor"
  const actionId = "audit-filter-action"
  const entityId = "audit-filter-entity"
  const prefixId = "audit-filter-prefix"
  const fromId = "audit-filter-from"
  const toId = "audit-filter-to"
  const textId = "audit-filter-text"

  return (
    <form
      className="panel"
      onSubmit={(e) => e.preventDefault()}
      aria-label={t("audit.filters.aria", { defaultValue: "Audit filters" })}
    >
      <div className="panel-head">
        <span className="panel-title">
          {t("audit.filters.title", { defaultValue: "Filters" })}
        </span>
      </div>
      <div className="panel-body grid grid-cols-1 gap-4 md:grid-cols-3 lg:grid-cols-6">
        <div>
          <label className="field-label" htmlFor={actorId}>
            {t("audit.filters.actor", { defaultValue: "Actor (UUID)" })}
          </label>
          <input
            id={actorId}
            type="text"
            className="input"
            value={value.actor_user_id ?? ""}
            placeholder={t("audit.filters.actor_placeholder", {
              defaultValue: "user uuid",
            })}
            onChange={(e) => set("actor_user_id", e.target.value || undefined)}
          />
        </div>
        <div>
          <label className="field-label" htmlFor={actionId}>
            {t("audit.filters.action", { defaultValue: "Action" })}
          </label>
          <select
            id={actionId}
            className="input"
            value={value.action ?? ""}
            onChange={(e) =>
              set(
                "action",
                e.target.value
                  ? (e.target.value as AuditFilter["action"])
                  : undefined
              )
            }
          >
            <option value="">
              {t("audit.filters.any", { defaultValue: "Any" })}
            </option>
            {AUDIT_ACTIONS.map((a) => (
              <option key={a} value={a}>
                {t(`audit.actions.${a}`, { defaultValue: a })}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="field-label" htmlFor={entityId}>
            {t("audit.filters.entity", { defaultValue: "Entity" })}
          </label>
          <select
            id={entityId}
            className="input"
            value={value.entity ?? ""}
            onChange={(e) =>
              set(
                "entity",
                e.target.value
                  ? (e.target.value as AuditFilter["entity"])
                  : undefined
              )
            }
          >
            <option value="">
              {t("audit.filters.any", { defaultValue: "Any" })}
            </option>
            {AUDIT_ENTITIES.map((ent) => (
              <option key={ent} value={ent}>
                {t(`audit.entities.${ent}`, { defaultValue: ent })}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="field-label" htmlFor={prefixId}>
            {t("audit.filters.entity_id_prefix", {
              defaultValue: "Entity ID prefix",
            })}
          </label>
          <input
            id={prefixId}
            type="text"
            className="input"
            placeholder={t("audit.filters.entity_id_prefix_placeholder", {
              defaultValue: "first 4-36 chars",
            })}
            value={value.entity_id_prefix ?? ""}
            onChange={(e) =>
              set("entity_id_prefix", e.target.value || undefined)
            }
          />
        </div>
        <div>
          <label className="field-label" htmlFor={fromId}>
            {t("audit.filters.from", { defaultValue: "From" })}
          </label>
          <input
            id={fromId}
            type="datetime-local"
            className="input"
            value={localFromIso(value.from_utc)}
            onChange={(e) => set("from_utc", localToIso(e.target.value))}
          />
        </div>
        <div>
          <label className="field-label" htmlFor={toId}>
            {t("audit.filters.to", { defaultValue: "To" })}
          </label>
          <input
            id={toId}
            type="datetime-local"
            className="input"
            value={localFromIso(value.to_utc)}
            onChange={(e) => set("to_utc", localToIso(e.target.value))}
          />
        </div>
        <div className="md:col-span-3 lg:col-span-6">
          <label className="field-label" htmlFor={textId}>
            {t("audit.filters.text", { defaultValue: "Free text" })}
          </label>
          <input
            id={textId}
            type="text"
            className="input"
            placeholder={t("audit.filters.text_placeholder", {
              defaultValue: "Search in delta or entity_id (2-100 chars)",
            })}
            value={value.text ?? ""}
            onChange={(e) => set("text", e.target.value || undefined)}
          />
        </div>
      </div>
    </form>
  )
}

function localFromIso(iso: string | undefined): string {
  if (!iso) return ""
  try {
    const d = new Date(iso)
    const tzOffset = d.getTimezoneOffset() * 60_000
    return new Date(d.getTime() - tzOffset).toISOString().slice(0, 16)
  } catch {
    return ""
  }
}

function localToIso(local: string): string | undefined {
  if (!local) return undefined
  const d = new Date(local)
  if (Number.isNaN(d.getTime())) return undefined
  return d.toISOString()
}
