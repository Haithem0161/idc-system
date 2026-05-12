import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  invoke,
  isTauri,
  type DailyCloseRecord,
  type DashboardKpisRecord,
  type DashboardTopsRecord,
  type DoctorDrilldownRecord,
  type DoctorEarningsRecord,
  type OperatorDrilldownRecord,
  type OperatorEarningsRecord,
  type ReportsRangeArgs,
  type ReportsVisitsArgs,
  type VisitsReportRecord,
} from "@/lib/ipc"

export const reportsKeys = {
  all: ["reports"] as const,
  dashboard: (range: ReportsRangeArgs) => [...reportsKeys.all, "dashboard", range] as const,
  tops: (range: ReportsRangeArgs) => [...reportsKeys.all, "tops", range] as const,
  visits: (filters: ReportsVisitsArgs) => [...reportsKeys.all, "visits", filters] as const,
  doctors: (range: ReportsRangeArgs) => [...reportsKeys.all, "doctors", range] as const,
  doctor: (id: string | null, range: ReportsRangeArgs) =>
    [...reportsKeys.all, "doctor", id ?? "__house__", range] as const,
  operators: (range: ReportsRangeArgs) => [...reportsKeys.all, "operators", range] as const,
  operator: (id: string, range: ReportsRangeArgs) =>
    [...reportsKeys.all, "operator", id, range] as const,
  dailyClose: (date: string) => [...reportsKeys.all, "dailyClose", date] as const,
}

export function useDashboardKpis (range: ReportsRangeArgs) {
  return useQuery<DashboardKpisRecord>({
    queryKey: reportsKeys.dashboard(range),
    enabled: isTauri(),
    queryFn: () => invoke("reports_dashboard_kpis", { args: range }),
    staleTime: 30_000,
  })
}

export function useDashboardTops (range: ReportsRangeArgs) {
  return useQuery<DashboardTopsRecord>({
    queryKey: reportsKeys.tops(range),
    enabled: isTauri(),
    queryFn: () => invoke("reports_dashboard_tops", { args: range }),
    staleTime: 30_000,
  })
}

export function useVisitsReport (filters: ReportsVisitsArgs) {
  return useQuery<VisitsReportRecord>({
    queryKey: reportsKeys.visits(filters),
    enabled: isTauri(),
    queryFn: () => invoke("reports_visits", { args: filters }),
    staleTime: 15_000,
  })
}

export function useDoctorEarnings (range: ReportsRangeArgs) {
  return useQuery<DoctorEarningsRecord[]>({
    queryKey: reportsKeys.doctors(range),
    enabled: isTauri(),
    queryFn: () => invoke("reports_doctor_earnings", { args: range }),
    staleTime: 30_000,
  })
}

export function useDoctorDrilldown (
  doctorId: string | null,
  range: ReportsRangeArgs
) {
  return useQuery<DoctorDrilldownRecord>({
    queryKey: reportsKeys.doctor(doctorId, range),
    enabled: isTauri(),
    queryFn: () =>
      invoke("reports_doctor_drilldown", {
        args: {
          doctor_id: doctorId,
          from_utc: range.from_utc,
          to_utc: range.to_utc,
          include_voided: range.include_voided,
        },
      }),
    staleTime: 30_000,
  })
}

export function useOperatorEarnings (range: ReportsRangeArgs) {
  return useQuery<OperatorEarningsRecord[]>({
    queryKey: reportsKeys.operators(range),
    enabled: isTauri(),
    queryFn: () => invoke("reports_operator_earnings", { args: range }),
    staleTime: 30_000,
  })
}

export function useOperatorDrilldown (
  operatorId: string | null,
  range: ReportsRangeArgs
) {
  return useQuery<OperatorDrilldownRecord>({
    queryKey: reportsKeys.operator(operatorId ?? "", range),
    enabled: isTauri() && Boolean(operatorId),
    queryFn: () =>
      invoke("reports_operator_drilldown", {
        args: {
          operator_id: operatorId!,
          from_utc: range.from_utc,
          to_utc: range.to_utc,
          include_voided: range.include_voided,
        },
      }),
    staleTime: 30_000,
  })
}

export function useDailyClose (date: string | null | undefined) {
  return useQuery<DailyCloseRecord>({
    queryKey: reportsKeys.dailyClose(date ?? ""),
    enabled: isTauri() && Boolean(date),
    queryFn: () => invoke("reports_daily_close", { args: { date: date! } }),
    staleTime: 5_000,
  })
}

export function useDailyCloseRerun () {
  const qc = useQueryClient()
  return useMutation<DailyCloseRecord, Error, { date: string }>({
    mutationFn: ({ date }) => invoke("reports_daily_close", { args: { date } }),
    onSuccess: (data) => {
      qc.setQueryData(reportsKeys.dailyClose(data.target_date), data)
    },
  })
}

export function useExportVisitsCsv () {
  return useMutation<
    { path: string },
    Error,
    { filters: ReportsVisitsArgs; path: string }
  >({
    mutationFn: ({ filters, path }) =>
      invoke("reports_export_visits_csv", { args: { filters, path } }),
  })
}

export function useExportDoctorsCsv () {
  return useMutation<
    { path: string },
    Error,
    { from_utc: string; to_utc: string; include_voided?: boolean; path: string }
  >({
    mutationFn: (input) => invoke("reports_export_doctors_csv", { args: input }),
  })
}

export function useExportOperatorsCsv () {
  return useMutation<
    { path: string },
    Error,
    { from_utc: string; to_utc: string; include_voided?: boolean; path: string }
  >({
    mutationFn: (input) => invoke("reports_export_operators_csv", { args: input }),
  })
}

export function useExportDailyClosePdf () {
  return useMutation<{ path: string }, Error, { date: string; path: string }>({
    mutationFn: (input) => invoke("reports_export_daily_close_pdf", { args: input }),
  })
}
