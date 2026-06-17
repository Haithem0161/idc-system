import { useMemo, useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { ChevronLeft, ChevronRight, Copy, Users } from "lucide-react"

import { AdminHeader, EmptyRow } from "@/components/admin/admin-panel"
import { PatientDuplicates } from "@/components/patients/patient-duplicates"
import { useDebouncedValue } from "@/hooks/use-debounced-value"
import {
  type PatientListFilter,
  usePatientsList,
} from "@/features/patients/queries"
import type { PatientSortLiteral } from "@/lib/ipc"
import { ageFromBirthDate } from "@/features/patients/age"
import { cn } from "@/lib/utils"

const PAGE_SIZE = 50

const SORTS: PatientSortLiteral[] = [
  "updated_desc",
  "name_asc",
  "name_desc",
  "created_desc",
]

export default function PatientsListPage () {
  const { t } = useTranslation(["patients", "common"])
  const [rawQuery, setRawQuery] = useState("")
  const query = useDebouncedValue(rawQuery, 250)
  const [includeDeleted, setIncludeDeleted] = useState(false)
  const [sort, setSort] = useState<PatientSortLiteral>("updated_desc")
  const [page, setPage] = useState(0)
  const [showDuplicates, setShowDuplicates] = useState(false)

  const filter: PatientListFilter = useMemo(
    () => ({
      query: query.trim().length >= 2 ? query.trim() : undefined,
      includeDeleted,
      sort,
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    }),
    [query, includeDeleted, sort, page]
  )

  const list = usePatientsList(filter)
  const rows = list.data ?? []
  const hasNextPage = rows.length === PAGE_SIZE

  // Reset to the first page whenever the filter inputs change.
  const onFilterChange = <T,>(setter: (v: T) => void) => (v: T) => {
    setter(v)
    setPage(0)
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <AdminHeader
        eyebrow={t("patients:eyebrow")}
        title={t("patients:list.title")}
        subtitle={t("patients:list.subtitle")}
        count={rows.length}
        actions={
          <>
            <input
              type="search"
              value={rawQuery}
              onChange={(e) => onFilterChange(setRawQuery)(e.target.value)}
              placeholder={t("patients:list.search_placeholder")}
              className="input h-8 w-52"
            />
            <select
              value={sort}
              onChange={(e) =>
                onFilterChange(setSort)(e.target.value as PatientSortLiteral)
              }
              className="input h-8 w-40"
              aria-label={t("patients:list.sort")}
            >
              {SORTS.map((s) => (
                <option key={s} value={s}>
                  {t(`patients:list.sort_${s}`)}
                </option>
              ))}
            </select>
            <label className="inline-flex cursor-pointer items-center gap-2 text-[12px] font-medium text-ink-2">
              <input
                type="checkbox"
                checked={includeDeleted}
                onChange={(e) =>
                  onFilterChange(setIncludeDeleted)(e.target.checked)
                }
                className="h-3.5 w-3.5 accent-ink"
              />
              <span>{t("patients:list.include_archived")}</span>
            </label>
            <button
              type="button"
              onClick={() => setShowDuplicates((v) => !v)}
              className="btn btn-ghost btn-sm"
            >
              <Copy className="h-3.5 w-3.5" strokeWidth={1.8} />
              {t("patients:duplicates.find")}
            </button>
          </>
        }
      />

      {showDuplicates ? (
        <PatientDuplicates onClose={() => setShowDuplicates(false)} />
      ) : null}

      <div className="panel overflow-hidden">
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("patients:list.col_name")}</th>
              <th>{t("patients:list.col_phone")}</th>
              <th>{t("patients:list.col_sex")}</th>
              <th className="text-end">{t("patients:list.col_age")}</th>
              <th>{t("patients:list.col_file_no")}</th>
              <th>{t("patients:list.col_status")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((p) => {
              const age = ageFromBirthDate(p.birth_date)
              const archived = p.deleted_at != null
              return (
                <tr key={p.id}>
                  <td className="font-medium text-ink">
                    <Link
                      to={`/reception/patients/${p.id}`}
                      className="inline-flex items-center gap-2 hover:text-crimson"
                    >
                      <Users className="h-3.5 w-3.5 text-ink-4" strokeWidth={1.8} />
                      {p.name}
                    </Link>
                  </td>
                  <td className="font-mono text-[12px] text-ink-3">
                    {p.phone ?? "—"}
                  </td>
                  <td className="text-[12px] text-ink-3">
                    {p.sex ? t(`patients:sex_${p.sex}`) : "—"}
                  </td>
                  <td className="text-end font-mono text-[12px] text-ink-3">
                    {age != null ? age : "—"}
                  </td>
                  <td className="font-mono text-[12px] text-ink-3">
                    {p.file_no ?? "—"}
                  </td>
                  <td>
                    <span
                      className={cn("status-pill", archived ? "is-warn" : "is-success")}
                    >
                      {archived
                        ? t("patients:status_archived")
                        : t("patients:status_active")}
                    </span>
                  </td>
                </tr>
              )
            })}
            {rows.length === 0 ? (
              <EmptyRow colSpan={6} message={t("patients:list.empty")} />
            ) : null}
          </tbody>
        </table>
      </div>

      {/* Offset pagination: Prev/Next gated by whether a full page came back. */}
      {(page > 0 || hasNextPage) && rows.length > 0 ? (
        <div className="flex items-center justify-between text-[12px] text-ink-3">
          <span>
            {t("patients:list.page", { page: page + 1 })}
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={page === 0}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              className="btn btn-ghost btn-sm"
            >
              <ChevronLeft className="h-3.5 w-3.5 rtl:rotate-180" strokeWidth={1.8} />
              {t("patients:list.prev")}
            </button>
            <button
              type="button"
              disabled={!hasNextPage}
              onClick={() => setPage((p) => p + 1)}
              className="btn btn-ghost btn-sm"
            >
              {t("patients:list.next")}
              <ChevronRight className="h-3.5 w-3.5 rtl:rotate-180" strokeWidth={1.8} />
            </button>
          </div>
        </div>
      ) : null}
    </div>
  )
}
