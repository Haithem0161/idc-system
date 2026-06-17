// Phase-05 §2.4 React Query hook tests for the visits + patients feature
// surface. Each test runs in both `dir=ltr` and `dir=rtl` per the RTL
// invariant (`.claude/rules/testing.md` §14).

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
  ChecksGridCardRecord,
  LockResultRecord,
  PatientRecord,
  QualifiedOperatorRecord,
  ReceiptArtifactsRecord,
  VisitRecord,
} from "@/lib/ipc"
import {
  patientKeys,
  usePatientCreate,
  usePatientSearch,
  useChecksGrid,
  useQualifiedOperators,
  useReceiptReprint,
  useVisit,
  useVisitCreateDraft,
  useVisitDiscard,
  useVisitLock,
  useVisitUpdateDraft,
  useVisitVoid,
  useVisitsTodayByCheck,
  useVisitsWorkspace,
  visitKeys,
} from "@/features/visits/queries"

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

const UUID_VISIT = "0190f3a0-f1c0-7000-8000-0000000a0001"
const UUID_PATIENT = "0190f3a0-f1c0-7000-8000-0000000a0002"
const UUID_CHECK = "0190f3a0-f1c0-7000-8000-0000000a0003"
const UUID_DOCTOR = "0190f3a0-f1c0-7000-8000-0000000a0004"
const UUID_OPERATOR = "0190f3a0-f1c0-7000-8000-0000000a0005"
const UUID_USER = "0190f3a0-f1c0-7000-8000-0000000a0006"

function patient(overrides: Partial<PatientRecord> = {}): PatientRecord {
  return {
    id: UUID_PATIENT,
    name: "Layla",
    phone: null,
    sex: null,
    birth_date: null,
    file_no: null,
    notes: null,
    created_at: "2026-05-14T10:00:00Z",
    updated_at: "2026-05-14T10:00:00Z",
    deleted_at: null,
    version: 1,
    dirty: true,
    entity_id: "tenant-x",
    ...overrides,
  }
}

function visit(overrides: Partial<VisitRecord> = {}): VisitRecord {
  return {
    id: UUID_VISIT,
    patient_id: UUID_PATIENT,
    status: "draft",
    receptionist_user_id: UUID_USER,
    check_type_id: UUID_CHECK,
    check_subtype_id: null,
    doctor_id: UUID_DOCTOR,
    operator_id: null,
    dye: false,
    report: false,
    locked_at: null,
    voided_at: null,
    voided_by_user_id: null,
    void_reason: null,
    snapshots: null,
    created_at: "2026-05-14T10:00:00Z",
    updated_at: "2026-05-14T10:00:00Z",
    deleted_at: null,
    version: 1,
    dirty: true,
    entity_id: "tenant-x",
    ...overrides,
  }
}

function checksGridCard(): ChecksGridCardRecord {
  return {
    check_type_id: UUID_CHECK,
    name_ar: "أشعة",
    name_en: "X-Ray",
    has_subtypes: false,
    dye_supported: true,
    report_supported: false,
    todays_visits: 3,
  }
}

function qualifiedOperator(): QualifiedOperatorRecord {
  return {
    id: UUID_OPERATOR,
    name: "Kareem",
    is_active: true,
  }
}

function receiptArtifacts(): ReceiptArtifactsRecord {
  return {
    a5_path: "/tmp/r/2026/05/a5.pdf",
    thermal_path: "/tmp/r/2026/05/thermal.txt",
  }
}

function lockResult(): LockResultRecord {
  return {
    visit: visit({ status: "locked", locked_at: "2026-05-14T10:00:00Z" }),
    artifacts: receiptArtifacts(),
  }
}

const mockOnce = (value: unknown) => {
  vi.mocked(invoke).mockResolvedValueOnce(value as never)
}

describe.each(directions)(
  "Phase-05 §2.4 visits + patients hooks (dir=%s)",
  (dir) => {
    beforeEach(() => {
      document.documentElement.dir = dir
      vi.mocked(invoke).mockReset()
      vi.mocked(isTauri).mockReturnValue(true)
    })

    afterEach(() => {
      document.documentElement.dir = ""
    })

    // ---- key shape -------------------------------------------------------

    it("visitKeys.todayByCheck segments by check_type_id", () => {
      const a = visitKeys.todayByCheck("a")
      const b = visitKeys.todayByCheck("b")
      expect(a).not.toEqual(b)
      expect(a[2]).toBe("a")
    })

    it("patientKeys.search includes the query in the cache key", () => {
      const a = patientKeys.search("Lay")
      const b = patientKeys.search("Bob")
      expect(a).not.toEqual(b)
    })

    // ---- useChecksGrid ---------------------------------------------------

    it("useChecksGrid resolves to the IPC array under visitKeys.checksGrid", async () => {
      mockOnce([checksGridCard()])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useChecksGrid(), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data?.[0].check_type_id).toBe(UUID_CHECK)
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("visits_checks_grid")
    })

    // ---- useVisitsTodayByCheck ------------------------------------------

    it("useVisitsTodayByCheck disabled when checkTypeId is empty string", () => {
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useVisitsTodayByCheck(""), {
        wrapper,
      })
      expect(result.current.fetchStatus).toBe("idle")
    })

    it("useVisitsTodayByCheck invokes IPC with check_type_id", async () => {
      mockOnce([visit({ status: "locked" })])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(
        () => useVisitsTodayByCheck(UUID_CHECK),
        { wrapper }
      )
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "visits_list_today_by_check",
        { args: { check_type_id: UUID_CHECK } }
      )
    })

    // ---- useVisitsWorkspace --------------------------------------------

    it("useVisitsWorkspace passes filters through to IPC", async () => {
      mockOnce([visit({ status: "draft" })])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(
        () =>
          useVisitsWorkspace(UUID_CHECK, {
            statuses: ["draft"],
            limit: 25,
          }),
        { wrapper }
      )
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("visits_list_workspace", {
        args: {
          check_type_id: UUID_CHECK,
          statuses: ["draft"],
          limit: 25,
        },
      })
    })

    // ---- useVisit -------------------------------------------------------

    it("useVisit calls visits_get with visit_id", async () => {
      mockOnce(visit())
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useVisit(UUID_VISIT), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("visits_get", {
        args: { visit_id: UUID_VISIT },
      })
    })

    // ---- useQualifiedOperators -----------------------------------------

    it("useQualifiedOperators is disabled when checkTypeId is null", () => {
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useQualifiedOperators(null), {
        wrapper,
      })
      expect(result.current.fetchStatus).toBe("idle")
    })

    it("useQualifiedOperators returns the operator array", async () => {
      mockOnce([qualifiedOperator()])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(
        () => useQualifiedOperators(UUID_CHECK),
        { wrapper }
      )
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(result.current.data?.[0].name).toBe("Kareem")
    })

    // ---- useVisitCreateDraft -------------------------------------------

    it("useVisitCreateDraft invalidates visits keys on success", async () => {
      mockOnce(visit())
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { result } = renderHook(() => useVisitCreateDraft(), { wrapper })
      await result.current.mutateAsync({
        patient_id: UUID_PATIENT,
        check_type_id: UUID_CHECK,
        dye: false,
        report: false,
      })
      expect(spy).toHaveBeenCalledWith({ queryKey: visitKeys.all })
    })

    it("useVisitCreateDraft surfaces typed error when IPC rejects", async () => {
      vi.mocked(invoke).mockRejectedValueOnce(new Error("VALIDATION_ERROR"))
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useVisitCreateDraft(), { wrapper })
      await expect(
        result.current.mutateAsync({
          patient_id: UUID_PATIENT,
          check_type_id: UUID_CHECK,
          dye: false,
          report: false,
        })
      ).rejects.toThrow()
    })

    // ---- useVisitUpdateDraft -------------------------------------------

    it("useVisitUpdateDraft invalidates visits keys", async () => {
      mockOnce(visit())
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { result } = renderHook(() => useVisitUpdateDraft(), { wrapper })
      await result.current.mutateAsync({
        visit_id: UUID_VISIT,
        dye: true,
      })
      expect(spy).toHaveBeenCalledWith({ queryKey: visitKeys.all })
    })

    // ---- useVisitDiscard -----------------------------------------------

    it("useVisitDiscard invalidates visits keys on success", async () => {
      mockOnce(null)
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { result } = renderHook(() => useVisitDiscard(), { wrapper })
      await result.current.mutateAsync({ visit_id: UUID_VISIT })
      expect(spy).toHaveBeenCalledWith({ queryKey: visitKeys.all })
    })

    // ---- useVisitLock --------------------------------------------------

    it("useVisitLock invalidates visits keys", async () => {
      mockOnce(lockResult())
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { result } = renderHook(() => useVisitLock(), { wrapper })
      const res = await result.current.mutateAsync({
        visit_id: UUID_VISIT,
        operator_id: UUID_OPERATOR,
      })
      expect(res.visit.status).toBe("locked")
      expect(spy).toHaveBeenCalledWith({ queryKey: visitKeys.all })
    })

    // ---- useVisitVoid --------------------------------------------------

    it("useVisitVoid invokes IPC with visit_id + reason", async () => {
      mockOnce(visit({ status: "voided", void_reason: "wrong patient" }))
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => useVisitVoid(), { wrapper })
      await result.current.mutateAsync({
        visit_id: UUID_VISIT,
        reason: "wrong patient",
      })
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("visits_void", {
        args: { visit_id: UUID_VISIT, reason: "wrong patient" },
      })
    })

    // ---- useReceiptReprint ---------------------------------------------

    it("useReceiptReprint resolves to the paths block without invalidating caches", async () => {
      mockOnce(receiptArtifacts())
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { result } = renderHook(() => useReceiptReprint(), { wrapper })
      const res = await result.current.mutateAsync({ visit_id: UUID_VISIT })
      expect(res.a5_path).toMatch(/\.pdf$/)
      expect(spy).not.toHaveBeenCalled()
    })

    // ---- patient hooks --------------------------------------------------

    it("usePatientSearch invokes IPC with query + limit", async () => {
      mockOnce([patient()])
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => usePatientSearch("Lay"), { wrapper })
      await waitFor(() => expect(result.current.isSuccess).toBe(true))
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("patients_search", {
        args: { query: "Lay", limit: 20 },
      })
    })

    it("usePatientCreate returns the created patient row", async () => {
      mockOnce(patient({ name: "Mariam" }))
      const { wrapper } = makeWrapper()
      const { result } = renderHook(() => usePatientCreate(), { wrapper })
      const created = await result.current.mutateAsync({ name: "Mariam" })
      expect(created.name).toBe("Mariam")
    })
  }
)
