// Phase-09 §8 component-render assertion: ClockInDialog.
//
// Fourth IPC-driven test in the §8 component-render battery (after
// ConflictResolverPanel direct-invoke seed, OpenShiftConflictBanner
// single-useQuery extension, and AdjustForm useQuery+useMutation FORM
// seed). ClockInDialog combines TWO data queries (useOperators +
// useOpenShifts) with one useMutation (useShiftClockIn) -- the
// candidates list is the operators result with open-shift owners
// filtered out, so both queries must resolve before the test can
// assert on the dropdown contents.
//
// Harness shape notes:
//
//   1. The component is a controlled dialog (`open=true` mounts the
//      body; `open=false` returns null). Every assertion that needs
//      the form body passes `open={true}`. The open=false case is its
//      own assertion.
//   2. The `<select required>` trips JSDOM's HTML5 form validation when
//      fireEvent.click hits the submit button -- we use the
//      `fireEvent.submit(form)` pattern lifted from AdjustForm to fire
//      the synthetic submit event directly. This is the canonical
//      required-input form pattern for React 19 + JSDOM.
//   3. The IPC mock uses `mockImplementation` (NOT mockResolvedValueOnce)
//      because the on-success invalidation refetches BOTH operators_list
//      AND shifts_list_open via the `shiftKeys.all` invalidation -- a
//      once-only stub would starve those refetches mid-test.
//   4. NO MemoryRouter wrapper here -- ClockInDialog does not use
//      `useNavigate` (cancel goes via the `onClose` prop, not a
//      navigation). The QueryClientProvider alone is enough; this is
//      the spec for any dialog-shaped form that drives close-by-prop
//      rather than route-pop.
//
// What this file pins (phase-04 §3.Frontend `<ClockInDialog>`,
// phase-04 §7.shifts open-shift filter, `.claude/rules/offline-first.md`
// invariant 2 "every write commits locally first" + invariant 3 "no
// write reaches the server without an op_id" -- the local-DB write
// supplies the op_id via the Rust handler):
//
//   (a) `open={false}` renders nothing (no chrome flash before the
//       dialog is invoked).
//   (b) Dialog chrome (title, operator label, placeholder, note label,
//       cancel, submit) resolves through i18n in both locales.
//   (c) Operator <select> populates from `useOperators`, FILTERED to
//       exclude operators that already own an open shift -- the core
//       phase-04 §7.shifts invariant "an operator cannot clock in
//       twice without clocking out first".
//   (d) When every active operator owns an open shift the candidates
//       list collapses to empty and the localized "all_on_shift" hint
//       renders.
//   (e) Submit with no operator selected surfaces the localized
//       "Choose an operator" error AND does NOT invoke
//       `shifts_clock_in`.
//   (f) Submit with an operator + empty note happy path: invokes
//       `shifts_clock_in` with `{ args: { operator_id, note: null } }`
//       and the dialog's `onClose` callback fires exactly once.
//   (g) Submit with an operator + non-empty note: the note string
//       flows through verbatim (trimmed) as `note` in the IPC envelope.
//   (h) Submit with an operator + whitespace-only note: the trim
//       reduces it to "" and the envelope carries `note: null` (the
//       phase-04 §7.shifts "empty note normalizes to NULL" invariant
//       -- prevents the server from receiving blank-string notes that
//       differ from absent notes).
//   (i) Submit failure surfaces the raw `err.message` in the error
//       banner AND does NOT invoke `onClose` (operator stays in the
//       dialog to fix or cancel).
//   (j) `useOperators` is invoked with the documented envelope
//       `{ args: { include_inactive: false } }`.
//   (k) `useShiftClockIn` invalidates the `shiftKeys.all` (`["shifts"]`)
//       query key on success -- the on-shift table, today's history,
//       and any open-overlap banners all refetch from the freshly-
//       mutated cache.
//   (l) Cancel button calls `onClose` exactly once and does not invoke
//       `shifts_clock_in` even if the operator is selected (cancel is
//       not a stealth submit).
//   (m) Submit button is disabled while the mutation is pending AND
//       while the operator is unset (the latter is the
//       chosen-empty-state UX guardrail; phase-04 §3.Frontend dialog
//       affordance pin).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
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
  OperatorRecord,
  ShiftRecord,
  ShiftWithMetaRecord,
} from "@/lib/ipc"
import { ClockInDialog } from "@/components/reception/clock-in-dialog"

const directions = [["ltr"], ["rtl"]] as const

// UUID v7 format -- 13th hex digit is `7` (version), 17th is `8/9/a/b`
// (RFC-4122 variant). Same convention as AdjustForm.test.tsx; necessary
// for any IPC envelope that flows through a Zod v4 `z.string().uuid()`
// guard.
const OP_NEDA_ID = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"
const OP_FATIMA_ID = "01923af0-7c1a-7000-8001-bbbbbbbbbbbb"
const OP_HUDA_ID = "01923af0-7c1a-7000-8001-cccccccccccc"
const SHIFT_ID = "01923af0-7c1a-7000-8002-aaaaaaaaaaaa"
const USER_ID = "01923af0-7c1a-7000-8003-aaaaaaaaaaaa"
const ENTITY_ID = "01923af0-7c1a-7000-8099-000000000099"

function operator(overrides: Partial<OperatorRecord> = {}): OperatorRecord {
  return {
    id: OP_NEDA_ID,
    // Latin-only ASCII so the i18n linter (ARABIC_RE) stays quiet --
    // operator names in fixtures don't need locale-specific rendering
    // for this dialog (the select shows op.name as-is, no
    // resolveLocaleName indirection).
    name: "Neda",
    phone: null,
    base_cut_per_check_iqd: 0,
    is_active: true,
    notes: null,
    created_at: "2026-05-01T08:00:00.000Z",
    updated_at: "2026-05-18T10:00:00.000Z",
    version: 1,
    ...overrides,
  }
}

function shift(overrides: Partial<ShiftRecord> = {}): ShiftRecord {
  return {
    id: SHIFT_ID,
    operator_id: OP_NEDA_ID,
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

function openShift(
  overrides: Partial<ShiftWithMetaRecord> = {},
): ShiftWithMetaRecord {
  return {
    ...shift(),
    operator_name: "Neda",
    operator_phone: null,
    ...overrides,
  }
}

interface IpcMockOpts {
  operators?: OperatorRecord[]
  openShifts?: ShiftWithMetaRecord[]
  clockInResult?: ShiftRecord
  clockInError?: Error
  clockInPending?: boolean
}

function installIpc(opts: IpcMockOpts = {}): void {
  // Default: two operators, neither on shift. Lets the happy-path
  // assertions select an operator without juggling the open-shift
  // filter.
  const operators = opts.operators ?? [
    operator({ id: OP_NEDA_ID, name: "Neda" }),
    operator({ id: OP_FATIMA_ID, name: "Fatima" }),
  ]
  const openShifts = opts.openShifts ?? []
  vi.mocked(invoke).mockImplementation(((cmd: string) => {
    if (cmd === "operators_list") {
      return Promise.resolve(operators)
    }
    if (cmd === "shifts_list_open") {
      return Promise.resolve(openShifts)
    }
    if (cmd === "shifts_clock_in") {
      if (opts.clockInPending) {
        return new Promise(() => {})
      }
      if (opts.clockInError) {
        return Promise.reject(opts.clockInError)
      }
      return Promise.resolve(opts.clockInResult ?? shift())
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
  if (!form) throw new Error("ClockInDialog <form> not found in container")
  return form as HTMLFormElement
}

function getSelect(container: HTMLElement): HTMLSelectElement {
  const select = container.querySelector("select")
  if (!select) throw new Error("ClockInDialog operator <select> not found")
  return select as HTMLSelectElement
}

function getNoteInput(container: HTMLElement): HTMLInputElement {
  const input = container.querySelector("input[type='text']")
  if (!input) throw new Error("ClockInDialog note <input> not found")
  return input as HTMLInputElement
}

function getSubmitBtn(container: HTMLElement): HTMLButtonElement {
  const btn = container.querySelector("button[type='submit']")
  if (!btn) throw new Error("ClockInDialog submit button not found")
  return btn as HTMLButtonElement
}

describe.each(directions)(
  "Phase-09 §8 component-render: ClockInDialog (dir=%s)",
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
      // No global state to reset (no Zustand role gate in this dialog).
    })

    it("renders nothing when open={false}", () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={false} onClose={onClose} />,
        { wrapper },
      )
      expect(container.querySelector("form")).toBeNull()
      expect(container.querySelector("[role='dialog']")).toBeNull()
    })

    it("renders the dialog chrome (title, operator label, placeholder, note label, cancel, submit) in the active locale", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      // Wait for both queries to settle so the option list paints.
      // Placeholder + 2 candidates = 3 options.
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const text = container.textContent ?? ""
      const expected = [
        i18n.t("reception.shifts.clock_in.title"),
        i18n.t("reception.shifts.clock_in.operator"),
        i18n.t("reception.shifts.clock_in.placeholder"),
        i18n.t("reception.shifts.clock_in.note"),
        i18n.t("reception.shifts.clock_in.submit"),
        i18n.t("admin.cancel"),
      ] as string[]
      for (const copy of expected) {
        expect(text).toContain(copy)
      }
    })

    it("populates the operator <select> from useOperators, EXCLUDING operators that own an open shift", async () => {
      installIpc({
        operators: [
          operator({ id: OP_NEDA_ID, name: "Neda" }),
          operator({ id: OP_FATIMA_ID, name: "Fatima" }),
          operator({ id: OP_HUDA_ID, name: "Huda" }),
        ],
        // Fatima is on shift -- she must not appear in the candidate list.
        openShifts: [openShift({ operator_id: OP_FATIMA_ID })],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        // 1 placeholder + 2 candidates (Neda + Huda, Fatima filtered out).
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const options = container.querySelectorAll("option")
      const placeholder = options[0] as HTMLOptionElement
      expect(placeholder.value).toBe("")
      const candidateValues = Array.from(options)
        .slice(1)
        .map((o) => (o as HTMLOptionElement).value)
        .sort()
      expect(candidateValues).toEqual([OP_NEDA_ID, OP_HUDA_ID].sort())
      // Negative-space sentinel: Fatima must NOT be in the candidate
      // list -- a regression that dropped the open-shift filter would
      // surface here.
      expect(candidateValues).not.toContain(OP_FATIMA_ID)
    })

    it("renders the localized 'all_on_shift' hint when every active operator owns an open shift", async () => {
      installIpc({
        operators: [
          operator({ id: OP_NEDA_ID, name: "Neda" }),
          operator({ id: OP_FATIMA_ID, name: "Fatima" }),
        ],
        openShifts: [
          openShift({ operator_id: OP_NEDA_ID }),
          openShift({
            id: "01923af0-7c1a-7000-8002-bbbbbbbbbbbb",
            operator_id: OP_FATIMA_ID,
          }),
        ],
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      const allOnShiftCopy = i18n.t(
        "reception.shifts.clock_in.all_on_shift",
      ) as string
      await waitFor(() => {
        expect(container.textContent ?? "").toContain(allOnShiftCopy)
      })
      // Sentinel: placeholder is the only option (no candidates).
      expect(container.querySelectorAll("option").length).toBe(1)
    })

    it("submit with no operator selected surfaces the 'Choose an operator' error and does NOT invoke shifts_clock_in", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      // fireEvent.submit bypasses JSDOM's HTML5 validation chain that
      // would silently abort the submit event when <select required>
      // has an empty value. The React onSubmit handler runs as if the
      // user pressed Enter inside a valid form.
      fireEvent.submit(getForm(container))
      const chooseCopy = i18n.t(
        "reception.shifts.errors.choose_operator",
      ) as string
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(chooseCopy),
      )
      // IPC was NOT invoked for shifts_clock_in. (operators_list +
      // shifts_list_open were invoked by the queries; we only check the
      // mutation didn't fire.)
      const clockInCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "shifts_clock_in")
      expect(clockInCalls.length).toBe(0)
      // onClose must NOT be called when the submit was rejected client-side.
      expect(onClose).not.toHaveBeenCalled()
    })

    it("happy path: invokes shifts_clock_in with { operator_id, note: null } and closes the dialog on success", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_clock_in", {
          args: { operator_id: OP_NEDA_ID, note: null },
        }),
      )
      await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
    })

    it("forwards a non-empty note string verbatim (post-trim) as note in the IPC envelope", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      fireEvent.change(getNoteInput(container), {
        target: { value: "  early start  " },
      })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_clock_in", {
          args: { operator_id: OP_NEDA_ID, note: "early start" },
        }),
      )
    })

    it("whitespace-only note normalizes to note: null (phase-04 §7.shifts invariant)", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      fireEvent.change(getNoteInput(container), { target: { value: "   " } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("shifts_clock_in", {
          args: { operator_id: OP_NEDA_ID, note: null },
        }),
      )
    })

    it("surfaces the raw error.message in the banner and does NOT close the dialog on failure", async () => {
      installIpc({
        clockInError: new Error("operator already on shift"),
      })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(
          "operator already on shift",
        ),
      )
      // onClose must NOT fire when the mutation rejected -- the
      // operator stays in the dialog to either retry or cancel.
      expect(onClose).not.toHaveBeenCalled()
    })

    it("invokes operators_list with the documented envelope ({ args: { include_inactive: false } })", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      expect(invoke).toHaveBeenCalledWith("operators_list", {
        args: { include_inactive: false },
      })
    })

    it("invalidates the ['shifts'] query key on a successful clock-in", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(spy).toHaveBeenCalledWith({ queryKey: ["shifts"] }),
      )
    })

    it("cancel button invokes onClose exactly once and does NOT invoke shifts_clock_in even if an operator is selected", async () => {
      installIpc()
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      // Pre-select an operator so the dialog is in a submit-ready
      // state -- cancel must short-circuit regardless.
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      // The footer Cancel button is the only `<button>` with the
      // localized cancel copy AND `type="button"` (the X icon button
      // has an aria-label, not text). Find it via text.
      const cancelCopy = i18n.t("admin.cancel") as string
      const cancelBtn = Array.from(container.querySelectorAll("button")).find(
        (b) =>
          (b as HTMLButtonElement).type === "button" &&
          (b.textContent ?? "").trim() === cancelCopy,
      ) as HTMLButtonElement | undefined
      if (!cancelBtn) {
        throw new Error("Cancel button not found in dialog footer")
      }
      fireEvent.click(cancelBtn)
      expect(onClose).toHaveBeenCalledTimes(1)
      const clockInCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "shifts_clock_in")
      expect(clockInCalls.length).toBe(0)
    })

    it("submit button is disabled while operatorId is unset AND while the mutation is in-flight", async () => {
      installIpc({ clockInPending: true })
      const onClose = vi.fn()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <ClockInDialog open={true} onClose={onClose} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      // Empty-operator baseline -- submit is disabled per the
      // `disabled={clockIn.isPending || !operatorId}` guard.
      expect(getSubmitBtn(container).disabled).toBe(true)
      // Select an operator -- submit becomes enabled.
      fireEvent.change(getSelect(container), { target: { value: OP_NEDA_ID } })
      expect(getSubmitBtn(container).disabled).toBe(false)
      // Fire submit; the mutation enters the never-resolving pending
      // state and the button re-disables.
      fireEvent.submit(getForm(container))
      await waitFor(() => expect(getSubmitBtn(container).disabled).toBe(true))
    })

    // Defensive reference -- silences lint warnings for the screen
    // helper while the local-DOM querySelector pattern owns the
    // assertion surface.
    void screen
  },
)
