// Component-render assertions for the shift activity feed (right pane of the
// shifts page). Runs in both ltr + rtl per `.claude/rules/testing.md` §14.
//
// What this pins:
//   (a) A closed shift explodes into TWO events (clock-in + clock-out); an
//       open shift contributes ONE (clock-in only).
//   (b) Events are ordered newest-first by timestamp.
//   (c) "by <user>" is resolved from the users list when available.
//   (d) Empty input renders the empty-state copy, no feed list.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor, within } from "@testing-library/react"
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
import type { ShiftWithMetaRecord, UserAdminRecord } from "@/lib/ipc"
import { ShiftActivityFeed } from "@/components/reception/shift-activity-feed"

const directions = [["ltr"], ["rtl"]] as const

const USER = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const ENTITY = "01923af0-7c1a-7000-8099-000000000099"

function shift (overrides: Partial<ShiftWithMetaRecord> = {}): ShiftWithMetaRecord {
  return {
    id: "01923af0-7c1a-7000-8001-aaaaaaaaaaaa",
    operator_id: "01923af0-7c1a-7000-8002-aaaaaaaaaaaa",
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
    operator_phone: null,
    ...overrides,
  }
}

const ADMIN: UserAdminRecord = {
  id: USER,
  email: "admin@idc.iq",
  name: "Admin",
  role: "superadmin",
  is_active: true,
  last_login_at: null,
  created_at: "2026-05-19T06:00:00.000Z",
  updated_at: "2026-05-19T06:00:00.000Z",
  entity_id: ENTITY,
  version: 1,
}

function mockUsers (rows: UserAdminRecord[] = [ADMIN]): void {
  ;(invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation(
    (cmd: string) => {
      if (cmd === "users_list") return Promise.resolve(rows)
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

describe.each(directions)("ShiftActivityFeed (dir=%s)", (dir) => {
  beforeEach(async () => {
    await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
    document.documentElement.dir = dir
    mockUsers()
  })
  afterEach(() => {
    vi.clearAllMocks()
  })

  it("renders the empty state for no shifts", () => {
    render(<ShiftActivityFeed shifts={[]} />, { wrapper: wrapper() })
    expect(screen.getByText(i18n.t("reception.shifts.activity.empty"))).toBeTruthy()
    expect(screen.queryByTestId("activity-feed")).toBeNull()
  })

  it("explodes a closed shift into two events and an open shift into one", async () => {
    render(
      <ShiftActivityFeed
        shifts={[
          shift({
            id: "closed",
            check_in_at: "2026-05-19T08:00:00.000Z",
            check_out_at: "2026-05-19T13:00:00.000Z",
            check_out_by_user_id: USER,
          }),
          shift({ id: "open", check_in_at: "2026-05-19T09:00:00.000Z" }),
        ]}
      />,
      { wrapper: wrapper() }
    )
    const feed = await screen.findByTestId("activity-feed")
    // 2 (closed in+out) + 1 (open in) = 3 events.
    expect(feed.querySelectorAll("li")).toHaveLength(3)
  })

  it("orders events newest-first", async () => {
    render(
      <ShiftActivityFeed
        shifts={[
          shift({
            id: "closed",
            operator_name: "Hassan Tech",
            check_in_at: "2026-05-19T08:00:00.000Z",
            check_out_at: "2026-05-19T13:00:00.000Z",
            check_out_by_user_id: USER,
          }),
        ]}
      />,
      { wrapper: wrapper() }
    )
    const feed = await screen.findByTestId("activity-feed")
    const items = within(feed).getAllByRole("listitem")
    const clockedOut = i18n.t("reception.shifts.activity.clocked_out")
    const clockedIn = i18n.t("reception.shifts.activity.clocked_in")
    // 13:00 clock-out comes before 08:00 clock-in.
    expect(items[0].textContent).toContain(clockedOut)
    expect(items[1].textContent).toContain(clockedIn)
  })

  it("resolves the acting user's name from the users list", async () => {
    render(
      <ShiftActivityFeed shifts={[shift()]} />,
      { wrapper: wrapper() }
    )
    const feed = await screen.findByTestId("activity-feed")
    await waitFor(() => expect(feed.textContent).toContain("Admin"))
  })
})
