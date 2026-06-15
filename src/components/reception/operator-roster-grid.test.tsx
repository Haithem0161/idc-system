// Component-render assertions for the operator roster grid (left pane of the
// shifts page). Runs in both ltr + rtl per `.claude/rules/testing.md` §14.
//
// What this pins:
//   (a) Every active operator renders one row (roster join, not just on-shift).
//   (b) An operator with an open shift shows the live ON SHIFT status; one
//       without shows the muted OFF status.
//   (c) The row action is context-aware: off -> Clock in (dispatches
//       shifts_clock_in with operator_id), on -> Clock out (dispatches
//       shifts_clock_out with the open shift_id).
//   (d) The On/Off/All filter narrows the rows.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
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
import type { OperatorRecord, ShiftWithMetaRecord } from "@/lib/ipc"
import {
  OperatorRosterGrid,
  type RosterFilter,
} from "@/components/reception/operator-roster-grid"

const directions = [["ltr"], ["rtl"]] as const

const OP_A = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const OP_B = "01923af0-7c1a-7000-8002-bbbbbbbbbbbb"
const SHIFT_A = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"
const USER = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const ENTITY = "01923af0-7c1a-7000-8099-000000000099"

function operator (overrides: Partial<OperatorRecord> = {}): OperatorRecord {
  return {
    id: OP_A,
    name: "Hassan Tech",
    phone: "07710000001",
    base_cut_per_check_iqd: 2000,
    is_active: true,
    notes: null,
    created_at: "2026-05-19T06:00:00.000Z",
    updated_at: "2026-05-19T06:00:00.000Z",
    version: 1,
    ...overrides,
  }
}

function openShift (overrides: Partial<ShiftWithMetaRecord> = {}): ShiftWithMetaRecord {
  return {
    id: SHIFT_A,
    operator_id: OP_A,
    check_in_at: "2026-05-19T07:00:00.000Z",
    check_out_at: null,
    check_in_by_user_id: USER,
    check_out_by_user_id: null,
    note: null,
    created_at: "2026-05-19T07:00:00.000Z",
    updated_at: "2026-05-19T07:00:00.000Z",
    deleted_at: null,
    version: 1,
    entity_id: ENTITY,
    operator_name: "Hassan Tech",
    operator_phone: "07710000001",
    ...overrides,
  }
}

const OPERATORS: OperatorRecord[] = [
  operator(),
  operator({ id: OP_B, name: "Zainab Tech", phone: "07710000002" }),
]

function mockOperators (rows: OperatorRecord[] = OPERATORS): void {
  ;(invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation(
    (cmd: string) => {
      if (cmd === "operators_list") return Promise.resolve(rows)
      return Promise.resolve(null)
    }
  )
}

function wrapper (): (props: { children: ReactNode }) => ReturnType<typeof createElement> {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  return ({ children }) =>
    createElement(QueryClientProvider, { client }, children)
}

function renderGrid (
  openShifts: ShiftWithMetaRecord[],
  filter: RosterFilter = "all"
) {
  return render(
    <OperatorRosterGrid
      openShifts={openShifts}
      filter={filter}
      onFilterChange={() => {}}
    />,
    { wrapper: wrapper() }
  )
}

describe.each(directions)("OperatorRosterGrid (dir=%s)", (dir) => {
  beforeEach(async () => {
    await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
    document.documentElement.dir = dir
    mockOperators()
  })
  afterEach(() => {
    vi.clearAllMocks()
  })

  it("renders one row per active operator (roster join, not just on-shift)", async () => {
    renderGrid([openShift()])
    await screen.findByText("Hassan Tech")
    await screen.findByText("Zainab Tech")
  })

  it("shows ON SHIFT status for an open-shift operator and OFF for the rest", async () => {
    renderGrid([openShift()])
    const onLabel = i18n.t("reception.shifts.status_on")
    const offLabel = i18n.t("reception.shifts.status_off")
    await waitFor(() => expect(screen.getByText(onLabel)).toBeTruthy())
    expect(screen.getByText(offLabel)).toBeTruthy()
  })

  it("clocks IN an off operator (shifts_clock_in with operator_id)", async () => {
    renderGrid([]) // nobody on shift
    const rowB = (await screen.findByText("Zainab Tech")).closest("tr")!
    const btn = within(rowB).getByRole("button", {
      name: new RegExp(i18n.t("reception.shifts.clock_in_action"), "i"),
    })
    fireEvent.click(btn)
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("shifts_clock_in", {
        args: { operator_id: OP_B, note: null },
      })
    )
  })

  it("clocks OUT an on-shift operator (shifts_clock_out with shift_id)", async () => {
    renderGrid([openShift()]) // Hassan on shift A
    const rowA = (await screen.findByText("Hassan Tech")).closest("tr")!
    const btn = within(rowA).getByRole("button", {
      name: new RegExp(i18n.t("reception.shifts.clock_out"), "i"),
    })
    fireEvent.click(btn)
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("shifts_clock_out", {
        args: { shift_id: SHIFT_A },
      })
    )
  })

  it("filter=on shows only on-shift operators", async () => {
    renderGrid([openShift()], "on")
    await screen.findByText("Hassan Tech")
    expect(screen.queryByText("Zainab Tech")).toBeNull()
  })
})
