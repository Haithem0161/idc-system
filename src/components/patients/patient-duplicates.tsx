import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { GitMerge, X } from "lucide-react"

import {
  usePatientDuplicates,
  usePatientMerge,
} from "@/features/patients/queries"
import { usePatientsList } from "@/features/patients/queries"
import type { DuplicateGroupRecord, PatientRecord } from "@/lib/ipc"
import { formatIpcError } from "@/lib/errors"
import { cn } from "@/lib/utils"

/**
 * Duplicate-detection panel for the patients archive. Lists groups of patients
 * that collide on name or phone, and lets the user merge a group into a chosen
 * survivor. Merge re-points all visits to the survivor and archives the rest.
 */
export function PatientDuplicates ({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation(["patients"])
  const dupes = usePatientDuplicates(true)
  const groups = dupes.data ?? []

  return (
    <div className="panel">
      <div className="panel-head">
        <span className="panel-title">{t("patients:duplicates.title")}</span>
        <button
          type="button"
          onClick={onClose}
          aria-label={t("patients:duplicates.merge_cancel")}
          className="flex h-7 w-7 items-center justify-center rounded text-ink-3 hover:bg-paper-2 hover:text-ink"
        >
          <X className="h-3.5 w-3.5" strokeWidth={1.8} />
        </button>
      </div>
      <div className="panel-body space-y-4">
        <p className="text-[12px] text-ink-3">{t("patients:duplicates.subtitle")}</p>
        {groups.length === 0 ? (
          <p className="py-4 text-center text-[13px] text-ink-3">
            {t("patients:duplicates.none")}
          </p>
        ) : (
          <ul className="space-y-3">
            {groups.map((g) => (
              <DuplicateGroupRow key={`${g.kind}:${g.key}`} group={g} />
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

function DuplicateGroupRow ({ group }: { group: DuplicateGroupRecord }) {
  const { t } = useTranslation(["patients"])
  // Pull a generous page so the group's patients resolve to names; the list is
  // cached and small in practice.
  const list = usePatientsList({ limit: 500, includeDeleted: false })
  const merge = usePatientMerge()
  const [survivor, setSurvivor] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [done, setDone] = useState(false)

  const byId = useMemo(() => {
    const m = new Map<string, PatientRecord>()
    for (const p of list.data ?? []) m.set(p.id, p)
    return m
  }, [list.data])

  const members = group.patient_ids
    .map((id) => byId.get(id))
    .filter((p): p is PatientRecord => Boolean(p))

  const doMerge = async () => {
    if (!survivor) return
    setError(null)
    try {
      // Merge every other member into the survivor, sequentially.
      for (const p of members) {
        if (p.id === survivor) continue
        await merge.mutateAsync({ survivor_id: survivor, merged_id: p.id })
      }
      setDone(true)
    } catch (e) {
      setError(formatIpcError(e, t))
    }
  }

  if (done) {
    return (
      <li className="rounded-md border border-line bg-success-soft px-3 py-2 text-[12px] text-success">
        {t("patients:duplicates.merged_done")}
      </li>
    )
  }

  return (
    <li className="rounded-md border border-line bg-paper-2 p-3">
      <div className="mb-2 flex items-center gap-2">
        <span className="status-pill is-info">
          {group.kind === "name"
            ? t("patients:duplicates.by_name")
            : t("patients:duplicates.by_phone")}
        </span>
        <span className="font-mono text-[11px] text-ink-3">{group.key}</span>
      </div>
      <ul className="space-y-1.5">
        {members.map((p) => (
          <li
            key={p.id}
            className="flex items-center justify-between gap-2 text-[13px]"
          >
            <label className="inline-flex cursor-pointer items-center gap-2">
              <input
                type="radio"
                name={`survivor-${group.kind}-${group.key}`}
                checked={survivor === p.id}
                onChange={() => setSurvivor(p.id)}
                className="accent-ink"
              />
              <span className="font-medium text-ink">{p.name}</span>
              {p.phone ? (
                <span className="font-mono text-[11px] text-ink-4">{p.phone}</span>
              ) : null}
            </label>
            {survivor === p.id ? (
              <span className="text-[10px] font-semibold uppercase tracking-[0.06em] text-success">
                {t("patients:duplicates.survivor")}
              </span>
            ) : null}
          </li>
        ))}
      </ul>
      {error ? (
        <p className="mt-2 text-[12px] text-crimson">{error}</p>
      ) : null}
      <div className="mt-3 flex justify-end">
        <button
          type="button"
          disabled={!survivor || merge.isPending}
          onClick={() => void doMerge()}
          className={cn("btn btn-sm", survivor ? "btn-ink" : "btn-ghost")}
        >
          <GitMerge className="h-3.5 w-3.5" strokeWidth={1.8} />
          {t("patients:duplicates.merge")}
        </button>
      </div>
    </li>
  )
}
