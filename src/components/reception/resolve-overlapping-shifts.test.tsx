// Phase-09 §8 component-render assertion: ResolveOverlappingShifts.
//
// Dialog-shaped resolver harness. The component fans out to THREE
// React-Query handles in one shell:
//
//   - `useShiftOverlaps(operatorId)` (useQuery, gated by operatorId)
//   - `useShiftClockOut` (useMutation, per-shift close-now button)
//   - `useShiftSoftDelete` (useMutation, per-shift delete button)
//
// Harness shape notes:
//
//   1. The dialog renders `null` when `operatorId` is null -- exactly
//      one assertion covers that gate; every other assertion mounts the
//      dialog with a real UUID v7.
//   2. The IPC mock uses `mockImplementation` (not mockResolvedValueOnce)
//      because both mutations invalidate `shiftKeys.all` on success and
//      the overlaps query refetches -- a once-only stub would starve
//      the refetch leg.
//   3. The dialog has TWO cancel surfaces (the X icon button with an
//      aria-label and the footer "Close" text button). We find each by
//      its distinguishing trait: the X uses `getByLabelText`, the
//      footer uses text content. This is the spec for any modal that
//      mounts both surfaces; lifted from clock-in-dialog.test.tsx.
//   4. NO MemoryRouter wrapper -- the dialog closes through the
//      `onClose` prop, not via `useNavigate`. The QueryClientProvider
//      alone is sufficient.
//
// What this file pins (phase-04 §3.Frontend overlap resolver,
// phase-04 §7.shifts "operator cannot clock in twice without clocking
// out first" remediation flow, phase-08 §7.16 manual-resolution policy):
//
//   (a) `operatorId={null}` renders nothing (the parent banner controls
//       open/closed via the prop).
//   (b) Dialog chrome (title, modal hint, X cancel aria-label, footer
//       Close button) resolves through i18n in both locales.
//   (c) Empty overlap list renders the localized "cleared" placeholder
//       AND no <li> rows render.
//   (d) Non-empty overlap list: each overlap pair contributes its
//       `left` + `right` shift to the resolver list, deduplicated by
//       shift_id (a shift appearing in TWO pairs renders ONE row, not
//       two -- the Map<id, shift> guard in the component body).
//   (e) Shift rows are sorted by `check_in_at` ascending (the resolver
//       presents the older overlap first -- usability invariant; lets
//       the operator pick which to keep without scrolling).
//   (f) Each shift row carries its 8-char id prefix (no full UUID leak)
//       and the formatted `check_in_at` / `check_out_at` times.
//   (g) Open shift (check_out_at=null) renders the "Close now" button;
//       closed shift renders ONLY the Delete button (close-now would be
//       a no-op against a row that already has check_out_at).
//   (h) Close-now click invokes `shifts_clock_out` with the matching
//       shift_id envelope.
//   (i) Delete click invokes `shifts_soft_delete` with the matching
//       shift_id AND the canonical reason "overlap resolution" (this
//       reason text is the load-bearing trail for the audit row that
//       phase-08 §7.16 expects to see in the resolver flow).
//   (j) `useShiftOverlaps` is invoked with the documented envelope
//       `{ args: { operator_id } }`.
//   (k) Both close-now buttons disable while ANY clock_out is pending
//       (the component reads `clockOut.isPending` globally, not per-row
//       -- this is the documented "delete and close are queue-style"
//       behaviour; phase-04 §3.Frontend dialog affordance pin).
//   (l) Both delete buttons disable while ANY soft_delete is pending
//       (same global-mutation lock invariant).
//   (m) Footer Close button calls `onClose` exactly once; the X icon
//       button also calls `onClose` exactly once.
//   (n) Failure on close-now surfaces the raw `err.message` in the
//       error banner.

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
import type { ShiftOverlapPair, ShiftRecord } from "@/lib/ipc"
import { ResolveOverlappingShifts } from "@/components/reception/resolve-overlapping-shifts"

const directions = [["ltr"], ["rtl"]] as const

const OP_A = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const SHIFT_OPEN = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"
const SHIFT_CLOSED = "01923af0-7c1a-7000-8001-bbbbbbbbbbbb"
const SHIFT_THIRD = "01923af0-7c1a-7000-8001-cccccccccccc"
const USER_ID = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const ENTITY_ID = "01923af0-7c1a-7000-8099-000000000099"

function shift(overrides: Partial<ShiftRecord> = {}): ShiftRecord {
  return {
    id: SHIFT_OPEN,
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
    ...overrides,
  }
}

interface IpcMockOpts {
  overlaps?: ShiftOverlapPair[]
  clockOutPending?: boolean
  softDeletePending?: boolean
  clockOutError?: Error
  softDeleteError?: Error
}

function installIpc(opts: IpcMockOpts = {}): void {
  const overlaps = opts.overlaps ?? []
  vi.mocked(invoke).mockImplementation(((cmd: string, payload?: unknown) => {
    if (cmd === "shifts_list_overlaps") {
      return Promise.resolve(overlaps)
    }
    if (cmd === "shifts_clock_out") {
      const shiftId = (payload as { args: { shift_id: string } } | undefined)
        ?.args?.shift_id
      if (opts.clockOutPending) return new Promise(() => {})
      if (opts.clockOutError) return Promise.reject(opts.clockOutError)
      return Promise.resolve(
        shift({
          id: shiftId ?? SHIFT_OPEN,
          check_out_at: "2026-05-19T08:00:00.000Z",
        }),
      )
    }
    if (cmd === "shifts_soft_delete") {
      const shiftId = (payload as { args: { shift_id: string } } | undefined)
        ?.args?.shift_id
      if (opts.softDeletePending) return new Promise(() => {})
      if (opts.softDeleteError) return Promise.reject(opts.softDeleteError)
      return Promise.resolve(
        shift({
          id: shiftId ?? SHIFT_OPEN,
          deleted_at: "2026-05-19T09:00:00.000Z",
        }),
      )
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
  "Phase-09 §8 component-render: ResolveOverlappingShifts (dir=%s)",
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

    it("renders nothing when operatorId is null", () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={null} onClose={onClose} />,
        { wrapper },
      )
      expect(container.querySelector("[role='dialog']")).toBeNull()
    })

    it("renders dialog chrome (title, modal hint, X aria-label, footer Close) in the active locale", async () => {
      installIpc({
        overlaps: [
          { left: shift({ id: SHIFT_OPEN }), right: shift({ id: SHIFT_CLOSED, check_out_at: "2026-05-19T09:00:00.000Z" }) },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const text = container.textContent ?? ""
      expect(text).toContain(i18n.t("reception.shifts.overlap.title"))
      expect(text).toContain(i18n.t("reception.shifts.overlap.modal_hint"))
      // X icon button carries the cancel aria-label.
      const cancelAria = i18n.t("admin.cancel") as string
      const xBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => b.getAttribute("aria-label") === cancelAria,
      )
      expect(xBtn).not.toBeUndefined()
      // Footer Close button: type=button + text content "admin.close".
      const closeText = i18n.t("admin.close") as string
      const footerCloseBtn = Array.from(
        container.querySelectorAll("button[type='button']"),
      ).find((b) => (b.textContent ?? "").trim() === closeText)
      expect(footerCloseBtn).not.toBeUndefined()
    })

    it("renders the 'cleared' placeholder and no <li> rows when overlaps resolve empty", async () => {
      installIpc({ overlaps: [] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      const cleared = i18n.t("reception.shifts.overlap.cleared") as string
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(cleared),
      )
      expect(container.querySelectorAll("li").length).toBe(0)
    })

    it("dedupes shifts across pairs by id (one li per distinct shift, not per pair-side)", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN, check_in_at: "2026-05-19T07:00:00.000Z" }),
            right: shift({
              id: SHIFT_CLOSED,
              check_in_at: "2026-05-19T08:00:00.000Z",
              check_out_at: "2026-05-19T09:00:00.000Z",
            }),
          },
          // Same SHIFT_OPEN appears in a second pair against a third
          // overlapping row -- the Map<id, shift> guard must keep it as
          // a single li.
          {
            left: shift({ id: SHIFT_OPEN, check_in_at: "2026-05-19T07:00:00.000Z" }),
            right: shift({
              id: SHIFT_THIRD,
              check_in_at: "2026-05-19T09:30:00.000Z",
              check_out_at: "2026-05-19T11:00:00.000Z",
            }),
          },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(3)
      })
    })

    it("rows are sorted by check_in_at ascending (older overlap first)", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_CLOSED, check_in_at: "2026-05-19T09:30:00.000Z", check_out_at: "2026-05-19T11:00:00.000Z" }),
            right: shift({ id: SHIFT_OPEN, check_in_at: "2026-05-19T07:00:00.000Z" }),
          },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const rowIds = Array.from(container.querySelectorAll("li .text-ink-3"))
        .map((el) => (el.textContent ?? "").trim())
        .filter((s) => /^[a-f0-9]{8}$/.test(s))
      // Older check_in (07:00) sorts first; 8-char prefix of SHIFT_OPEN.
      expect(rowIds[0]).toBe(SHIFT_OPEN.slice(0, 8))
      expect(rowIds[1]).toBe(SHIFT_CLOSED.slice(0, 8))
    })

    it("each row carries the 8-char shift id prefix (no full UUID leak)", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN }),
            right: shift({ id: SHIFT_CLOSED, check_out_at: "2026-05-19T09:00:00.000Z" }),
          },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const text = container.textContent ?? ""
      expect(text).toContain(SHIFT_OPEN.slice(0, 8))
      expect(text).toContain(SHIFT_CLOSED.slice(0, 8))
      // Negative sentinel: the FULL UUID must NOT be visible.
      expect(text).not.toContain(SHIFT_OPEN)
      expect(text).not.toContain(SHIFT_CLOSED)
    })

    it("open shift gets BOTH Close now + Delete buttons; closed shift gets Delete only", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN, check_out_at: null }),
            right: shift({
              id: SHIFT_CLOSED,
              check_out_at: "2026-05-19T09:00:00.000Z",
            }),
          },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const closeNowCopy = i18n.t("reception.shifts.overlap.close_now") as string
      const deleteCopy = i18n.t("reception.shifts.overlap.delete") as string
      const allButtons = Array.from(container.querySelectorAll("li button")) as HTMLButtonElement[]
      const closeNowBtns = allButtons.filter(
        (b) => (b.textContent ?? "").trim() === closeNowCopy,
      )
      const deleteBtns = allButtons.filter(
        (b) => (b.textContent ?? "").trim() === deleteCopy,
      )
      // Exactly 1 Close-now (only the open shift exposes it).
      expect(closeNowBtns.length).toBe(1)
      // Both rows expose Delete.
      expect(deleteBtns.length).toBe(2)
    })

    it("Close now click invokes shifts_clock_out with the row's shift_id envelope", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN, check_out_at: null }),
            right: shift({
              id: SHIFT_CLOSED,
              check_out_at: "2026-05-19T09:00:00.000Z",
            }),
          },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const closeNowCopy = i18n.t("reception.shifts.overlap.close_now") as string
      const closeNowBtn = Array.from(container.querySelectorAll("li button")).find(
        (b) => (b.textContent ?? "").trim() === closeNowCopy,
      ) as HTMLButtonElement
      fireEvent.click(closeNowBtn)
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_clock_out", {
          args: { shift_id: SHIFT_OPEN },
        }),
      )
    })

    it("Delete click invokes shifts_soft_delete with the canonical 'overlap resolution' reason", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN, check_out_at: null }),
            right: shift({
              id: SHIFT_CLOSED,
              check_out_at: "2026-05-19T09:00:00.000Z",
            }),
          },
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const deleteCopy = i18n.t("reception.shifts.overlap.delete") as string
      const deleteBtn = Array.from(container.querySelectorAll("li button")).find(
        (b) => (b.textContent ?? "").trim() === deleteCopy,
      ) as HTMLButtonElement
      fireEvent.click(deleteBtn)
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_soft_delete", {
          args: {
            // The first li (sorted by check_in_at) carries SHIFT_OPEN.
            shift_id: SHIFT_OPEN,
            reason: "overlap resolution",
          },
        }),
      )
    })

    it("invokes shifts_list_overlaps with the operator_id envelope", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      render(<ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />, {
        wrapper,
      })
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_list_overlaps", {
          args: { operator_id: OP_A },
        }),
      )
    })

    it("all Close-now buttons disable while ANY clock_out is pending", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN, check_out_at: null }),
            right: shift({ id: SHIFT_THIRD, check_in_at: "2026-05-19T09:30:00.000Z", check_out_at: null }),
          },
        ],
        clockOutPending: true,
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const closeNowCopy = i18n.t("reception.shifts.overlap.close_now") as string
      const closeNowBtns = Array.from(
        container.querySelectorAll("li button"),
      ).filter(
        (b) => (b.textContent ?? "").trim() === closeNowCopy,
      ) as HTMLButtonElement[]
      // Pre-click baseline: both close-now buttons enabled.
      expect(closeNowBtns.length).toBe(2)
      expect(closeNowBtns.every((b) => !b.disabled)).toBe(true)
      // Click the first; the second must also disable.
      fireEvent.click(closeNowBtns[0]!)
      await waitFor(() => expect(closeNowBtns[0]!.disabled).toBe(true))
      expect(closeNowBtns[1]!.disabled).toBe(true)
    })

    it("all Delete buttons disable while ANY soft_delete is pending", async () => {
      installIpc({
        overlaps: [
          {
            left: shift({ id: SHIFT_OPEN }),
            right: shift({
              id: SHIFT_CLOSED,
              check_out_at: "2026-05-19T09:00:00.000Z",
            }),
          },
        ],
        softDeletePending: true,
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const deleteCopy = i18n.t("reception.shifts.overlap.delete") as string
      const deleteBtns = Array.from(
        container.querySelectorAll("li button"),
      ).filter(
        (b) => (b.textContent ?? "").trim() === deleteCopy,
      ) as HTMLButtonElement[]
      expect(deleteBtns.length).toBe(2)
      expect(deleteBtns.every((b) => !b.disabled)).toBe(true)
      fireEvent.click(deleteBtns[0]!)
      await waitFor(() => expect(deleteBtns[0]!.disabled).toBe(true))
      expect(deleteBtns[1]!.disabled).toBe(true)
    })

    it("X icon button invokes onClose exactly once", async () => {
      installIpc({ overlaps: [] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      const cancelAria = i18n.t("admin.cancel") as string
      const xBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => b.getAttribute("aria-label") === cancelAria,
      )
      if (!xBtn) throw new Error("X cancel button not found")
      fireEvent.click(xBtn)
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it("footer Close button invokes onClose exactly once", async () => {
      installIpc({ overlaps: [] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      const closeCopy = i18n.t("admin.close") as string
      const footerBtn = Array.from(
        container.querySelectorAll("button[type='button']"),
      ).find(
        (b) => (b.textContent ?? "").trim() === closeCopy,
      ) as HTMLButtonElement
      fireEvent.click(footerBtn)
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it("close-now failure surfaces the raw error.message in the error banner", async () => {
      installIpc({
        overlaps: [
          { left: shift({ id: SHIFT_OPEN }), right: shift({ id: SHIFT_CLOSED, check_out_at: "2026-05-19T09:00:00.000Z" }) },
        ],
        clockOutError: new Error("shift already closed"),
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ResolveOverlappingShifts operatorId={OP_A} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("li").length).toBe(2)
      })
      const closeNowCopy = i18n.t("reception.shifts.overlap.close_now") as string
      const closeNowBtn = Array.from(container.querySelectorAll("li button")).find(
        (b) => (b.textContent ?? "").trim() === closeNowCopy,
      ) as HTMLButtonElement
      fireEvent.click(closeNowBtn)
      await waitFor(() =>
        expect(container.textContent ?? "").toContain("shift already closed"),
      )
    })

    // Defensive reference -- silences lint warnings for the screen
    // helper while the local-DOM querySelector pattern owns the
    // assertion surface.
    void screen
  },
)
