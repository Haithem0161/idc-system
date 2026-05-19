// Phase-09 §8 component-render assertion: OnShiftTable.
//
// Hybrid harness slice: the component receives `shifts` via pure props
// (no useQuery to mock) but drives `useShiftClockOut` via React Query,
// so the wrapper carries only the `QueryClientProvider` -- no
// MemoryRouter (no useNavigate) and no isolated useQuery fixture.
//
// Harness shape notes:
//
//   1. The shifts prop is passed in synchronously -- no async settling
//      step is required before the first assertion.
//   2. The IPC mock uses `mockImplementation` because after a successful
//      clock_out the mutation invalidates `shiftKeys.all` and React
//      Query refetches every dependent slice; a once-only stub would
//      starve those refetches mid-test.
//   3. The per-row pending guard reads
//      `clockOut.isPending && clockOut.variables?.shift_id === s.id` --
//      this means a clock-out on shift A does NOT disable shift B's
//      button. We assert this explicitly with two-row fixtures and a
//      hanging Promise on `shifts_clock_out`.
//
// What this file pins (phase-04 §3.Frontend OnShiftTable, phase-04
// §7.shifts "clock_out closes a single open shift", `.claude/rules/
// offline-first.md` invariant 2):
//
//   (a) Panel title + count badge resolve in both locales.
//   (b) 4 column headers (operator, phone, since, actions) resolve in
//       both locales.
//   (c) Empty shifts list renders the EmptyRow with the localized
//       "empty_open" copy and colSpan=4.
//   (d) Non-empty list renders one <tr> per shift in the order given.
//   (e) Phone null falls back to the em-dash placeholder.
//   (f) `formatSince(check_in_at)` is rendered inside the
//       `.status-pill.is-live` live-dot pill (visual liveness invariant).
//   (g) Clock-out click invokes `shifts_clock_out` with the matching
//       shift_id (envelope `{ args: { shift_id } }`).
//   (h) While clock_out is pending for shift A, shift A's button is
//       disabled but shift B's button remains enabled (per-row guard,
//       NOT a global mutation lock).
//   (i) `shiftKeys.all` (`["shifts"]`) invalidates on success.
//   (j) Count badge tnum: `font-mono` class on the span -- numeric
//       columns in IDC must use tabular numerals.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import {
  afterAll,
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

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
  }
})

import { invoke } from "@/lib/ipc"
import type { ShiftRecord, ShiftWithMetaRecord } from "@/lib/ipc"
import { OnShiftTable } from "@/components/reception/on-shift-table"

const directions = [["ltr"], ["rtl"]] as const

const SHIFT_A = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"
const SHIFT_B = "01923af0-7c1a-7000-8001-bbbbbbbbbbbb"
const OP_A = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const OP_B = "01923af0-7c1a-7000-8002-bbbbbbbbbbbb"
const USER_ID = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const ENTITY_ID = "01923af0-7c1a-7000-8099-000000000099"

function row(overrides: Partial<ShiftWithMetaRecord> = {}): ShiftWithMetaRecord {
  return {
    id: SHIFT_A,
    operator_id: OP_A,
    check_in_at: "2026-05-19T07:00:00.000Z",
    check_out_at: null,
    check_in_by_user_id: USER_ID,
    check_out_by_user_id: null,
    note: null,
    created_at: "2026-05-19T07:00:00.000Z",
    updated_at: "2026-05-19T07:00:00.000Z",
    deleted_at: null,
    version: 1,
    entity_id: ENTITY_ID,
    operator_name: "Neda",
    operator_phone: null,
    ...overrides,
  }
}

function shift(overrides: Partial<ShiftRecord> = {}): ShiftRecord {
  return {
    id: SHIFT_A,
    operator_id: OP_A,
    check_in_at: "2026-05-19T07:00:00.000Z",
    check_out_at: "2026-05-19T15:00:00.000Z",
    check_in_by_user_id: USER_ID,
    check_out_by_user_id: USER_ID,
    note: null,
    created_at: "2026-05-19T07:00:00.000Z",
    updated_at: "2026-05-19T15:00:00.000Z",
    deleted_at: null,
    version: 2,
    entity_id: ENTITY_ID,
    ...overrides,
  }
}

interface IpcMockOpts {
  clockOutResult?: ShiftRecord
  clockOutError?: Error
  /**
   * Per-shift-id pending toggle -- when set, the matching `shifts_clock_out`
   * call hangs as an unresolved Promise so the per-row disabled-state
   * assertion can stabilize.
   */
  clockOutPendingFor?: string
}

function installIpc(opts: IpcMockOpts = {}): void {
  vi.mocked(invoke).mockImplementation(((cmd: string, payload?: unknown) => {
    if (cmd === "shifts_clock_out") {
      const shiftId = (payload as { args: { shift_id: string } } | undefined)
        ?.args?.shift_id
      if (opts.clockOutPendingFor && shiftId === opts.clockOutPendingFor) {
        return new Promise(() => {})
      }
      if (opts.clockOutError) {
        return Promise.reject(opts.clockOutError)
      }
      return Promise.resolve(opts.clockOutResult ?? shift({ id: shiftId ?? SHIFT_A }))
    }
    return Promise.resolve(null)
  }) as never)
}

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

describe.each(directions)(
  "Phase-09 §8 component-render: OnShiftTable (dir=%s)",
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
    })

    it("renders the panel title and count badge in the active locale", () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<OnShiftTable shifts={[row()]} />, {
        wrapper,
      })
      const text = container.textContent ?? ""
      expect(text).toContain(i18n.t("reception.shifts.on_shift"))
      const badge = container.querySelector(".count-badge")
      expect(badge).not.toBeNull()
      expect(badge?.textContent?.trim()).toBe("1")
      // tnum guardrail -- count badges in IDC must be mono-tabular.
      expect(badge?.classList.contains("font-mono")).toBe(true)
    })

    it("renders the 4 column headers (operator, phone, since, actions) in the active locale", () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<OnShiftTable shifts={[]} />, { wrapper })
      const headers = Array.from(container.querySelectorAll("th")).map((th) =>
        (th.textContent ?? "").trim(),
      )
      expect(headers).toEqual([
        i18n.t("reception.shifts.operator"),
        i18n.t("reception.shifts.phone"),
        i18n.t("reception.shifts.since"),
        i18n.t("admin.actions"),
      ])
    })

    it("renders EmptyRow with empty_open copy and colSpan=4 when shifts is empty", () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<OnShiftTable shifts={[]} />, { wrapper })
      const text = container.textContent ?? ""
      expect(text).toContain(i18n.t("reception.shifts.empty_open"))
      // The EmptyRow component renders a single TD with colSpan=4 over
      // the (operator, phone, since, actions) column set.
      const placeholderCell = Array.from(container.querySelectorAll("td")).find(
        (td) => td.getAttribute("colspan") === "4",
      )
      expect(placeholderCell).not.toBeUndefined()
    })

    it("renders one body <tr> per shift in the order given", () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OnShiftTable
          shifts={[
            row({ id: SHIFT_A, operator_id: OP_A, operator_name: "Neda" }),
            row({ id: SHIFT_B, operator_id: OP_B, operator_name: "Fatima" }),
          ]}
        />,
        { wrapper },
      )
      const bodyRows = container.querySelectorAll("tbody tr")
      // 2 data rows + 0 empty placeholder rows (list is non-empty).
      expect(bodyRows.length).toBe(2)
      const names = Array.from(bodyRows).map(
        (tr) => (tr.querySelector("td")?.textContent ?? "").trim(),
      )
      expect(names).toEqual(["Neda", "Fatima"])
    })

    it("falls back to em-dash when operator_phone is null", () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OnShiftTable shifts={[row({ operator_phone: null })]} />,
        { wrapper },
      )
      // The em-dash sits in the phone column (2nd td of the row).
      const cells = container.querySelectorAll("tbody tr td")
      expect((cells[1]?.textContent ?? "").trim()).toBe("—")
    })

    it("renders formatSince(check_in_at) inside a live status-pill", () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OnShiftTable shifts={[row({ check_in_at: "2026-05-19T07:00:00.000Z" })]} />,
        { wrapper },
      )
      const pill = container.querySelector(".status-pill.is-live")
      expect(pill).not.toBeNull()
      // We don't pin the formatted string -- it depends on the test's
      // wall clock vs the 07:00 timestamp. The invariant is: the pill
      // exists and is non-empty.
      expect((pill?.textContent ?? "").trim().length).toBeGreaterThan(0)
    })

    it("clock-out click invokes shifts_clock_out with the matching shift_id envelope", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OnShiftTable shifts={[row({ id: SHIFT_A })]} />,
        { wrapper },
      )
      const btn = container.querySelector(
        "button[type='button']",
      ) as HTMLButtonElement
      fireEvent.click(btn)
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_clock_out", {
          args: { shift_id: SHIFT_A },
        }),
      )
    })

    it("per-row pending guard: pending on shift A does NOT disable shift B", async () => {
      installIpc({ clockOutPendingFor: SHIFT_A })
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OnShiftTable
          shifts={[
            row({ id: SHIFT_A, operator_id: OP_A, operator_name: "Neda" }),
            row({ id: SHIFT_B, operator_id: OP_B, operator_name: "Fatima" }),
          ]}
        />,
        { wrapper },
      )
      const buttons = Array.from(
        container.querySelectorAll("button[type='button']"),
      ) as HTMLButtonElement[]
      // Pre-click baseline: both buttons enabled.
      expect(buttons[0]!.disabled).toBe(false)
      expect(buttons[1]!.disabled).toBe(false)
      // Click row A's clock-out -- A pends forever, B must stay enabled.
      fireEvent.click(buttons[0]!)
      await waitFor(() => expect(buttons[0]!.disabled).toBe(true))
      expect(buttons[1]!.disabled).toBe(false)
    })

    it("invalidates the ['shifts'] query key on a successful clock_out", async () => {
      installIpc()
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { container } = render(
        <OnShiftTable shifts={[row({ id: SHIFT_A })]} />,
        { wrapper },
      )
      const btn = container.querySelector(
        "button[type='button']",
      ) as HTMLButtonElement
      fireEvent.click(btn)
      await waitFor(() =>
        expect(spy).toHaveBeenCalledWith({ queryKey: ["shifts"] }),
      )
    })

    // Defensive reference -- silences lint warnings for the screen
    // helper while the local-DOM querySelector pattern owns the
    // assertion surface.
    void screen
  },
)
