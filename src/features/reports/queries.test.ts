// Phase-07 §2.4 React Query hook tests for the reports feature surface.
// Every test runs in both `dir=ltr` and `dir=rtl` per the plan's RTL
// invariant (`.claude/rules/testing.md` §14 anti-pattern row "RTL never
// tested").

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
  }
})

import { invoke, isTauri } from "@/lib/ipc"
import type {
  DailyCloseRecord,
  DashboardKpisRecord,
  DashboardTopsRecord,
  DoctorDrilldownRecord,
  DoctorEarningsRecord,
  OperatorDrilldownRecord,
  OperatorEarningsRecord,
  ReportsRangeArgs,
  TrendCellRecord,
  TrendMatrixRecord,
  VisitsReportRecord,
} from "@/lib/ipc"
import {
  reportsKeys,
  useDailyClose,
  useDailyCloseRerun,
  useDashboardKpis,
  useDashboardTops,
  useDoctorDrilldown,
  useDoctorEarnings,
  useExportDailyClosePdf,
  useExportDoctorsCsv,
  useExportOperatorsCsv,
  useExportVisitsCsv,
  useOperatorDrilldown,
  useOperatorEarnings,
  useVisitsReport,
} from "@/features/reports/queries"

const directions = [["ltr"], ["rtl"]] as const

function makeWrapper(): {
  wrapper: (props: { children: ReactNode }) => ReturnType<typeof createElement>
  client: QueryClient
} {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children)
  return { wrapper, client }
}

const UUID_DOCTOR = "0190f3a0-f1c0-7000-8000-0000000a0001"
const UUID_OPERATOR = "0190f3a0-f1c0-7000-8000-0000000a0002"
const UUID_CHECK_TYPE = "0190f3a0-f1c0-7000-8000-0000000a0003"
const RANGE: ReportsRangeArgs = {
  from_utc: "2026-05-01T00:00:00Z",
  to_utc: "2026-05-13T00:00:00Z",
}

function trendCell(over: Partial<TrendCellRecord> = {}): TrendCellRecord {
  return {
    current_iqd: 100_000,
    prior_iqd: 80_000,
    delta_iqd: 20_000,
    delta_permille: 250,
    ...over,
  }
}

function trendMatrix(): TrendMatrixRecord {
  return {
    revenue: trendCell(),
    doctor_cuts: trendCell(),
    operator_cuts: trendCell(),
    inventory_value: trendCell(),
    net: trendCell(),
  }
}

function kpis(over: Partial<DashboardKpisRecord> = {}): DashboardKpisRecord {
  return {
    range_from: RANGE.from_utc,
    range_to: RANGE.to_utc,
    revenue_iqd: 200_000,
    doctor_cuts_iqd: 60_000,
    operator_cuts_iqd: 20_000,
    inventory_consumption_value_iqd: 4_000,
    net_iqd: 116_000,
    trend_today_vs_yesterday: trendMatrix(),
    trend_week_vs_last_week: trendMatrix(),
    trend_month_vs_last_month: trendMatrix(),
    ...over,
  }
}

function doctorEarnings(
  over: Partial<DoctorEarningsRecord> = {}
): DoctorEarningsRecord {
  return {
    doctor_id: UUID_DOCTOR,
    name: "Dr Ali",
    specialty: "Cardio",
    visits: 5,
    revenue_iqd: 250_000,
    doctor_cut_total_iqd: 75_000,
    avg_cut_per_visit_iqd: 15_000,
    ...over,
  }
}

function operatorEarnings(
  over: Partial<OperatorEarningsRecord> = {}
): OperatorEarningsRecord {
  return {
    operator_id: UUID_OPERATOR,
    name: "Kareem",
    visits: 6,
    visits_with_dye: 2,
    operator_cut_total_iqd: 24_000,
    hours_on_shift_milli: 8 * 3_600_000,
    avg_cut_per_hour_iqd: 3_000,
    ...over,
  }
}

function tops(): DashboardTopsRecord {
  return {
    top_doctors: [doctorEarnings()],
    top_operators: [operatorEarnings()],
    top_check_types: [
      {
        check_type_id: UUID_CHECK_TYPE,
        name_ar: "موجات",
        name_en: "US",
        visits: 8,
        revenue_iqd: 400_000,
        doctor_cut_iqd: 120_000,
        operator_cut_iqd: 32_000,
      },
    ],
  }
}

function visitsRows(): VisitsReportRecord {
  return {
    mode: "rows",
    rows: [
      {
        visit_id: "0190f3a0-f1c0-7000-8000-0000000f0001",
        locked_at: "2026-05-12T10:00:00Z",
        status: "locked",
        patient_name: "Sara",
        check_type_name_ar: "ت",
        check_type_name_en: "T",
        check_subtype_name_ar: null,
        check_subtype_name_en: null,
        doctor_name: "Dr Ali",
        operator_name: "Kareem",
        dye: false,
        report: false,
        price_iqd: 50_000,
        doctor_cut_iqd: 15_000,
        operator_cut_iqd: 4_000,
        total_iqd: 50_000,
        amount_paid_override_iqd: null,
        net_iqd: 31_000,
      },
    ],
    totals: {
      visits: 1,
      revenue_iqd: 50_000,
      doctor_cut_iqd: 15_000,
      operator_cut_iqd: 4_000,
      net_iqd: 31_000,
    },
  }
}

function visitsGroups(): VisitsReportRecord {
  return {
    mode: "groups",
    groups: [
      {
        key: UUID_DOCTOR,
        label: "Dr Ali",
        visits: 5,
        revenue_iqd: 250_000,
        doctor_cut_iqd: 75_000,
        operator_cut_iqd: 20_000,
        net_iqd: 155_000,
      },
    ],
    totals: {
      visits: 5,
      revenue_iqd: 250_000,
      doctor_cut_iqd: 75_000,
      operator_cut_iqd: 20_000,
      net_iqd: 155_000,
    },
  }
}

function dailyClose(over: Partial<DailyCloseRecord> = {}): DailyCloseRecord {
  return {
    tenant_id: "tenant-x",
    target_date: "2026-05-13",
    tz_offset: "+03:00",
    total_revenue_iqd: 150_000,
    total_collected_iqd: 150_000,
    total_discount_iqd: 0,
    total_doctor_cuts_iqd: 45_000,
    total_operator_cuts_iqd: 12_000,
    total_inventory_consumption_value_iqd: 3_000,
    net_iqd: 90_000,
    locked_count: 3,
    voided_count: 1,
    voided_value_iqd: 50_000,
    per_doctor: [],
    per_operator: [],
    per_check_type: [],
    pending_sync: 0,
    provisional: false,
    input_hash: "abcdef1234567890".padEnd(64, "0"),
    generated_at: "2026-05-13T15:00:00Z",
    ...over,
  }
}

function doctorDrilldown(): DoctorDrilldownRecord {
  return {
    doctor_id: UUID_DOCTOR,
    name: "Dr Ali",
    specialty: "Cardio",
    per_check: [
      {
        check_type_id: UUID_CHECK_TYPE,
        check_type_name_ar: "ت",
        check_type_name_en: "T",
        check_subtype_id: null,
        check_subtype_name_ar: null,
        check_subtype_name_en: null,
        visits: 3,
        revenue_iqd: 150_000,
        doctor_cut_iqd: 45_000,
        avg_cut_iqd: 15_000,
      },
    ],
    source_visits: [],
    totals: {
      visits: 3,
      revenue_iqd: 150_000,
      doctor_cut_iqd: 45_000,
      operator_cut_iqd: 12_000,
      net_iqd: 93_000,
    },
  }
}

function operatorDrilldown(): OperatorDrilldownRecord {
  return {
    operator_id: UUID_OPERATOR,
    name: "Kareem",
    shifts: [],
    attributed_visits: [],
    totals: {
      visits: 0,
      revenue_iqd: 0,
      doctor_cut_iqd: 0,
      operator_cut_iqd: 0,
      net_iqd: 0,
    },
    total_hours_milli: 0,
  }
}

const mockOnce = (value: unknown) => {
  vi.mocked(invoke).mockResolvedValueOnce(value as never)
}

describe.each(directions)(
  "Phase-07 §2.4 reports hooks (dir=%s)",
  (dir) => {
    beforeEach(() => {
      document.documentElement.dir = dir
      vi.mocked(invoke).mockReset()
      vi.mocked(isTauri).mockReturnValue(true)
    })

    afterEach(() => {
      document.documentElement.dir = ""
    })

    it("reportsKeys exposes the documented cache key shapes", () => {
      expect(reportsKeys.all).toEqual(["reports"])
      expect(reportsKeys.dashboard(RANGE)).toEqual([
        "reports",
        "dashboard",
        RANGE,
      ])
      expect(reportsKeys.tops(RANGE)).toEqual(["reports", "tops", RANGE])
      expect(
        reportsKeys.visits({ from_utc: RANGE.from_utc, to_utc: RANGE.to_utc })
      ).toEqual([
        "reports",
        "visits",
        { from_utc: RANGE.from_utc, to_utc: RANGE.to_utc },
      ])
      expect(reportsKeys.doctors(RANGE)).toEqual([
        "reports",
        "doctors",
        RANGE,
      ])
      expect(reportsKeys.operators(RANGE)).toEqual([
        "reports",
        "operators",
        RANGE,
      ])
      expect(reportsKeys.dailyClose("2026-05-13")).toEqual([
        "reports",
        "dailyClose",
        "2026-05-13",
      ])
    })

    it("reportsKeys.doctor uses __house__ token when doctorId is null", () => {
      const k_house = reportsKeys.doctor(null, RANGE)
      const k_doc = reportsKeys.doctor(UUID_DOCTOR, RANGE)
      expect(k_house).toEqual(["reports", "doctor", "__house__", RANGE])
      expect(k_doc).toEqual(["reports", "doctor", UUID_DOCTOR, RANGE])
      expect(k_house).not.toEqual(k_doc)
    })

    it("useDashboardKpis dispatches reports_dashboard_kpis and caches under range key", async () => {
      const fixture = kpis()
      mockOnce(fixture)
      const { wrapper, client } = makeWrapper()
      const { result } = renderHook(() => useDashboardKpis(RANGE), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_dashboard_kpis", {
        args: RANGE,
      })
      expect(client.getQueryData(reportsKeys.dashboard(RANGE))).toEqual(fixture)
    })

    it("useDashboardKpis is disabled outside Tauri", () => {
      vi.mocked(isTauri).mockReturnValue(false)
      const { wrapper } = makeWrapper()
      renderHook(() => useDashboardKpis(RANGE), { wrapper })
      expect(invoke).not.toHaveBeenCalled()
    })

    it("useDashboardTops dispatches reports_dashboard_tops", async () => {
      mockOnce(tops())
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useDashboardTops(RANGE), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_dashboard_tops", {
        args: RANGE,
      })
      expect(result.current.data?.top_doctors).toHaveLength(1)
      expect(result.current.data?.top_operators).toHaveLength(1)
      expect(result.current.data?.top_check_types).toHaveLength(1)
    })

    it("useVisitsReport returns rows-mode payload", async () => {
      mockOnce(visitsRows())
      const { wrapper } = makeWrapper()
      const filters = { from_utc: RANGE.from_utc, to_utc: RANGE.to_utc }
      const { result } = renderHook(() => useVisitsReport(filters), {
        wrapper,
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data?.mode).toBe("rows")
      if (result.current.data?.mode === "rows") {
        expect(result.current.data.rows).toHaveLength(1)
        expect(result.current.data.totals.visits).toBe(1)
      }
    })

    it("useVisitsReport returns groups-mode payload when group_by is set", async () => {
      mockOnce(visitsGroups())
      const { wrapper } = makeWrapper()
      const filters = {
        from_utc: RANGE.from_utc,
        to_utc: RANGE.to_utc,
        group_by: "by_doctor" as const,
      }
      const { result } = renderHook(() => useVisitsReport(filters), {
        wrapper,
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data?.mode).toBe("groups")
    })

    it("useDoctorEarnings returns the array of earnings rows", async () => {
      mockOnce([doctorEarnings(), doctorEarnings({ name: "Dr B" })])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useDoctorEarnings(RANGE), {
        wrapper,
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data).toHaveLength(2)
      expect(invoke).toHaveBeenCalledWith("reports_doctor_earnings", {
        args: RANGE,
      })
    })

    it("useDoctorDrilldown threads doctor_id through to the IPC", async () => {
      mockOnce(doctorDrilldown())
      const { wrapper } = makeWrapper()
      const { result } = renderHook(
        () => useDoctorDrilldown(UUID_DOCTOR, RANGE),
        { wrapper }
      )
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_doctor_drilldown", {
        args: {
          doctor_id: UUID_DOCTOR,
          from_utc: RANGE.from_utc,
          to_utc: RANGE.to_utc,
          include_voided: undefined,
        },
      })
    })

    it("useDoctorDrilldown passes null doctor_id for the house pseudo-row", async () => {
      mockOnce(doctorDrilldown())
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useDoctorDrilldown(null, RANGE), {
        wrapper,
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_doctor_drilldown", {
        args: {
          doctor_id: null,
          from_utc: RANGE.from_utc,
          to_utc: RANGE.to_utc,
          include_voided: undefined,
        },
      })
    })

    it("useOperatorEarnings dispatches reports_operator_earnings", async () => {
      mockOnce([operatorEarnings()])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useOperatorEarnings(RANGE), {
        wrapper,
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_operator_earnings", {
        args: RANGE,
      })
      expect(result.current.data?.[0]?.hours_on_shift_milli).toBeGreaterThan(0)
    })

    it("useOperatorDrilldown is disabled when operatorId is null", () => {
      const { wrapper } = makeWrapper()
      renderHook(() => useOperatorDrilldown(null, RANGE), { wrapper })
      expect(invoke).not.toHaveBeenCalled()
    })

    it("useOperatorDrilldown dispatches with operator_id when present", async () => {
      mockOnce(operatorDrilldown())
      const { wrapper } = makeWrapper()
      const { result } = renderHook(
        () => useOperatorDrilldown(UUID_OPERATOR, RANGE),
        { wrapper }
      )
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_operator_drilldown", {
        args: {
          operator_id: UUID_OPERATOR,
          from_utc: RANGE.from_utc,
          to_utc: RANGE.to_utc,
          include_voided: undefined,
        },
      })
    })

    it("useDailyClose dispatches when a date is provided", async () => {
      mockOnce(dailyClose())
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useDailyClose("2026-05-13"), {
        wrapper,
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_daily_close", {
        args: { date: "2026-05-13" },
      })
      expect(result.current.data?.tz_offset).toBe("+03:00")
    })

    it("useDailyClose is disabled when date is null", () => {
      const { wrapper } = makeWrapper()
      renderHook(() => useDailyClose(null), { wrapper })
      expect(invoke).not.toHaveBeenCalled()
    })

    it("useDailyCloseRerun mutation re-fires the IPC and returns fresh artifact", async () => {
      const fresh = dailyClose({
        target_date: "2026-05-13",
        input_hash: "ffeeddccbbaa9988".padEnd(64, "1"),
      })
      mockOnce(fresh)
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useDailyCloseRerun(), { wrapper })
      result.current.mutate({ date: "2026-05-13" })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_daily_close", {
        args: { date: "2026-05-13" },
      })
      // Mutation success returns the fresh artifact.
      expect(result.current.data).toEqual(fresh)
    })

    it("useExportVisitsCsv mutation calls reports_export_visits_csv with path", async () => {
      mockOnce({ path: "/exports/visits.csv" })
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useExportVisitsCsv(), { wrapper })
      result.current.mutate({
        filters: { from_utc: RANGE.from_utc, to_utc: RANGE.to_utc },
        path: "/exports/visits.csv",
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_export_visits_csv", {
        args: {
          filters: { from_utc: RANGE.from_utc, to_utc: RANGE.to_utc },
          path: "/exports/visits.csv",
        },
      })
    })

    it("useExportDoctorsCsv mutation calls reports_export_doctors_csv with range + path", async () => {
      mockOnce({ path: "/exports/doctors.csv" })
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useExportDoctorsCsv(), { wrapper })
      result.current.mutate({
        from_utc: RANGE.from_utc,
        to_utc: RANGE.to_utc,
        path: "/exports/doctors.csv",
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_export_doctors_csv", {
        args: {
          from_utc: RANGE.from_utc,
          to_utc: RANGE.to_utc,
          path: "/exports/doctors.csv",
        },
      })
    })

    it("useExportOperatorsCsv mutation calls reports_export_operators_csv", async () => {
      mockOnce({ path: "/exports/operators.csv" })
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useExportOperatorsCsv(), { wrapper })
      result.current.mutate({
        from_utc: RANGE.from_utc,
        to_utc: RANGE.to_utc,
        path: "/exports/operators.csv",
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_export_operators_csv", {
        args: {
          from_utc: RANGE.from_utc,
          to_utc: RANGE.to_utc,
          path: "/exports/operators.csv",
        },
      })
    })

    it("useExportDailyClosePdf mutation calls reports_export_daily_close_pdf with date + path", async () => {
      mockOnce({ path: "/exports/daily-close.pdf" })
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useExportDailyClosePdf(), {
        wrapper,
      })
      result.current.mutate({
        date: "2026-05-13",
        path: "/exports/daily-close.pdf",
      })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(invoke).toHaveBeenCalledWith("reports_export_daily_close_pdf", {
        args: { date: "2026-05-13", path: "/exports/daily-close.pdf" },
      })
    })
  }
)
