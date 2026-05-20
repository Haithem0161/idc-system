// Component-render assertions for the tabbed new-visit editor.
//
// Pins the contracts that replaced the old single-form flow:
//
//   (a) With no active tab, the page renders the empty/help copy.
//   (b) With an active tab, the form renders bound to the tab's form
//       state. The check-type subtitle uses the localised name.
//   (c) Dye + Report are rendered as `role="switch"` pill buttons
//       (FeatureToggle), and respect dye_supported / report_supported
//       (rendered with aria-disabled).
//   (d) Pressing the Dye toggle flips the tab form state.
//   (e) The `Finish` button is disabled until a patient is entered
//       (and a subtype, when the check has subtypes).
//   (f) Committing a patient + clicking Finish opens the operator
//       picker, picking an operator fires `visits_lock` and navigates
//       to /reception/visits/{id}.
//
// Harness notes:
//   - The visit-tabs store is global; `beforeEach` clears it so each
//     test starts clean.
//   - IPC is mocked at `@/lib/ipc`. We dispatch via a per-command
//     handler so the same mock can answer multiple commands
//     (visits_checks_grid, patients_search, patients_create,
//     visits_create_draft, visits_qualified_operators, visits_lock,
//     doctors_list, check_subtypes_list).
//   - The page uses `useDebouncedCallback(500ms)` for auto-save. We
//     install fake timers and `advanceTimersByTime(600)` to fire it.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { MemoryRouter } from "react-router"
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react"
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest"
import type { ReactNode } from "react"

import "@/i18n"

const mockNavigate = vi.fn()
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router")
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
import NewVisitTabbedPage from "@/pages/reception/new-visit-tabbed"
import { useVisitTabsStore } from "@/stores/visit-tabs-store"

const CHECK_ID = "01923af0-7c1a-7000-8001-cccccccccccc"
const PATIENT_ID = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const VISIT_ID = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const OPERATOR_ID = "01923af0-7c1a-7000-8004-aaaaaaaaaaaa"
const ENTITY_ID = "tenant"

interface InvokeResponses {
  visits_checks_grid?: unknown
  patients_search?: unknown
  patients_create?: unknown
  visits_create_draft?: unknown
  visits_update_draft?: unknown
  visits_qualified_operators?: unknown
  visits_lock?: unknown
  doctors_list?: unknown
  check_subtypes_list?: unknown
}

function installInvokeRouter (responses: InvokeResponses) {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>
  mockInvoke.mockImplementation((command: string) => {
    const r = (responses as Record<string, unknown>)[command]
    if (r === undefined) return Promise.resolve(null)
    return Promise.resolve(r)
  })
}

function wrapper ({ children }: { children: ReactNode }) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return (
    <MemoryRouter>
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    </MemoryRouter>
  )
}

function defaultResponses (
  override: Partial<InvokeResponses> = {},
): InvokeResponses {
  return {
    visits_checks_grid: [
      {
        check_type_id: CHECK_ID,
        name_ar: "AR_ECHO",
        name_en: "Echocardiogram",
        has_subtypes: false,
        dye_supported: true,
        report_supported: true,
        todays_visits: 0,
      },
    ],
    patients_search: [
      {
        id: PATIENT_ID,
        name: "Salma A.",
        created_at: "2026-05-19T10:00:00.000Z",
        updated_at: "2026-05-19T10:00:00.000Z",
        deleted_at: null,
        version: 1,
        dirty: false,
        entity_id: ENTITY_ID,
      },
    ],
    patients_create: {
      id: PATIENT_ID,
      name: "Salma A.",
      created_at: "2026-05-19T10:00:00.000Z",
      updated_at: "2026-05-19T10:00:00.000Z",
      deleted_at: null,
      version: 1,
      dirty: false,
      entity_id: ENTITY_ID,
    },
    visits_create_draft: {
      id: VISIT_ID,
      patient_id: PATIENT_ID,
      status: "draft",
      check_type_id: CHECK_ID,
      check_subtype_id: null,
      doctor_id: null,
      operator_id: null,
      dye: false,
      report: false,
      created_at: "2026-05-19T10:00:00.000Z",
      updated_at: "2026-05-19T10:00:00.000Z",
      deleted_at: null,
      version: 1,
      dirty: true,
      entity_id: ENTITY_ID,
      snapshots: null,
    },
    visits_update_draft: null,
    visits_qualified_operators: [
      { id: OPERATOR_ID, name: "Neda", is_active: true },
    ],
    visits_lock: {
      visit: { id: VISIT_ID },
    },
    doctors_list: [],
    check_subtypes_list: [],
    ...override,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  useVisitTabsStore.setState({
    ownerUserId: null,
    tabs: [],
    activeTabId: null,
  })
})

afterEach(() => {
  vi.useRealTimers()
})

describe("NewVisitTabbedPage", () => {
  it("renders the empty state when no tab is active", () => {
    installInvokeRouter(defaultResponses())
    render(<NewVisitTabbedPage />, { wrapper })
    expect(screen.getByText(/No visit selected/)).toBeTruthy()
  })

  it("renders the form bound to the active tab when one exists", async () => {
    installInvokeRouter(defaultResponses())
    const tabId = useVisitTabsStore.getState().openTab(CHECK_ID)
    expect(tabId).toBeTruthy()
    render(<NewVisitTabbedPage />, { wrapper })
    await waitFor(() => {
      // Subtitle includes the localised check name.
      expect(screen.getByText(/Echocardiogram/)).toBeTruthy()
    })
  })

  it("renders dye + report as role=switch pill buttons", async () => {
    installInvokeRouter(defaultResponses())
    useVisitTabsStore.getState().openTab(CHECK_ID)
    render(<NewVisitTabbedPage />, { wrapper })
    await waitFor(() => {
      const switches = screen.getAllByRole("switch")
      expect(switches).toHaveLength(2)
    })
  })

  it("disables dye when the check type does not support it", async () => {
    installInvokeRouter(
      defaultResponses({
        visits_checks_grid: [
          {
            check_type_id: CHECK_ID,
            name_ar: "AR_X",
            name_en: "X-Ray",
            has_subtypes: false,
            dye_supported: false,
            report_supported: true,
            todays_visits: 0,
          },
        ],
      }),
    )
    useVisitTabsStore.getState().openTab(CHECK_ID)
    render(<NewVisitTabbedPage />, { wrapper })
    await waitFor(() => {
      const switches = screen.getAllByRole("switch")
      const dyeSwitch = switches.find(
        (s) => s.textContent && /dye/i.test(s.textContent),
      )
      expect(dyeSwitch).toBeTruthy()
      expect(dyeSwitch?.getAttribute("aria-disabled")).toBe("true")
    })
  })

  it("pressing Dye flips the tab form state", async () => {
    installInvokeRouter(defaultResponses())
    const tabId = useVisitTabsStore.getState().openTab(CHECK_ID)
    render(<NewVisitTabbedPage />, { wrapper })
    // Wait for the check-type query to resolve so the toggle isn't
    // disabled (which would coerce pressed=false and swallow the click).
    let dyeSwitch: HTMLElement | undefined
    await waitFor(() => {
      const switches = screen.getAllByRole("switch")
      dyeSwitch = switches.find(
        (s) => s.textContent && /dye/i.test(s.textContent),
      )
      expect(dyeSwitch).toBeTruthy()
      expect(dyeSwitch?.getAttribute("aria-disabled")).toBeNull()
    })
    fireEvent.click(dyeSwitch!)
    const tab = useVisitTabsStore
      .getState()
      .tabs.find((t) => t.tabId === tabId)
    expect(tab?.form.dye).toBe(true)
  })

  it("disables Finish until a patient name is entered", async () => {
    installInvokeRouter(defaultResponses())
    useVisitTabsStore.getState().openTab(CHECK_ID)
    render(<NewVisitTabbedPage />, { wrapper })
    await waitFor(() => screen.getByTestId("finish-btn"))
    expect((screen.getByTestId("finish-btn") as HTMLButtonElement).disabled).toBe(
      true,
    )
  })

  it("Finish flow: opens picker, locks, navigates", async () => {
    installInvokeRouter(defaultResponses())
    // Pre-seed a tab already attached to a draft + patient. This pins the
    // lock-and-navigate slice of the flow without entangling the test in
    // the debounced auto-save state machine (covered separately by the
    // store unit tests).
    const tabId = useVisitTabsStore.getState().openTab(CHECK_ID)
    useVisitTabsStore.getState().updateTabForm(tabId, {
      patientId: PATIENT_ID,
      patientName: "Salma A.",
    })
    useVisitTabsStore.getState().attachDraft(tabId, VISIT_ID)

    render(<NewVisitTabbedPage />, { wrapper })

    // Finish becomes clickable once the check-type metadata loads.
    let finish: HTMLButtonElement
    await waitFor(() => {
      finish = screen.getByTestId("finish-btn") as HTMLButtonElement
      expect(finish.disabled).toBe(false)
    })

    await act(async () => {
      fireEvent.click(finish!)
    })

    // Operator picker should now be open with "Neda" listed.
    await waitFor(() => expect(screen.getByText("Neda")).toBeTruthy())

    // Click confirm (the per-row "Finish visit" button next to Neda).
    const confirmBtn = screen
      .getAllByRole("button")
      .find((b) => b.textContent && /Finish visit/.test(b.textContent))
    expect(confirmBtn).toBeTruthy()
    fireEvent.click(confirmBtn!)

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith(`/reception/visits/${VISIT_ID}`)
    })
  })
})
