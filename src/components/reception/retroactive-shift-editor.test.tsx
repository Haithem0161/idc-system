// Phase-09 §8 component-render assertion: RetroactiveShiftEditor.
//
// Dialog-form harness with a TWO-LAYER component:
//
//   - Outer `RetroactiveShiftEditor` resolves the target shift from
//     `useShiftHistoryToday()` -- if the shift is missing it returns
//     null (no flash of a stale form before the new id loads).
//   - Inner `Body` mounts only when a shift is found; it carries the
//     dialog chrome + `<form>` + `useShiftEdit` mutation. The Body
//     UNMOUNTS when the shift_id prop changes (the outer resolves a
//     new shift), so its `useState` initialisers re-run from the
//     fresh row -- no effect-driven reset.
//
// Harness shape notes:
//
//   1. The `<input type="datetime-local" required>` trips JSDOM's
//      HTML5 validation chain if check_in is blank when fireEvent.click
//      hits Save. We use `fireEvent.submit(form)` from the AdjustForm
//      and ClockInDialog harnesses to fire the synthetic submit event
//      directly.
//   2. The IPC mock uses `mockImplementation` so the post-mutation
//      `shiftKeys.all` invalidation can refetch `shifts_history_today`
//      without starving the stub.
//   3. NO MemoryRouter wrapper -- the editor closes through `onClose`,
//      not a route pop. QueryClientProvider alone is sufficient.
//   4. The `toLocalInput()` helper formats datetime-local input values
//      from a UTC ISO string in the host's local zone -- we assert by
//      regex (YYYY-MM-DDTHH:mm) rather than a hardcoded literal so the
//      test stays stable across the LTR/RTL describe.each rerun (and
//      across CI machine timezones).
//
// What this file pins (phase-04 §3.Frontend retroactive editor,
// phase-04 §7.shifts "edit transitions emit an audit row with the prior
// + next values", phase-04 §3.Frontend "note: {value: ...} envelope is
// the load-bearing optional-clear marker"):
//
//   (a) `shiftId={null}` renders nothing.
//   (b) shiftId resolves to a row not in `useShiftHistoryToday` (404):
//       renders nothing (no chrome flash).
//   (c) shiftId resolves to a row that IS in history: dialog chrome
//       (title, X cancel aria-label, In label, Out label, Out hint,
//       Note label, Cancel, Save) resolves through i18n in both
//       locales.
//   (d) check_in input seeds from the shift's check_in_at converted
//       through `toLocalInput()` (regex YYYY-MM-DDTHH:mm).
//   (e) check_out input is empty when shift.check_out_at is null
//       (the "keep the shift open" UX path).
//   (f) check_out input seeds when shift.check_out_at is non-null.
//   (g) Note input seeds from shift.note (or empty when null).
//   (h) Happy path -- both in + out set, note trimmed: `shifts_edit`
//       invoked with `{ shift_id, check_in_at, check_out_at, note: {value: <trimmed>} }`.
//   (i) Whitespace-only note normalises to `note: { value: null }`
//       (load-bearing -- prevents blank-string notes from differing
//       from absent notes).
//   (j) Blank check_out input forwards `check_out_at: null` (keep the
//       shift open path).
//   (k) Successful submit calls `onClose` exactly once.
//   (l) Failure surfaces the raw `err.message` in the error banner AND
//       does NOT close the dialog.
//   (m) Save button disables while the mutation is in-flight.
//   (n) X icon button + footer Cancel button both invoke `onClose`
//       once each and do NOT fire `shifts_edit`.
//   (o) Successful submit invalidates `["shifts"]`.

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
import { RetroactiveShiftEditor } from "@/components/reception/retroactive-shift-editor"

const directions = [["ltr"], ["rtl"]] as const

const SHIFT_ID = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"
const OP_ID = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const USER_ID = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const ENTITY_ID = "01923af0-7c1a-7000-8099-000000000099"

function row(
  overrides: Partial<ShiftWithMetaRecord> = {},
): ShiftWithMetaRecord {
  return {
    id: SHIFT_ID,
    operator_id: OP_ID,
    check_in_at: "2026-05-19T07:00:00.000Z",
    check_out_at: "2026-05-19T15:00:00.000Z",
    check_in_by_user_id: USER_ID,
    check_out_by_user_id: USER_ID,
    note: null,
    created_at: "2026-05-19T07:00:00.000Z",
    updated_at: "2026-05-19T07:00:00.000Z",
    deleted_at: null,
    version: 2,
    entity_id: ENTITY_ID,
    operator_name: "Neda",
    operator_phone: null,
    ...overrides,
  }
}

function shift(overrides: Partial<ShiftRecord> = {}): ShiftRecord {
  return {
    id: SHIFT_ID,
    operator_id: OP_ID,
    check_in_at: "2026-05-19T07:00:00.000Z",
    check_out_at: "2026-05-19T15:00:00.000Z",
    check_in_by_user_id: USER_ID,
    check_out_by_user_id: USER_ID,
    note: null,
    created_at: "2026-05-19T07:00:00.000Z",
    updated_at: "2026-05-19T15:00:00.000Z",
    deleted_at: null,
    version: 3,
    entity_id: ENTITY_ID,
    ...overrides,
  }
}

interface IpcMockOpts {
  history?: ShiftWithMetaRecord[]
  editResult?: ShiftRecord
  editError?: Error
  editPending?: boolean
}

function installIpc(opts: IpcMockOpts = {}): void {
  vi.mocked(invoke).mockImplementation(((cmd: string) => {
    if (cmd === "shifts_history_today") {
      return Promise.resolve(opts.history ?? [row()])
    }
    if (cmd === "shifts_edit") {
      if (opts.editPending) return new Promise(() => {})
      if (opts.editError) return Promise.reject(opts.editError)
      return Promise.resolve(opts.editResult ?? shift())
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

function getForm(container: HTMLElement): HTMLFormElement {
  const form = container.querySelector("form")
  if (!form) throw new Error("RetroactiveShiftEditor <form> not found")
  return form as HTMLFormElement
}

function getInputs(container: HTMLElement): {
  checkIn: HTMLInputElement
  checkOut: HTMLInputElement
  note: HTMLInputElement
} {
  const inputs = Array.from(container.querySelectorAll("input")) as HTMLInputElement[]
  const checkIn = inputs.find((i) => i.type === "datetime-local")
  const checkOut = inputs.filter((i) => i.type === "datetime-local")[1]
  const note = inputs.find((i) => i.type === "text")
  if (!checkIn || !checkOut || !note) {
    throw new Error("RetroactiveShiftEditor expected 2 datetime-local + 1 text inputs")
  }
  return { checkIn, checkOut, note }
}

const LOCAL_RE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/

describe.each(directions)(
  "Phase-09 §8 component-render: RetroactiveShiftEditor (dir=%s)",
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

    it("renders nothing when shiftId is null", () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={null} onClose={onClose} />,
        { wrapper },
      )
      expect(container.querySelector("[role='dialog']")).toBeNull()
    })

    it("renders nothing when shiftId is not in today's history", async () => {
      installIpc({ history: [] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      // Allow the history query to settle.
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("shifts_history_today")
      })
      // Body never mounts -- no <form> renders.
      expect(container.querySelector("form")).toBeNull()
      expect(container.querySelector("[role='dialog']")).toBeNull()
    })

    it("renders dialog chrome (title, X cancel aria-label, In/Out/Note labels, Cancel, Save) in the active locale", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const text = container.textContent ?? ""
      expect(text).toContain(i18n.t("reception.shifts.edit.title"))
      expect(text).toContain(i18n.t("reception.shifts.check_in"))
      expect(text).toContain(i18n.t("reception.shifts.check_out"))
      expect(text).toContain(i18n.t("reception.shifts.edit.out_hint"))
      expect(text).toContain(i18n.t("reception.shifts.clock_in.note"))
      expect(text).toContain(i18n.t("admin.save"))
      expect(text).toContain(i18n.t("admin.cancel"))
      const cancelAria = i18n.t("admin.cancel") as string
      const xBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => b.getAttribute("aria-label") === cancelAria,
      )
      expect(xBtn).not.toBeUndefined()
    })

    it("check_in input seeds from shift.check_in_at via toLocalInput()", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const { checkIn } = getInputs(container)
      expect(LOCAL_RE.test(checkIn.value)).toBe(true)
    })

    it("check_out input is empty when shift.check_out_at is null", async () => {
      installIpc({ history: [row({ check_out_at: null })] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const { checkOut } = getInputs(container)
      expect(checkOut.value).toBe("")
    })

    it("check_out input seeds via toLocalInput() when shift.check_out_at is non-null", async () => {
      installIpc({
        history: [row({ check_out_at: "2026-05-19T15:00:00.000Z" })],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const { checkOut } = getInputs(container)
      expect(LOCAL_RE.test(checkOut.value)).toBe(true)
    })

    it("note input seeds from shift.note when non-null", async () => {
      installIpc({ history: [row({ note: "second-pass review" })] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const { note } = getInputs(container)
      expect(note.value).toBe("second-pass review")
    })

    it("happy path: shifts_edit invoked with shift_id + ISO check_in_at + ISO check_out_at + note:{value:<trimmed>}", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const { note } = getInputs(container)
      fireEvent.change(note, { target: { value: "  manual fix  " } })
      fireEvent.submit(getForm(container))
      await waitFor(() => {
        const editCall = vi
          .mocked(invoke)
          .mock.calls.find(([cmd]) => cmd === "shifts_edit")
        expect(editCall).toBeDefined()
        const payload = editCall![1] as {
          args: {
            shift_id: string
            check_in_at: string
            check_out_at: string | null
            note: { value: string | null }
          }
        }
        expect(payload.args.shift_id).toBe(SHIFT_ID)
        expect(payload.args.check_in_at).toMatch(/T\d{2}:\d{2}/)
        expect(payload.args.check_out_at).not.toBeNull()
        expect(payload.args.note).toEqual({ value: "manual fix" })
      })
    })

    it("whitespace-only note normalises to note:{value:null} (phase-04 §7.shifts invariant)", async () => {
      installIpc({ history: [row({ note: "" })] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const { note } = getInputs(container)
      fireEvent.change(note, { target: { value: "    " } })
      fireEvent.submit(getForm(container))
      await waitFor(() => {
        const editCall = vi
          .mocked(invoke)
          .mock.calls.find(([cmd]) => cmd === "shifts_edit")
        expect(editCall).toBeDefined()
        const payload = editCall![1] as {
          args: { note: { value: string | null } }
        }
        expect(payload.args.note).toEqual({ value: null })
      })
    })

    it("blank check_out input forwards check_out_at: null (keep shift open)", async () => {
      installIpc({ history: [row({ check_out_at: null })] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      fireEvent.submit(getForm(container))
      await waitFor(() => {
        const editCall = vi
          .mocked(invoke)
          .mock.calls.find(([cmd]) => cmd === "shifts_edit")
        expect(editCall).toBeDefined()
        const payload = editCall![1] as {
          args: { check_out_at: string | null }
        }
        expect(payload.args.check_out_at).toBeNull()
      })
    })

    it("successful submit calls onClose exactly once", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      fireEvent.submit(getForm(container))
      await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
    })

    it("surfaces raw error.message in the error banner AND does NOT close on failure", async () => {
      installIpc({
        history: [row()],
        editError: new Error("future check_in_at"),
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(container.textContent ?? "").toContain("future check_in_at"),
      )
      expect(onClose).not.toHaveBeenCalled()
    })

    it("Save button disables while the mutation is in-flight", async () => {
      installIpc({ history: [row()], editPending: true })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const submitBtn = container.querySelector(
        "button[type='submit']",
      ) as HTMLButtonElement
      expect(submitBtn.disabled).toBe(false)
      fireEvent.submit(getForm(container))
      await waitFor(() => expect(submitBtn.disabled).toBe(true))
    })

    it("X icon button invokes onClose once and does NOT invoke shifts_edit", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const cancelAria = i18n.t("admin.cancel") as string
      const xBtn = Array.from(container.querySelectorAll("button")).find(
        (b) => b.getAttribute("aria-label") === cancelAria,
      )
      if (!xBtn) throw new Error("X cancel button not found")
      fireEvent.click(xBtn)
      expect(onClose).toHaveBeenCalledTimes(1)
      const editCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "shifts_edit")
      expect(editCalls.length).toBe(0)
    })

    it("footer Cancel button invokes onClose once and does NOT invoke shifts_edit", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      const cancelCopy = i18n.t("admin.cancel") as string
      const cancelBtn = Array.from(
        container.querySelectorAll("button[type='button']"),
      ).find(
        (b) => (b.textContent ?? "").trim() === cancelCopy,
      ) as HTMLButtonElement
      fireEvent.click(cancelBtn)
      expect(onClose).toHaveBeenCalledTimes(1)
      const editCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "shifts_edit")
      expect(editCalls.length).toBe(0)
    })

    it("invalidates the ['shifts'] query key on a successful edit", async () => {
      installIpc({ history: [row()] })
      const onClose = vi.fn()
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { container } = render(
        <RetroactiveShiftEditor shiftId={SHIFT_ID} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelector("form")).not.toBeNull()
      })
      fireEvent.submit(getForm(container))
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
