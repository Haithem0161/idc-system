import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import {
  useDoctorEarnings,
  useOperatorEarnings,
  useVisitsReport,
} from "@/features/reports/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { formatIqd } from "@/lib/format/money"
import { abbreviateIqd, thousandsK } from "@/components/accounting/format-abbrev"
import { doctorIdToSegment } from "@/components/accounting/entity-link"
import type { ExplorerEntity, MasterRow } from "@/components/accounting/explorer-types"
import type { ReportsVisitsArgs } from "@/lib/ipc"
import {
  rangeAsUtc,
  useAccountingFiltersStore,
} from "@/stores/accounting-filters-store"

interface MasterRowsResult {
  rows: MasterRow[]
  isLoading: boolean
  /** Localized label naming the sort applied to the list. */
  sortLabel: string
  /** Pre-formatted footer total for the list (sum of the primary metric). */
  footTotal: string
}

/**
 * Fetch and normalize the master list for one explorer entity, then apply the
 * client-side search filter. Each entity is sorted by its most meaningful
 * metric (doctors/checks by money, operators by money, visits by recency) and
 * the footer totals the primary money column.
 *
 * Visits use the `check_type` master differently -- the list is the ungrouped
 * visit rows; for very large ranges the report caps server-side.
 */
export function useMasterRows (
  entity: ExplorerEntity,
  search: string
): MasterRowsResult {
  const { t, i18n } = useTranslation()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"
  const localeShort = i18n.language === "ar" ? "ar" : "en"
  const fromDate = useAccountingFiltersStore((s) => s.fromDate)
  const toDate = useAccountingFiltersStore((s) => s.toDate)
  const includeVoided = useAccountingFiltersStore((s) => s.includeVoided)
  const range = useMemo(
    () => ({ ...rangeAsUtc(fromDate, toDate), include_voided: includeVoided }),
    [fromDate, toDate, includeVoided]
  )

  // Always call every hook (rules of hooks); disable the unused ones.
  const doctors = useDoctorEarnings(range)
  const operators = useOperatorEarnings(range)
  const checksArgs: ReportsVisitsArgs = useMemo(
    () => ({ ...range, group_by: "by_check_type" }),
    [range]
  )
  const checks = useVisitsReport(checksArgs)
  const visitsArgs: ReportsVisitsArgs = useMemo(
    () => ({ ...range, group_by: "none", limit: 500 }),
    [range]
  )
  const visits = useVisitsReport(visitsArgs)

  const all = useMemo<{ rows: MasterRow[]; isLoading: boolean }>(() => {
    if (entity === "doctors") {
      if (!doctors.data) return { rows: [], isLoading: doctors.isLoading }
      const rows = [...doctors.data]
        .sort((a, b) => b.doctor_cut_total_iqd - a.doctor_cut_total_iqd)
        .map<MasterRow>((d) => {
          const isHouse = !d.doctor_id
          const name = isHouse
            ? t("accounting.house.label", { defaultValue: "Internal" })
            : d.name
          return {
            id: doctorIdToSegment(d.doctor_id),
            name,
            sub: [
              d.specialty ?? t("accounting.doctors.no_specialty", { defaultValue: "No specialty" }),
              t("accounting.tops.visits_count", { defaultValue: "{{count}} visits", count: d.visits }),
            ].join(" · "),
            primary: abbreviateIqd(d.doctor_cut_total_iqd),
            secondary: t("accounting.tops.per_visit", {
              defaultValue: "{{value}} / visit",
              value: thousandsK(d.avg_cut_per_visit_iqd),
            }),
            searchText: `${name} ${d.specialty ?? ""}`.toLowerCase(),
            house: isHouse,
          }
        })
      return { rows, isLoading: false }
    }

    if (entity === "operators") {
      if (!operators.data) return { rows: [], isLoading: operators.isLoading }
      const rows = [...operators.data]
        .sort((a, b) => b.operator_cut_total_iqd - a.operator_cut_total_iqd)
        .map<MasterRow>((o) => {
          const name = o.name || o.operator_id
          return {
            id: o.operator_id,
            name,
            sub: [
              t("accounting.tops.visits_count", { defaultValue: "{{count}} visits", count: o.visits }),
              t("accounting.tops.dye_count", { defaultValue: "{{count}} dye", count: o.visits_with_dye }),
            ].join(" · "),
            primary: abbreviateIqd(o.operator_cut_total_iqd),
            secondary: t("accounting.tops.per_hour", {
              defaultValue: "{{value}} / hr",
              value: thousandsK(o.avg_cut_per_hour_iqd),
            }),
            searchText: name.toLowerCase(),
          }
        })
      return { rows, isLoading: false }
    }

    if (entity === "checks") {
      if (!checks.data) return { rows: [], isLoading: checks.isLoading }
      const groups = checks.data.mode === "groups" ? checks.data.groups : []
      const rows = [...groups]
        .sort((a, b) => b.revenue_iqd - a.revenue_iqd)
        .map<MasterRow>((g) => ({
          id: g.key,
          name: g.label || g.key,
          sub: t("accounting.tops.visits_count", { defaultValue: "{{count}} visits", count: g.visits }),
          primary: abbreviateIqd(g.revenue_iqd),
          secondary: t("accounting.tops.doc_cut_per_visit", {
            defaultValue: "{{value}} doc/v",
            value: thousandsK(g.visits > 0 ? g.doctor_cut_iqd / g.visits : 0),
          }),
          searchText: (g.label || g.key).toLowerCase(),
        }))
      return { rows, isLoading: false }
    }

    // visits
    if (!visits.data) return { rows: [], isLoading: visits.isLoading }
    const visitRows = visits.data.mode === "rows" ? visits.data.rows : []
    const rows = visitRows.map<MasterRow>((v) => {
      const checkName = resolveLocaleName(
        { name_ar: v.check_type_name_ar, name_en: v.check_type_name_en },
        localeShort
      )
      return {
        id: v.visit_id,
        name: v.patient_name,
        sub: `${checkName} · ${v.locked_at ? v.locked_at.slice(0, 10) : "—"}`,
        primary: formatIqd(v.price_iqd, { locale }),
        secondary: t(`accounting.status.${v.status}`, { defaultValue: v.status }),
        searchText: `${v.patient_name} ${checkName}`.toLowerCase(),
      }
    })
    return { rows, isLoading: false }
  }, [entity, doctors, operators, checks, visits, t, locale, localeShort])

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return all.rows
    return all.rows.filter((r) => r.searchText.includes(q))
  }, [all.rows, search])

  const sortLabel = useMemo(() => {
    switch (entity) {
      case "doctors":
        return t("accounting.explorer.sort.cut_total", { defaultValue: "Cut total" })
      case "operators":
        return t("accounting.explorer.sort.cut_total", { defaultValue: "Cut total" })
      case "checks":
        return t("accounting.explorer.sort.revenue", { defaultValue: "Revenue" })
      case "visits":
      default:
        return t("accounting.explorer.sort.recent", { defaultValue: "Recent" })
    }
  }, [entity, t])

  const footTotal = useMemo(() => {
    // Sum the source money for the footer (independent of formatting/abbrev).
    let total = 0
    if (entity === "doctors" && doctors.data) {
      total = doctors.data.reduce((s, d) => s + d.doctor_cut_total_iqd, 0)
    } else if (entity === "operators" && operators.data) {
      total = operators.data.reduce((s, o) => s + o.operator_cut_total_iqd, 0)
    } else if (entity === "checks" && checks.data && checks.data.mode === "groups") {
      total = checks.data.totals.revenue_iqd
    } else if (entity === "visits" && visits.data && visits.data.mode === "rows") {
      total = visits.data.totals.revenue_iqd
    }
    return formatIqd(total, { locale, withSuffix: true })
  }, [entity, doctors.data, operators.data, checks.data, visits.data, locale])

  return { rows: filtered, isLoading: all.isLoading, sortLabel, footTotal }
}
