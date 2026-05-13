import type { SyncEntityStore } from '../../../sync/domain/sync-store'
import type {
  VisitSyncRecord,
  OperatorShiftSyncRecord,
} from '../../../sync/infrastructure/memory/store'

/**
 * Reports query service.
 *
 * Phase-7 v1 exposes `/reports/visits` and `/reports/daily-close/:date`.
 * Phase-09 follow-up: the service now reads via `SyncEntityStore`, so
 * production runs against Prisma and tests run against the memory store —
 * a single in-memory rollup path on top of either backend.
 */
export class ReportsService {
  constructor (private readonly store: SyncEntityStore) {}

  /**
   * Visits report -- §3 Server + §7.24. Filters by tenant + status set +
   * date window + optional check_type / subtype / doctor / operator / dye /
   * report. Supports row or grouped responses depending on `groupBy`.
   */
  async visits (params: VisitsReportParams): Promise<VisitsReportResponse> {
    const visits = await this.store.listAllVisits(params.tenantId)
    const matches = filterVisits(visits, params)
    const totals = sumTotals(matches)
    if (!params.groupBy || params.groupBy === 'none') {
      const rows = matches
        .slice(0, params.limit ?? 1000)
        .map(toVisitRow)
      return { mode: 'rows', rows, totals }
    }
    const groups = groupVisits(matches, params.groupBy)
    return { mode: 'groups', groups, totals }
  }

  /**
   * Daily close authoritative computation (§7.8 / §7.9). The server is the
   * source of truth for cross-90-day reconciliation; the desktop falls back
   * here when the local DB doesn't have the day (offline period).
   */
  async dailyClose (
    tenantId: string,
    date: string,
    tzOffsetMinutes: number
  ): Promise<DailyCloseResponse> {
    const [from, to] = utcWindowForLocalDay(date, tzOffsetMinutes)
    const fromIso = from.toISOString()
    const toIso = to.toISOString()

    const [visits, adjustments, shifts] = await Promise.all([
      this.store.listAllVisits(tenantId),
      this.store.listAllInventoryAdjustments(tenantId),
      this.store.listAllOperatorShifts(tenantId),
    ])

    const matched = visits.filter((v) =>
      v.status === 'locked'
      && v.locked_at != null
      && v.locked_at >= fromIso
      && v.locked_at < toIso
    )
    const voidedMatched = visits.filter((v) =>
      v.status === 'voided'
      && v.locked_at != null
      && v.locked_at >= fromIso
      && v.locked_at < toIso
    )

    const total_revenue_iqd = sumField(matched, 'total_amount_iqd_snapshot')
    const total_doctor_cuts_iqd = sumField(matched, 'doctor_cut_snapshot_iqd')
    const total_operator_cuts_iqd = sumField(matched, 'operator_cut_snapshot_iqd')

    // Inventory consumption: SUM(-delta) for consume_visit rows in window.
    let inv = 0
    for (const adj of adjustments) {
      if (adj.reason !== 'consume_visit') continue
      if (adj.created_at < fromIso || adj.created_at >= toIso) continue
      inv += -adj.delta
    }
    const total_inventory_consumption_value_iqd = inv
    const net_iqd = total_revenue_iqd - total_doctor_cuts_iqd - total_operator_cuts_iqd - inv

    const voided_value_iqd = sumField(voidedMatched, 'total_amount_iqd_snapshot')

    // Per-doctor breakdown.
    const per_doctor = aggregateBy(matched, (v) => v.doctor_id ?? '__house__', (rows) => {
      const first = rows[0]
      return {
        doctor_id: first.doctor_id,
        name: first.doctor_name_snapshot ?? '(house)',
        visits: rows.length,
        revenue_iqd: sumField(rows, 'total_amount_iqd_snapshot'),
        doctor_cut_iqd: sumField(rows, 'doctor_cut_snapshot_iqd'),
      }
    })

    const per_operator = aggregateBy(matched, (v) => v.operator_id ?? '', (rows) => {
      const first = rows[0]
      // Hours from operator_shifts overlapping the window.
      const millis = sumShiftMillis(shifts, first.operator_id ?? '', from, to)
      return {
        operator_id: first.operator_id ?? '',
        name: first.operator_name_snapshot ?? '',
        visits: rows.length,
        dye_visits: rows.filter((r) => r.dye).length,
        operator_cut_iqd: sumField(rows, 'operator_cut_snapshot_iqd'),
        hours_on_shift_milli: millis,
      }
    }).filter((r) => r.operator_id !== '')

    const per_check_type = aggregateBy(matched, (v) => v.check_type_id, (rows) => {
      const first = rows[0]
      return {
        check_type_id: first.check_type_id,
        name_ar: first.check_type_name_ar_snapshot ?? '',
        name_en: first.check_type_name_en_snapshot ?? null,
        visits: rows.length,
        revenue_iqd: sumField(rows, 'total_amount_iqd_snapshot'),
        doctor_cut_iqd: sumField(rows, 'doctor_cut_snapshot_iqd'),
        operator_cut_iqd: sumField(rows, 'operator_cut_snapshot_iqd'),
      }
    })

    return {
      tenant_id: tenantId,
      target_date: date,
      tz_offset: formatOffset(tzOffsetMinutes),
      total_revenue_iqd,
      total_doctor_cuts_iqd,
      total_operator_cuts_iqd,
      total_inventory_consumption_value_iqd,
      net_iqd,
      locked_count: matched.length,
      voided_count: voidedMatched.length,
      voided_value_iqd,
      per_doctor,
      per_operator,
      per_check_type,
      generated_at: new Date().toISOString(),
    }
  }
}

function filterVisits (visits: VisitSyncRecord[], params: VisitsReportParams): VisitSyncRecord[] {
  const includeStatuses = params.statuses && params.statuses.length > 0
    ? new Set(params.statuses)
    : (params.includeVoided ? new Set(['locked', 'voided']) : new Set(['locked']))
  return visits.filter((v) => {
    if (!includeStatuses.has(v.status)) return false
    if (v.locked_at == null) return false
    if (v.locked_at < params.from || v.locked_at >= params.to) return false
    if (params.checkTypeIds && params.checkTypeIds.length > 0 &&
        !params.checkTypeIds.includes(v.check_type_id)) return false
    if (params.subtypeIds && params.subtypeIds.length > 0 &&
        (v.check_subtype_id == null || !params.subtypeIds.includes(v.check_subtype_id))) return false
    if (params.doctorIds && params.doctorIds.length > 0) {
      const allow = v.doctor_id != null && params.doctorIds.includes(v.doctor_id)
      const houseHit = params.includeHouse === true && v.doctor_id == null
      if (!allow && !houseHit) return false
    } else if (params.includeHouse === false && v.doctor_id == null) {
      return false
    }
    if (params.operatorIds && params.operatorIds.length > 0 &&
        (v.operator_id == null || !params.operatorIds.includes(v.operator_id))) return false
    if (params.dye === 'y' && !v.dye) return false
    if (params.dye === 'n' && v.dye) return false
    if (params.report === 'y' && !v.report) return false
    if (params.report === 'n' && v.report) return false
    return true
  })
}

export interface VisitsReportParams {
  tenantId: string
  from: string
  to: string
  includeVoided?: boolean
  statuses?: string[]
  checkTypeIds?: string[]
  subtypeIds?: string[]
  doctorIds?: string[]
  operatorIds?: string[]
  includeHouse?: boolean
  dye?: 'y' | 'n' | 'all'
  report?: 'y' | 'n' | 'all'
  groupBy?: 'none' | 'by_date' | 'by_doctor' | 'by_operator' | 'by_check_type' | 'by_subtype' | 'by_status'
  limit?: number
}

export interface VisitsReportRow {
  visit_id: string
  locked_at: string | null
  status: string
  patient_name: string
  doctor_name: string | null
  operator_name: string
  check_type_name_ar: string
  check_type_name_en: string | null
  check_subtype_name_ar: string | null
  check_subtype_name_en: string | null
  dye: boolean
  report: boolean
  price_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  net_iqd: number
}

export interface VisitsReportGroup {
  key: string
  label: string
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  net_iqd: number
}

export interface VisitsReportTotals {
  visits: number
  revenue_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  net_iqd: number
}

export type VisitsReportResponse =
  | { mode: 'rows', rows: VisitsReportRow[], totals: VisitsReportTotals }
  | { mode: 'groups', groups: VisitsReportGroup[], totals: VisitsReportTotals }

export interface DailyCloseResponse {
  tenant_id: string
  target_date: string
  tz_offset: string
  total_revenue_iqd: number
  total_doctor_cuts_iqd: number
  total_operator_cuts_iqd: number
  total_inventory_consumption_value_iqd: number
  net_iqd: number
  locked_count: number
  voided_count: number
  voided_value_iqd: number
  per_doctor: Array<{
    doctor_id: string | null
    name: string
    visits: number
    revenue_iqd: number
    doctor_cut_iqd: number
  }>
  per_operator: Array<{
    operator_id: string
    name: string
    visits: number
    dye_visits: number
    operator_cut_iqd: number
    hours_on_shift_milli: number
  }>
  per_check_type: Array<{
    check_type_id: string
    name_ar: string
    name_en: string | null
    visits: number
    revenue_iqd: number
    doctor_cut_iqd: number
    operator_cut_iqd: number
  }>
  generated_at: string
}

// ---- helpers --------------------------------------------------------------

function toVisitRow (v: VisitSyncRecord): VisitsReportRow {
  const price = v.price_snapshot_iqd ?? 0
  const dc = v.doctor_cut_snapshot_iqd ?? 0
  const oc = v.operator_cut_snapshot_iqd ?? 0
  const total = v.total_amount_iqd_snapshot ?? price
  return {
    visit_id: v.id,
    locked_at: v.locked_at,
    status: v.status,
    patient_name: v.patient_name_snapshot ?? '',
    doctor_name: v.doctor_name_snapshot,
    operator_name: v.operator_name_snapshot ?? '',
    check_type_name_ar: v.check_type_name_ar_snapshot ?? '',
    check_type_name_en: v.check_type_name_en_snapshot,
    check_subtype_name_ar: v.check_subtype_name_ar_snapshot,
    check_subtype_name_en: v.check_subtype_name_en_snapshot,
    dye: v.dye,
    report: v.report,
    price_iqd: price,
    doctor_cut_iqd: dc,
    operator_cut_iqd: oc,
    net_iqd: total - dc - oc,
  }
}

function sumField (rows: VisitSyncRecord[], field: keyof VisitSyncRecord): number {
  let s = 0
  for (const r of rows) {
    const v = r[field]
    if (typeof v === 'number') s += v
  }
  return s
}

function sumTotals (rows: VisitSyncRecord[]): VisitsReportTotals {
  let visits = 0
  let revenue = 0
  let dc = 0
  let oc = 0
  let net = 0
  for (const r of rows) {
    visits += 1
    const price = r.price_snapshot_iqd ?? 0
    const total = r.total_amount_iqd_snapshot ?? price
    const cutD = r.doctor_cut_snapshot_iqd ?? 0
    const cutO = r.operator_cut_snapshot_iqd ?? 0
    revenue += price
    dc += cutD
    oc += cutO
    net += total - cutD - cutO
  }
  return { visits, revenue_iqd: revenue, doctor_cut_iqd: dc, operator_cut_iqd: oc, net_iqd: net }
}

function groupVisits (rows: VisitSyncRecord[], groupBy: NonNullable<VisitsReportParams['groupBy']>): VisitsReportGroup[] {
  const keyFn = (v: VisitSyncRecord): { key: string, label: string } => {
    switch (groupBy) {
      case 'by_date': return { key: (v.locked_at ?? '').slice(0, 10), label: (v.locked_at ?? '').slice(0, 10) }
      case 'by_doctor':
        return v.doctor_id != null
          ? { key: v.doctor_id, label: v.doctor_name_snapshot ?? v.doctor_id }
          : { key: '__house__', label: '(house)' }
      case 'by_operator':
        return v.operator_id != null
          ? { key: v.operator_id, label: v.operator_name_snapshot ?? v.operator_id }
          : { key: '', label: '' }
      case 'by_check_type':
        return { key: v.check_type_id, label: v.check_type_name_en_snapshot ?? v.check_type_name_ar_snapshot ?? v.check_type_id }
      case 'by_subtype':
        return v.check_subtype_id != null
          ? { key: v.check_subtype_id, label: v.check_subtype_name_en_snapshot ?? v.check_subtype_name_ar_snapshot ?? v.check_subtype_id }
          : { key: '', label: '' }
      case 'by_status':
        return { key: v.status, label: v.status }
      case 'none':
      default: return { key: '', label: '' }
    }
  }
  const buckets = new Map<string, { label: string, rows: VisitSyncRecord[] }>()
  for (const r of rows) {
    const { key, label } = keyFn(r)
    const b = buckets.get(key) ?? { label, rows: [] }
    b.rows.push(r)
    buckets.set(key, b)
  }
  const out: VisitsReportGroup[] = []
  for (const [key, { label, rows: rs }] of buckets.entries()) {
    const totals = sumTotals(rs)
    out.push({
      key,
      label,
      visits: totals.visits,
      revenue_iqd: totals.revenue_iqd,
      doctor_cut_iqd: totals.doctor_cut_iqd,
      operator_cut_iqd: totals.operator_cut_iqd,
      net_iqd: totals.net_iqd,
    })
  }
  out.sort((a, b) => b.revenue_iqd - a.revenue_iqd || a.label.localeCompare(b.label))
  return out
}

function aggregateBy<TOut> (
  rows: VisitSyncRecord[],
  keyFn: (v: VisitSyncRecord) => string,
  reduce: (rows: VisitSyncRecord[]) => TOut
): TOut[] {
  const buckets = new Map<string, VisitSyncRecord[]>()
  for (const r of rows) {
    const k = keyFn(r)
    const list = buckets.get(k) ?? []
    list.push(r)
    buckets.set(k, list)
  }
  const out: TOut[] = []
  for (const list of buckets.values()) {
    out.push(reduce(list))
  }
  return out
}

function utcWindowForLocalDay (date: string, offsetMinutes: number): [Date, Date] {
  // `date` is YYYY-MM-DD in the local tz; convert to a UTC range.
  const [y, m, d] = date.split('-').map((n) => parseInt(n, 10))
  // Local midnight in UTC = (UTC) - offset.
  const start = new Date(Date.UTC(y, m - 1, d, 0, 0, 0))
  start.setUTCMinutes(start.getUTCMinutes() - offsetMinutes)
  const end = new Date(start.getTime() + 24 * 3600 * 1000)
  return [start, end]
}

function formatOffset (mins: number): string {
  const sign = mins >= 0 ? '+' : '-'
  const abs = Math.abs(mins)
  const h = Math.floor(abs / 60).toString().padStart(2, '0')
  const m = (abs % 60).toString().padStart(2, '0')
  return `${sign}${h}:${m}`
}

function sumShiftMillis (
  shifts: OperatorShiftSyncRecord[],
  operatorId: string,
  from: Date,
  to: Date
): number {
  let total = 0
  for (const s of shifts) {
    if (s.operator_id !== operatorId) continue
    const checkIn = new Date(s.check_in_at).getTime()
    const checkOut = s.check_out_at != null ? new Date(s.check_out_at).getTime() : to.getTime()
    if (checkOut <= from.getTime()) continue
    if (checkIn >= to.getTime()) continue
    const lo = Math.max(checkIn, from.getTime())
    const hi = Math.min(checkOut, to.getTime())
    if (hi > lo) total += hi - lo
  }
  return total
}

