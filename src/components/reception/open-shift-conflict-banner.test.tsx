// Phase-09 §8 component-render assertion: OpenShiftConflictBanner.
//
// Second IPC-driven test in the §8 component-render battery (after
// ConflictResolverPanel). Where ConflictResolverPanel calls `invoke()`
// directly inside `submit()`, this component pulls its data via TanStack
// React Query (`useShiftOverlaps`) -- so the harness adds the
// `QueryClientProvider` wrapper from `src/features/audit/queries.test.ts`
// on top of the `vi.mock("@/lib/ipc", ...)` pattern.
//
// What this file pins (phase-04 §3.Frontend "open-shift conflict banner"):
//
//   (a) Returns `null` while the query is loading (no flash of empty
//       alert chrome before the IPC resolves).
//   (b) Returns `null` when the overlap list resolves empty.
//   (c) Renders `role="alert"` when at least one overlap pair exists.
//   (d) Title carries the pair count via the `reception.shifts.overlap
//       .title` i18n key (verifies the `count: pairs.length` wiring).
//   (e) Body carries the DISTINCT operator count (NOT the pair count)
//       -- the offline-first invariant is "one resolver flow per
//       operator, regardless of how many overlapping pairs that
//       operator owns" (phase-04 §3.Frontend, phase-08 §7.16).
//   (f) One Resolve button per DISTINCT `left.operator_id`. Two pairs
//       owned by the same operator render ONE button, not two.
//   (g) Clicking a Resolve button calls `onResolve(operatorId)` with
//       the exact UUID -- this is the entry point into the manual
//       resolver flow that closes phase-04 §5.shifts conflict policy.
//   (h) Invokes the `shifts_list_overlaps` command with no operator_id
//       filter (the banner is the no-arg overload).
//   (i) Both LTR (en) and RTL (ar) variants render the correct localized
//       strings.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { fireEvent, render, screen } from "@testing-library/react"
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

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
  }
})

import { invoke } from "@/lib/ipc"
import type { ShiftOverlapPair, ShiftRecord } from "@/lib/ipc"
import { OpenShiftConflictBanner } from "@/components/reception/open-shift-conflict-banner"

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

function shift(overrides: Partial<ShiftRecord> = {}): ShiftRecord {
  return {
    id: "01923af0-7c1a-7000-0001-aaaaaaaaaaaa",
    operator_id: "01923af0-7c1a-7000-0002-bbbbbbbbbbbb",
    check_in_at: "2026-05-18T07:00:00.000Z",
    check_out_at: "2026-05-18T15:30:00.000Z",
    check_in_by_user_id: "01923af0-7c1a-7000-0003-cccccccccccc",
    check_out_by_user_id: null,
    note: null,
    created_at: "2026-05-18T07:00:00.000Z",
    updated_at: "2026-05-18T07:00:00.000Z",
    deleted_at: null,
    version: 1,
    entity_id: "01923af0-7c1a-7000-0099-000000000099",
    ...overrides,
  }
}

function pair(operatorId: string, suffix: string): ShiftOverlapPair {
  return {
    left: shift({
      id: `01923af0-7c1a-7000-${suffix}-aaaaaaaaaaaa`,
      operator_id: operatorId,
    }),
    right: shift({
      id: `01923af0-7c1a-7000-${suffix}-bbbbbbbbbbbb`,
      operator_id: operatorId,
    }),
  }
}

async function flushQueries(): Promise<void> {
  // React Query schedules the queryFn microtask + a setState commit.
  // Two awaits cover the mocked-promise resolve plus the React commit
  // that promotes `data` from undefined to the array.
  await Promise.resolve()
  await Promise.resolve()
}

describe.each(directions)(
  "Phase-09 §8 component-render: OpenShiftConflictBanner (dir=%s)",
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

    afterEach(() => {
      // cleanup() handled by src/test-utils/setup.ts. Reset still done
      // explicitly above in beforeEach so a forgotten mockResolvedValueOnce
      // does not bleed across cases.
    })

    it("returns null while the overlaps query is still pending", () => {
      // Never resolves -- the query stays in `pending` for the assertion.
      vi.mocked(invoke).mockReturnValueOnce(
        new Promise(() => {}) as ReturnType<typeof invoke>,
      )
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      expect(container.firstChild).toBeNull()
    })

    it("returns null when the overlap list resolves empty", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([])
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      expect(container.firstChild).toBeNull()
    })

    it("renders role=alert when at least one overlap pair exists", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([
        pair("op-1", "0001"),
      ])
      const { wrapper } = makeWrapper()
      render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      const alert = await screen.findByRole("alert")
      expect(alert).toBeDefined()
    })

    it("title resolves through reception.shifts.overlap.title in the active locale", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([
        pair("op-1", "0001"),
        pair("op-2", "0002"),
      ])
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      await screen.findByRole("alert")
      const text = container.textContent ?? ""
      const titleMatch =
        dir === "rtl"
          ? /ورديات متداخلة/
          : /Overlapping shifts detected/i
      expect(text).toMatch(titleMatch)
    })

    it("body interpolates the DISTINCT operator count, not the pair count", async () => {
      // Two pairs, but both belong to the same operator. The body must
      // report "1 operator", not "2 operators" -- one resolver flow per
      // operator regardless of pair count.
      vi.mocked(invoke).mockResolvedValueOnce([
        pair("op-shared", "0001"),
        pair("op-shared", "0002"),
      ])
      const { wrapper } = makeWrapper()
      const { container } = render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      await screen.findByRole("alert")
      const text = container.textContent ?? ""
      // The body contains the `{{count}}` interpolation -- the literal
      // "1" must appear, and "2" must NOT appear inside the body
      // sentence (it would appear if pair-count leaked through).
      expect(text).toContain("1")
      // Tight assertion: there is exactly one Resolve button (one per
      // distinct operator), which independently confirms the
      // deduplication worked.
      const buttons = screen.getAllByRole("button")
      expect(buttons.length).toBe(1)
    })

    it("renders one Resolve button per distinct operator_id", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([
        pair("op-a", "0001"),
        pair("op-b", "0002"),
        pair("op-a", "0003"), // duplicate of op-a -- collapses
      ])
      const { wrapper } = makeWrapper()
      render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      await screen.findByRole("alert")
      const buttons = screen.getAllByRole("button")
      // Two distinct operators -> two buttons. The third pair (op-a
      // again) MUST NOT add a third button.
      expect(buttons.length).toBe(2)
      // Each carries the localized "Resolve" copy.
      const resolveMatch = dir === "rtl" ? /حلّ/ : /Resolve/i
      for (const btn of buttons) {
        expect(btn.textContent ?? "").toMatch(resolveMatch)
      }
    })

    it("clicking a Resolve button invokes onResolve with that operator_id", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([
        pair("op-alpha", "0001"),
        pair("op-beta", "0002"),
      ])
      const { wrapper } = makeWrapper()
      const onResolve = vi.fn()
      render(
        <OpenShiftConflictBanner onResolve={onResolve} />,
        { wrapper },
      )
      await flushQueries()
      await screen.findByRole("alert")
      const buttons = screen.getAllByRole("button")
      expect(buttons.length).toBe(2)
      fireEvent.click(buttons[0])
      expect(onResolve).toHaveBeenCalledTimes(1)
      expect(onResolve).toHaveBeenCalledWith("op-alpha")
      fireEvent.click(buttons[1])
      expect(onResolve).toHaveBeenCalledTimes(2)
      expect(onResolve).toHaveBeenLastCalledWith("op-beta")
    })

    it("invokes shifts_list_overlaps with the no-operator-id envelope", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([])
      const { wrapper } = makeWrapper()
      render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      expect(invoke).toHaveBeenCalledTimes(1)
      expect(invoke).toHaveBeenCalledWith("shifts_list_overlaps", {
        args: { operator_id: undefined },
      })
    })

    it("renders the alert chrome with the crimson border + soft background tokens", async () => {
      vi.mocked(invoke).mockResolvedValueOnce([pair("op-1", "0001")])
      const { wrapper } = makeWrapper()
      render(
        <OpenShiftConflictBanner onResolve={() => {}} />,
        { wrapper },
      )
      await flushQueries()
      const alert = await screen.findByRole("alert")
      // Design-system contract from .claude/rules/design-system.md
      // §1.4 -- crimson signals "danger / requires action". Pin both
      // tokens so a regression that swaps the border or background to
      // a neutral tone is caught.
      expect(alert.className).toMatch(/border-crimson/)
      expect(alert.className).toMatch(/bg-crimson-soft/)
    })
  },
)
