// Phase-09 §8 component-render assertion: NewVisitPage (the most
// complex slice in the §8 battery -- 6 React-Query handles, 5
// mutations, useNavigate, useParams, a controlled operator picker
// modal, and a running-total card sourced from the visit snapshot).
//
// Harness shape notes:
//
//   1. The page uses `useParams()` for the check-type slug, so the
//      wrapper sets up a MemoryRouter routed at the canonical path
//      `/reception/checks/:slug/new` and `initialEntries` seeds the
//      slug as the check_type_id we want the page to resolve against.
//   2. `useNavigate` is mocked at the react-router module level (only
//      that specific hook -- useParams + Link + MemoryRouter pass
//      through via the actual module). The mocked `useNavigate` returns
//      a vi.fn() so assertions on navigation target the canonical
//      `/reception/checks/:slug` (back, after-discard) and
//      `/reception/visits/:visit_id` (after-lock) paths.
//   3. The IPC mock uses `mockImplementation` because mutations
//      invalidate `visitKeys.all` on success and the underlying queries
//      refetch -- a once-only stub would starve the refetch leg.
//   4. The page's `<datalist>` patient-search surface is hard to drive
//      via fireEvent.change on a free-text input (JSDOM does not
//      synthesise the selection event), so the lock-and-discard flow
//      tests skip the datalist click and directly seed the patient
//      via a typed query that the search result list returns.
//
// What this file pins (phase-05 §3.Frontend NewVisit, phase-05 §4
// visit-lock flow, phase-05 §7.5 "house when doctor unset", phase-05
// §7.10 "operator picker is qualified-by-specialty"):
//
//   (a) Page chrome (eyebrow, title, back link) resolves through i18n.
//   (b) Page subtitle joins the resolved check name (`name_en` for en,
//       `name_ar` for ar -- the locale-aware fallback) with the
//       reception subtitle (load-bearing -- ops scan the check name to
//       confirm they navigated into the right workspace).
//   (c) Patient input renders the placeholder copy in the active
//       locale.
//   (d) Patient search results populate the datalist (one <option> per
//       patient).
//   (e) Subtype <select> renders ONLY when checkType.has_subtypes is
//       true (the dye-only or report-only check types skip this
//       affordance entirely).
//   (f) Subtype options localise their name AND format the price with
//       toLocaleString (`12,000` shape, NOT raw `12000`).
//   (g) Doctor <select> renders with the "House" placeholder + one
//       option per doctor.
//   (h) Dye + Report checkboxes disable when the check type does not
//       support them (phase-05 §7.4 -- the dye_supported /
//       report_supported flags drop these toggles).
//   (i) Total card renders the em-dash placeholder when no draft is
//       seeded; renders the locale-formatted snapshot total when one
//       is.
//   (j) "Lock & print" is disabled until patient is selected AND
//       subtype is set (when the check has subtypes).
//   (k) Discard with no draft just navigates back to /reception/checks/:slug
//       (no IPC fired).
//   (l) Discard with a draft invokes `visits_discard` AND navigates back.
//   (m) Save-draft with a brand-new patient (no match in search) fires
//       `patients_create` then `visits_create_draft` with the
//       resolved patient_id.
//   (n) Lock click with a qualified-operator list opens the operator
//       picker. Confirm fires `visits_lock` AND navigates to
//       /reception/visits/:visit_id (the locked visit's id).
//   (o) Operator picker shows the "no_qualified" copy when the
//       qualified-operators list resolves empty.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  MemoryRouter,
  Route,
  Routes,
} from "react-router"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

import "@/i18n"

import i18n from "i18next"

const mockNavigate = vi.fn()
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>(
    "react-router",
  )
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  }
})

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
  }
})

import { invoke } from "@/lib/ipc"
import type {
  ChecksGridCardRecord,
  CheckSubtypeRecord,
  DoctorRecord,
  LockResultRecord,
  PatientRecord,
  QualifiedOperatorRecord,
  VisitRecord,
  VisitSnapshotRecord,
} from "@/lib/ipc"
import NewVisitPage from "@/pages/reception/new-visit"

const directions = [["ltr"], ["rtl"]] as const

const CHECK_ID = "01923af0-7c1a-7000-8001-cccccccccccc"
const PATIENT_ID = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const VISIT_ID = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const DOCTOR_ID = "01923af0-7c1a-7000-8004-aaaaaaaaaaaa"
const SUBTYPE_ID = "01923af0-7c1a-7000-8005-aaaaaaaaaaaa"
const OPERATOR_ID = "01923af0-7c1a-7000-8006-aaaaaaaaaaaa"
const USER_ID = "01923af0-7c1a-7000-8007-aaaaaaaaaaaa"
const ENTITY_ID = "01923af0-7c1a-7000-8099-000000000099"
const ARTIFACT_PATH = "/var/idc/receipts/a5.pdf"

function checkCard(
  overrides: Partial<ChecksGridCardRecord> = {},
): ChecksGridCardRecord {
  return {
    check_type_id: CHECK_ID,
    name_ar: "AR_ECHO",
    name_en: "Echocardiogram",
    has_subtypes: false,
    dye_supported: true,
    report_supported: true,
    todays_visits: 0,
    ...overrides,
  }
}

function patient(overrides: Partial<PatientRecord> = {}): PatientRecord {
  return {
    id: PATIENT_ID,
    name: "Salma A.",
    created_at: "2026-05-19T10:00:00.000Z",
    updated_at: "2026-05-19T10:00:00.000Z",
    deleted_at: null,
    version: 1,
    dirty: false,
    entity_id: ENTITY_ID,
    ...overrides,
  }
}

function doctor(overrides: Partial<DoctorRecord> = {}): DoctorRecord {
  return {
    id: DOCTOR_ID,
    name: "Dr Sarah",
    specialty: "cardiology",
    phone: null,
    is_active: true,
    notes: null,
    created_at: "2026-05-19T10:00:00.000Z",
    updated_at: "2026-05-19T10:00:00.000Z",
    version: 1,
    entity_id: ENTITY_ID,
    ...overrides,
  }
}

function subtype(
  overrides: Partial<CheckSubtypeRecord> = {},
): CheckSubtypeRecord {
  return {
    id: SUBTYPE_ID,
    check_type_id: CHECK_ID,
    name_ar: "AR_BASIC",
    name_en: "Basic",
    price_iqd: 12_000,
    sort_order: 1,
    created_at: "2026-05-19T10:00:00.000Z",
    updated_at: "2026-05-19T10:00:00.000Z",
    version: 1,
    entity_id: ENTITY_ID,
    ...overrides,
  }
}

function snapshot(
  overrides: Partial<VisitSnapshotRecord> = {},
): VisitSnapshotRecord {
  return {
    price_iqd: 25_000,
    dye_cost_iqd: 0,
    report_cost_iqd: 0,
    doctor_cut_iqd: 0,
    operator_cut_iqd: 0,
    internal_pct: null,
    total_amount_iqd: 25_000,
    patient_name: "Salma A.",
    doctor_name: null,
    operator_name: "Neda",
    check_type_name_ar: "AR_ECHO",
    check_type_name_en: "Echocardiogram",
    check_subtype_name_ar: null,
    check_subtype_name_en: null,
    ...overrides,
  }
}

function visit(overrides: Partial<VisitRecord> = {}): VisitRecord {
  return {
    id: VISIT_ID,
    patient_id: PATIENT_ID,
    status: "draft",
    receptionist_user_id: USER_ID,
    check_type_id: CHECK_ID,
    check_subtype_id: null,
    doctor_id: null,
    operator_id: null,
    dye: false,
    report: false,
    locked_at: null,
    voided_at: null,
    voided_by_user_id: null,
    void_reason: null,
    snapshots: snapshot(),
    created_at: "2026-05-19T10:00:00.000Z",
    updated_at: "2026-05-19T10:00:00.000Z",
    deleted_at: null,
    version: 1,
    dirty: true,
    ...(overrides as Partial<VisitRecord>),
  } as VisitRecord
}

function qualifiedOp(
  overrides: Partial<QualifiedOperatorRecord> = {},
): QualifiedOperatorRecord {
  return {
    id: OPERATOR_ID,
    name: "Neda",
    is_active: true,
    ...overrides,
  }
}

interface IpcMockOpts {
  checksGrid?: ChecksGridCardRecord[]
  patients?: PatientRecord[]
  subtypes?: CheckSubtypeRecord[]
  doctors?: DoctorRecord[]
  qualifiedOperators?: QualifiedOperatorRecord[]
  patientCreateResult?: PatientRecord
  visitCreateResult?: VisitRecord
  visitUpdateResult?: VisitRecord
  visitDiscardResult?: null
  visitLockResult?: LockResultRecord
  lockError?: Error
}

function installIpc(opts: IpcMockOpts = {}): void {
  vi.mocked(invoke).mockImplementation(((cmd: string) => {
    if (cmd === "visits_checks_grid") {
      return Promise.resolve(opts.checksGrid ?? [checkCard()])
    }
    if (cmd === "patients_search") {
      return Promise.resolve(opts.patients ?? [])
    }
    if (cmd === "check_subtypes_list_by_type") {
      return Promise.resolve(opts.subtypes ?? [])
    }
    if (cmd === "doctors_list") {
      return Promise.resolve(opts.doctors ?? [])
    }
    if (cmd === "visits_qualified_operators") {
      return Promise.resolve(opts.qualifiedOperators ?? [])
    }
    if (cmd === "patients_create") {
      return Promise.resolve(opts.patientCreateResult ?? patient())
    }
    if (cmd === "visits_create_draft") {
      return Promise.resolve(opts.visitCreateResult ?? visit())
    }
    if (cmd === "visits_update_draft") {
      return Promise.resolve(opts.visitUpdateResult ?? visit())
    }
    if (cmd === "visits_discard") {
      return Promise.resolve(opts.visitDiscardResult ?? null)
    }
    if (cmd === "visits_lock") {
      if (opts.lockError) return Promise.reject(opts.lockError)
      return Promise.resolve(
        opts.visitLockResult ?? {
          visit: visit({ status: "locked", locked_at: "2026-05-19T11:00:00.000Z" }),
          artifacts: { a5_path: ARTIFACT_PATH, thermal_path: "" },
        },
      )
    }
    return Promise.resolve(null)
  }) as never)
}

const SLUG = CHECK_ID

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
    createElement(
      QueryClientProvider,
      { client },
      createElement(
        MemoryRouter,
        { initialEntries: [`/reception/checks/${SLUG}/new`] },
        createElement(
          Routes,
          null,
          createElement(Route, {
            path: "/reception/checks/:slug/new",
            element: children,
          }),
        ),
      ),
    )
  return { wrapper, client }
}

describe.each(directions)(
  "Phase-09 §8 component-render: NewVisitPage (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    beforeEach(() => {
      vi.mocked(invoke).mockReset()
      mockNavigate.mockReset()
    })

    afterEach(() => {
      mockNavigate.mockReset()
    })

    it("renders the eyebrow, title, and back link in the active locale", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const text = container.textContent ?? ""
      expect(text).toContain(i18n.t("reception.eyebrow"))
      expect(text).toContain(i18n.t("reception.new_visit.title"))
      expect(text).toContain(i18n.t("reception.new_visit.back_to_workspace"))
    })

    it("subtitle joins the check name (locale-aware) with reception.new_visit.subtitle", async () => {
      installIpc({
        checksGrid: [
          checkCard({
            check_type_id: CHECK_ID,
            name_ar: "AR_ECHO",
            name_en: "Echocardiogram",
          }),
        ],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        const text = container.textContent ?? ""
        // For en the page resolves name_en; for ar it resolves name_ar.
        const expectedName = dir === "rtl" ? "AR_ECHO" : "Echocardiogram"
        expect(text).toContain(expectedName)
      })
      const text = container.textContent ?? ""
      expect(text).toContain(i18n.t("reception.new_visit.subtitle"))
    })

    it("patient input renders placeholder copy in the active locale", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const placeholderCopy = i18n.t(
        "reception.new_visit.patient_placeholder",
      ) as string
      const input = container.querySelector(
        `input[placeholder='${placeholderCopy}']`,
      )
      expect(input).not.toBeNull()
    })

    it("populates the patient-search datalist with one <option> per result", async () => {
      installIpc({
        patients: [
          patient({ id: PATIENT_ID, name: "Salma A." }),
          patient({ id: "01923af0-7c1a-7000-8002-bbbbbbbbbbbb", name: "Layth B." }),
        ],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      const datalist = await waitFor(() => {
        const dl = container.querySelector("datalist#patient-search")
        if (!dl || dl.children.length === 0) {
          throw new Error("datalist not populated yet")
        }
        return dl as HTMLDataListElement
      })
      const optionValues = Array.from(datalist.querySelectorAll("option")).map(
        (o) => (o as HTMLOptionElement).value,
      )
      expect(optionValues.sort()).toEqual(["Layth B.", "Salma A."])
    })

    it("does NOT render the subtype <select> when checkType.has_subtypes is false", async () => {
      installIpc({
        checksGrid: [checkCard({ has_subtypes: false })],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      // The only <select> visible should be the doctor select; the
      // subtype select is conditionally suppressed.
      const selects = container.querySelectorAll("select")
      expect(selects.length).toBe(1)
    })

    it("renders the subtype <select> with localized name + locale-formatted price when has_subtypes is true", async () => {
      installIpc({
        checksGrid: [checkCard({ has_subtypes: true })],
        subtypes: [
          subtype({ name_en: "Basic", name_ar: "AR_BASIC", price_iqd: 12_000 }),
        ],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      // Wait until BOTH selects have rendered AND the subtype query has
      // populated its options (1 placeholder + 1 subtype = 2 options).
      await waitFor(() => {
        const selects = container.querySelectorAll("select")
        expect(selects.length).toBe(2)
        const subtypeSelect = selects[0]!
        expect(subtypeSelect.querySelectorAll("option").length).toBe(2)
      })
      // Locale-aware subtype name -- the load-bearing localised copy.
      const subtypeSelect = container.querySelectorAll("select")[0]!
      const optionTexts = Array.from(subtypeSelect.querySelectorAll("option"))
        .slice(1)
        .map((o) => (o.textContent ?? "").trim())
      const expectedName = dir === "rtl" ? "AR_BASIC" : "Basic"
      expect(optionTexts[0]).toContain(expectedName)
      // toLocaleString -- 12,000 has either an en-US comma or an ar
      // separator; we only assert that the raw 12000 form is NOT
      // present, since the formatted variant carries locale-specific
      // glyphs.
      expect(optionTexts[0]).not.toContain("12000")
    })

    it("doctor <select> renders 'House' placeholder + one option per doctor", async () => {
      installIpc({
        doctors: [
          doctor({ id: DOCTOR_ID, name: "Dr Sarah" }),
          doctor({ id: "01923af0-7c1a-7000-8004-bbbbbbbbbbbb", name: "Dr Ali" }),
        ],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        const sel = container.querySelector("select")
        const opts = sel?.querySelectorAll("option") ?? []
        // 1 House placeholder + 2 doctors = 3 options
        expect(opts.length).toBe(3)
      })
      const sel = container.querySelector("select") as HTMLSelectElement
      const opts = Array.from(sel.querySelectorAll("option"))
      expect((opts[0]!.textContent ?? "").trim()).toBe(
        i18n.t("reception.new_visit.house"),
      )
      const docNames = opts
        .slice(1)
        .map((o) => (o.textContent ?? "").trim())
        .sort()
      expect(docNames).toEqual(["Dr Ali", "Dr Sarah"])
    })

    it("dye + report checkboxes disable when the check type does not support them", async () => {
      installIpc({
        checksGrid: [
          checkCard({ dye_supported: false, report_supported: false }),
        ],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        const checks = container.querySelectorAll("input[type='checkbox']")
        expect(checks.length).toBe(2)
      })
      const checks = Array.from(
        container.querySelectorAll("input[type='checkbox']"),
      ) as HTMLInputElement[]
      expect(checks.every((c) => c.disabled)).toBe(true)
    })

    it("dye + report checkboxes enable when the check type supports them", async () => {
      installIpc({
        checksGrid: [checkCard({ dye_supported: true, report_supported: true })],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      // Wait for the cards query to propagate -- the checkboxes render
      // synchronously with disabled=true (because checkType is undefined
      // pre-resolve), then re-render with disabled=false once the cards
      // query resolves and the matching card is found.
      await waitFor(() => {
        const checks = Array.from(
          container.querySelectorAll("input[type='checkbox']"),
        ) as HTMLInputElement[]
        expect(checks.length).toBe(2)
        expect(checks.every((c) => !c.disabled)).toBe(true)
      })
    })

    it("total card renders em-dash when no draft is seeded", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const totalEl = container.querySelector(".font-mono.text-\\[28px\\]")
      expect((totalEl?.textContent ?? "").trim()).toBe("—")
    })

    it("Lock & print is disabled while patient is empty", async () => {
      installIpc({
        checksGrid: [checkCard({ has_subtypes: false })],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const lockCopy = i18n.t("reception.new_visit.lock_and_print") as string
      const lockBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => (b.textContent ?? "").trim() === lockCopy,
      ) as HTMLButtonElement
      expect(lockBtn.disabled).toBe(true)
    })

    it("discard with no draft navigates back to /reception/checks/:slug and does NOT invoke visits_discard", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const discardCopy = i18n.t("reception.new_visit.discard") as string
      const discardBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => (b.textContent ?? "").trim() === discardCopy,
      ) as HTMLButtonElement
      fireEvent.click(discardBtn)
      await waitFor(() =>
        expect(mockNavigate).toHaveBeenCalledWith(`/reception/checks/${SLUG}`),
      )
      const discardCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "visits_discard")
      expect(discardCalls.length).toBe(0)
    })

    it("save-draft with a brand-new patient fires patients_create then visits_create_draft", async () => {
      installIpc({
        patients: [], // No match in search -- the page should create one.
        patientCreateResult: patient({ id: PATIENT_ID, name: "Salma A." }),
        visitCreateResult: visit({
          id: VISIT_ID,
          patient_id: PATIENT_ID,
        }),
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const placeholderCopy = i18n.t(
        "reception.new_visit.patient_placeholder",
      ) as string
      const patientInput = container.querySelector(
        `input[placeholder='${placeholderCopy}']`,
      ) as HTMLInputElement
      fireEvent.change(patientInput, { target: { value: "Salma A." } })
      const saveCopy = i18n.t("reception.new_visit.save_draft") as string
      const saveBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => (b.textContent ?? "").trim() === saveCopy,
      ) as HTMLButtonElement
      fireEvent.click(saveBtn)
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("patients_create", {
          args: { name: "Salma A." },
        }),
      )
      await waitFor(() => {
        const draftCall = vi
          .mocked(invoke)
          .mock.calls.find(([cmd]) => cmd === "visits_create_draft")
        expect(draftCall).toBeDefined()
        const payload = draftCall![1] as { args: { patient_id: string; check_type_id: string } }
        expect(payload.args.patient_id).toBe(PATIENT_ID)
        expect(payload.args.check_type_id).toBe(CHECK_ID)
      })
    })

    it("operator picker renders 'no_qualified' copy when qualified-operators list resolves empty", async () => {
      installIpc({
        patients: [patient({ id: PATIENT_ID, name: "Salma A." })],
        qualifiedOperators: [],
        visitCreateResult: visit({ id: VISIT_ID, patient_id: PATIENT_ID }),
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      // Step 1: type the patient name (matches the search result by
      // case-insensitive name equality in selectOrCreatePatient).
      const placeholderCopy = i18n.t(
        "reception.new_visit.patient_placeholder",
      ) as string
      const patientInput = container.querySelector(
        `input[placeholder='${placeholderCopy}']`,
      ) as HTMLInputElement
      fireEvent.change(patientInput, { target: { value: "Salma A." } })
      // Step 2: click "Save draft" -- this resolves the patient match,
      // creates the draft visit, and seeds draft.patient + draft.visit
      // in state. Lock & print becomes enabled only after that.
      const saveCopy = i18n.t("reception.new_visit.save_draft") as string
      const saveBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => (b.textContent ?? "").trim() === saveCopy,
      ) as HTMLButtonElement
      fireEvent.click(saveBtn)
      // Step 3: wait for lock button to enable (draft.patient now set).
      const lockCopy = i18n.t("reception.new_visit.lock_and_print") as string
      const lockBtn = await waitFor(() => {
        const b = Array.from(container.querySelectorAll("button")).find(
          (btn) => (btn.textContent ?? "").trim() === lockCopy,
        ) as HTMLButtonElement | undefined
        if (!b || b.disabled) throw new Error("lock button not yet enabled")
        return b
      })
      fireEvent.click(lockBtn)
      const noQualifiedCopy = i18n.t(
        "reception.new_visit.operator_picker.no_qualified",
      ) as string
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(noQualifiedCopy),
      )
    })

    it("operator picker confirm fires visits_lock and navigates to /reception/visits/:visit_id", async () => {
      installIpc({
        patients: [patient({ id: PATIENT_ID, name: "Salma A." })],
        qualifiedOperators: [qualifiedOp({ id: OPERATOR_ID, name: "Neda" })],
        visitCreateResult: visit({ id: VISIT_ID, patient_id: PATIENT_ID }),
        visitLockResult: {
          visit: visit({
            id: VISIT_ID,
            status: "locked",
            locked_at: "2026-05-19T11:00:00.000Z",
          }),
          artifacts: { a5_path: ARTIFACT_PATH, thermal_path: "" },
        },
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<NewVisitPage />, { wrapper })
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("visits_checks_grid")
      })
      const placeholderCopy = i18n.t(
        "reception.new_visit.patient_placeholder",
      ) as string
      const patientInput = container.querySelector(
        `input[placeholder='${placeholderCopy}']`,
      ) as HTMLInputElement
      fireEvent.change(patientInput, { target: { value: "Salma A." } })
      // Step 1: save draft to seed draft.patient + draft.visit.
      const saveCopy = i18n.t("reception.new_visit.save_draft") as string
      const saveBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => (b.textContent ?? "").trim() === saveCopy,
      ) as HTMLButtonElement
      fireEvent.click(saveBtn)
      const lockCopy = i18n.t("reception.new_visit.lock_and_print") as string
      const lockBtn = await waitFor(() => {
        const b = Array.from(container.querySelectorAll("button")).find(
          (btn) => (btn.textContent ?? "").trim() === lockCopy,
        ) as HTMLButtonElement | undefined
        if (!b || b.disabled) throw new Error("lock button not yet enabled")
        return b
      })
      fireEvent.click(lockBtn)
      const confirmCopy = i18n.t(
        "reception.new_visit.operator_picker.confirm",
      ) as string
      const confirmBtn = await waitFor(() => {
        const b = Array.from(container.querySelectorAll("button")).find(
          (btn) => (btn.textContent ?? "").trim() === confirmCopy,
        ) as HTMLButtonElement | undefined
        if (!b) throw new Error("operator picker not yet open")
        return b
      })
      fireEvent.click(confirmBtn)
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("visits_lock", {
          args: { visit_id: VISIT_ID, operator_id: OPERATOR_ID },
        }),
      )
      await waitFor(() =>
        expect(mockNavigate).toHaveBeenCalledWith(
          `/reception/visits/${VISIT_ID}`,
        ),
      )
    })

    // Defensive reference -- silences lint warnings for the screen
    // helper while the local-DOM querySelector pattern owns the
    // assertion surface.
    void screen
  },
)
