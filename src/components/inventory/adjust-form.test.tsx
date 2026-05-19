// Phase-09 §8 component-render assertion: AdjustForm.
//
// Third IPC-driven test in the §8 component-render battery (after
// ConflictResolverPanel + OpenShiftConflictBanner). Where
// ConflictResolverPanel called `invoke()` directly inside `submit()` and
// OpenShiftConflictBanner pulled its data via a single `useQuery`,
// AdjustForm exercises BOTH React-Query surfaces at once:
//
//   - `useInventoryItems` (useQuery) populates the item <select>.
//   - `useInventoryAdjustmentCreate` (useMutation) drives the submit
//     handler and triggers `qc.invalidateQueries({ queryKey: ["inventory"] })`
//     on success.
//
// Harness shape notes:
//
//   1. The `<select required>` + `<input required>` inputs trip
//      JSDOM's HTML5 form validation when fireEvent.click hits the
//      submit button -- the submit event is silently aborted with no
//      visible error. We bypass this with `fireEvent.submit(form)`
//      which fires the synthetic submit event directly. This is the
//      canonical pattern for required-input form tests in React 19.
//   2. The component accepts an `initialItemId` prop that seeds the
//      itemId state at mount. We use this for every submit-path test
//      so the assertions don't depend on the JSDOM select-interaction
//      quirks; the dropdown-population sentinel asserts the rendered
//      <option> list directly.
//   3. The IPC mock uses `mockImplementation` (NOT mockResolvedValueOnce)
//      because the items query is refetched after every successful
//      mutation (the `qc.invalidateQueries(["inventory"])` triggers a
//      second `inventory_list_items` call) -- a once-only stub would
//      starve the second call.
//
// What this file pins (phase-06 §3.Frontend `<AdjustForm>`, §7.6
// note-trim cap, §7.8 |delta|>1000 warning, §7.15 reason vocabulary,
// `.claude/rules/offline-first.md` invariant 2 "every write commits
// locally first"):
//
//   (a) Form chrome -- title, item label, reason label, delta label,
//       note label, submit, cancel -- all resolve through i18n in both
//       locales.
//   (b) Reason radios under superadmin role: 3 choices
//       (receive / writeoff / count_correction).
//   (c) Reason radios under receptionist role: 2 choices only
//       (receive / writeoff). The count_correction option is hidden --
//       phase-06 §7.15 invariant.
//   (d) The item <select> is populated from `useInventoryItems` --
//       one <option value=""> placeholder + one option per item, each
//       carrying its `id` as the option value.
//   (e) `useInventoryItems` is invoked with the documented envelope:
//       `{ args: { status: undefined, include_inactive: false, query: null } }`.
//   (f) Submit -- receive happy path: `inventory_create_adjustment` is
//       invoked with `{ item_id, reason: "receive", delta: 5, note: null }`.
//   (g) Submit -- writeoff flips sign: input delta=10 carries through
//       as `delta: -10` in the IPC envelope (the form's `toIpcDelta`
//       helper). The CHECK constraint on the SQLite row + the Rust
//       constructor both expect a negative delta for writeoff -- a
//       regression that dropped the sign flip would surface as a write
//       rejected by the local DB at runtime.
//   (h) Submit -- count_correction forwards a signed value unchanged
//       (delta=-3 stays -3).
//   (i) `|delta| > 1000` surfaces the gold warning chip; `|delta| <= 1000`
//       does not (phase-06 §7.8 threshold = 1000).
//   (j) While the mutation is in-flight the submit button is `disabled`
//       and its label flips to the localized "Saving..." copy.
//   (k) On success the green success chip renders with the localized
//       "Adjustment saved." copy AND the delta + note inputs clear.
//   (l) Forbidden-shaped IPC error (`Error("forbidden: ...")`) surfaces
//       the localized forbidden copy in the crimson error chip.
//   (m) Zod refinement: receive with delta=0 surfaces the
//       `delta_must_be_positive` localized error copy without invoking
//       `inventory_create_adjustment` (client-side gate, phase-06 §7.8).
//   (n) On success the mutation invalidates the `["inventory"]` query
//       key -- the items table, item-detail, and adjustments lists all
//       refetch from the freshly-mutated cache.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { MemoryRouter } from "react-router"
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
  InventoryAdjustmentRecord,
  InventoryItemWithStatusRecord,
} from "@/lib/ipc"
import { AdjustForm } from "@/components/inventory/adjust-form"
import { useAuthStore } from "@/stores/auth-store"

const directions = [["ltr"], ["rtl"]] as const

// UUID v7 format -- the 13th hex digit is `7` (version) and the 17th
// is `8/9/a/b` (RFC-4122 variant). Zod v4's `z.string().uuid()` enforces
// this pattern, so the test fixtures use real UUID v7-shaped ids rather
// than the loose dash-separated placeholders used elsewhere in the
// component-render battery (those callers don't validate the bytes).
const ENTITY_ID = "01923af0-7c1a-7000-8099-000000000099"
const NEEDLES_ID = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"
const ALCOHOL_ID = "01923af0-7c1a-7000-8001-bbbbbbbbbbbb"

function item(
  overrides: Partial<InventoryItemWithStatusRecord> = {},
): InventoryItemWithStatusRecord {
  return {
    id: NEEDLES_ID,
    // Latin-only ASCII so i18n-lint (ARABIC_RE) stays quiet and the
    // ar-locale render of resolveLocaleName() returns a unique string
    // per item (we still differentiate Needles/Alcohol by `name_en`
    // overrides on call sites).
    name_ar: "AR_NEEDLES",
    name_en: "Needles",
    unit: "box",
    quantity_on_hand: 25,
    low_stock_threshold: 10,
    is_active: true,
    status: "ok",
    updated_at: "2026-05-18T10:00:00.000Z",
    created_at: "2026-05-01T08:00:00.000Z",
    version: 3,
    dirty: false,
    last_synced_at: "2026-05-18T09:55:00.000Z",
    entity_id: ENTITY_ID,
    ...overrides,
  }
}

function adjustment(
  overrides: Partial<InventoryAdjustmentRecord> = {},
): InventoryAdjustmentRecord {
  return {
    id: "01923af0-7c1a-7000-8002-aaaaaaaaaaaa",
    item_id: NEEDLES_ID,
    delta: 5,
    reason: "receive",
    visit_id: null,
    note: null,
    by_user_id: "0190a000-0000-7000-8000-000000000001",
    created_at: "2026-05-19T10:00:00.000Z",
    updated_at: "2026-05-19T10:00:00.000Z",
    version: 1,
    entity_id: ENTITY_ID,
    is_reversal: false,
    ...overrides,
  }
}

interface IpcMockOpts {
  items?: InventoryItemWithStatusRecord[]
  adjustmentResult?: InventoryAdjustmentRecord
  adjustmentError?: Error
  adjustmentPending?: boolean
}

function installIpc(opts: IpcMockOpts = {}): void {
  const items = opts.items ?? [
    item(),
    item({ id: ALCOHOL_ID, name_en: "Alcohol", name_ar: "AR_ALCOHOL" }),
  ]
  vi.mocked(invoke).mockImplementation(((cmd: string) => {
    if (cmd === "inventory_list_items") {
      return Promise.resolve(items)
    }
    if (cmd === "inventory_create_adjustment") {
      if (opts.adjustmentPending) {
        return new Promise(() => {})
      }
      if (opts.adjustmentError) {
        return Promise.reject(opts.adjustmentError)
      }
      return Promise.resolve(opts.adjustmentResult ?? adjustment())
    }
    // Default for any other command (e.g. inventory_get_item if a
    // sibling cache touches it).
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
    createElement(
      QueryClientProvider,
      { client },
      createElement(MemoryRouter, null, children),
    )
  return { wrapper, client }
}

function setRole(role: "superadmin" | "receptionist" | "accountant"): void {
  useAuthStore.setState({
    state: {
      kind: "authenticated",
      user: {
        user_id: "0190a000-0000-7000-8000-0000000a0001",
        entity_id: ENTITY_ID,
        email: "admin@idc.io",
        name: "Mariam",
        role,
      },
      role,
      mode: "online",
      locked: false,
    },
  })
}

function getForm(container: HTMLElement): HTMLFormElement {
  const form = container.querySelector("form")
  if (!form) throw new Error("AdjustForm root <form> not found in container")
  return form as HTMLFormElement
}

function getDeltaInput(): HTMLInputElement {
  return document.getElementById("adjust-delta") as HTMLInputElement
}

function getNoteInput(): HTMLInputElement {
  return document.getElementById("adjust-note") as HTMLInputElement
}

describe.each(directions)(
  "Phase-09 §8 component-render: AdjustForm (dir=%s)",
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
      setRole("superadmin")
    })

    afterEach(() => {
      useAuthStore.setState({ state: { kind: "anonymous" } })
    })

    it("renders the form chrome (title, item label, reason label, note label, submit, cancel) in the active locale", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<AdjustForm />, { wrapper })
      // Wait for items query so the option list paints. Identifying
      // the placeholder option (always rendered) guarantees the items
      // query has resolved and the panel body is fully painted.
      await waitFor(() => {
        const opts = container.querySelectorAll("option")
        // 1 placeholder + 2 items = 3 options.
        expect(opts.length).toBe(3)
      })
      const text = container.textContent ?? ""
      const expected = [
        i18n.t("inventory.adjust.title"),
        i18n.t("inventory.adjust.item_label"),
        i18n.t("inventory.adjust.reason_label"),
        i18n.t("inventory.adjust.note_label"),
        i18n.t("inventory.adjust.submit"),
        i18n.t("inventory.adjust.cancel"),
        // Delta label is reason-keyed; default reason is receive.
        i18n.t("inventory.adjust.delta_label.receive"),
      ] as string[]
      for (const copy of expected) {
        expect(text).toContain(copy)
      }
    })

    it("under superadmin: renders three reason radios (receive + writeoff + count_correction)", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<AdjustForm />, { wrapper })
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const radios = container.querySelectorAll("input[type='radio']")
      expect(radios.length).toBe(3)
      const values = Array.from(radios)
        .map((r) => (r as HTMLInputElement).value)
        .sort()
      expect(values).toEqual(
        ["count_correction", "receive", "writeoff"].sort(),
      )
    })

    it("under receptionist: renders only two reason radios (count_correction is hidden)", async () => {
      setRole("receptionist")
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<AdjustForm />, { wrapper })
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const radios = container.querySelectorAll("input[type='radio']")
      expect(radios.length).toBe(2)
      const values = Array.from(radios)
        .map((r) => (r as HTMLInputElement).value)
        .sort()
      expect(values).toEqual(["receive", "writeoff"].sort())
    })

    it("populates the item <select> from useInventoryItems (placeholder + one option per item)", async () => {
      installIpc({
        items: [
          item({ id: NEEDLES_ID, name_en: "Needles" }),
          item({ id: ALCOHOL_ID, name_en: "Alcohol" }),
        ],
      })
      const { wrapper } = makeWrapper()
      const { container } = render(<AdjustForm />, { wrapper })
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const options = container.querySelectorAll("option")
      // Placeholder option is disabled with empty value.
      const placeholder = options[0] as HTMLOptionElement
      expect(placeholder.value).toBe("")
      expect(placeholder.disabled).toBe(true)
      // Item options carry the matching value (item.id) -- this is
      // what the IPC envelope will forward, not the localized name.
      const itemOptions = Array.from(options).slice(1) as HTMLOptionElement[]
      const itemIds = itemOptions.map((o) => o.value).sort()
      expect(itemIds).toEqual([NEEDLES_ID, ALCOHOL_ID].sort())
    })

    it("invokes inventory_list_items with the documented envelope (status:undefined, include_inactive:false, query:null)", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<AdjustForm />, { wrapper })
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      expect(invoke).toHaveBeenCalledWith("inventory_list_items", {
        args: {
          status: undefined,
          include_inactive: false,
          query: null,
        },
      })
    })

    it("receive happy path: invokes inventory_create_adjustment with delta=+5 and the resolved item_id", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getDeltaInput(), { target: { value: "5" } })
      // fireEvent.submit fires the synthetic submit event directly,
      // bypassing JSDOM's HTML5 validation chain that aborts when a
      // <select required> is empty. The form's React onSubmit handler
      // runs as if the user pressed Enter inside a valid form.
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("inventory_create_adjustment", {
          args: {
            item_id: NEEDLES_ID,
            reason: "receive",
            delta: 5,
            note: null,
          },
        }),
      )
    })

    it("writeoff flips sign: input delta=10 carries through as delta=-10 in the IPC envelope", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const radios = container.querySelectorAll("input[type='radio']")
      const writeoffRadio = Array.from(radios).find(
        (r) => (r as HTMLInputElement).value === "writeoff",
      ) as HTMLInputElement
      fireEvent.click(writeoffRadio)
      fireEvent.change(getDeltaInput(), { target: { value: "10" } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("inventory_create_adjustment", {
          args: {
            item_id: NEEDLES_ID,
            reason: "writeoff",
            // Sign flip from +10 to -10 -- the load-bearing invariant.
            delta: -10,
            note: null,
          },
        }),
      )
    })

    it("count_correction forwards a signed value unchanged (delta=-3 stays -3)", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const radios = container.querySelectorAll("input[type='radio']")
      const correctionRadio = Array.from(radios).find(
        (r) => (r as HTMLInputElement).value === "count_correction",
      ) as HTMLInputElement
      fireEvent.click(correctionRadio)
      fireEvent.change(getDeltaInput(), { target: { value: "-3" } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("inventory_create_adjustment", {
          args: {
            item_id: NEEDLES_ID,
            reason: "count_correction",
            delta: -3,
            note: null,
          },
        }),
      )
    })

    it("renders the large-delta warning chip when |delta| > 1000 and hides it otherwise", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(<AdjustForm />, { wrapper })
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      const warningCopy = i18n.t(
        "inventory.adjust.helper.large_warning",
      ) as string
      // 1000 -> not shown (strictly greater than threshold).
      fireEvent.change(getDeltaInput(), { target: { value: "1000" } })
      expect(container.textContent ?? "").not.toContain(warningCopy)
      // 1500 -> shown.
      fireEvent.change(getDeltaInput(), { target: { value: "1500" } })
      expect(container.textContent ?? "").toContain(warningCopy)
      // Negative crossings count too -- phase-06 §7.8 uses |delta|.
      fireEvent.change(getDeltaInput(), { target: { value: "-1500" } })
      expect(container.textContent ?? "").toContain(warningCopy)
    })

    it("submit is disabled while the mutation is in-flight and the label flips to the localized Saving copy", async () => {
      installIpc({ adjustmentPending: true })
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getDeltaInput(), { target: { value: "5" } })
      fireEvent.submit(getForm(container))
      const submittingCopy = i18n.t("inventory.adjust.submitting") as string
      // Once the mutation enters the pending state the submit button
      // label flips and `disabled` is set.
      const pendingBtn = (await waitFor(() => {
        const btn = container.querySelector(
          "button[type='submit']",
        ) as HTMLButtonElement | null
        if (!btn || btn.textContent !== submittingCopy) {
          throw new Error("submit button has not flipped to Saving... yet")
        }
        return btn
      })) as HTMLButtonElement
      expect(pendingBtn.disabled).toBe(true)
    })

    it("renders the success chip and clears delta + note on a successful submit", async () => {
      installIpc({ adjustmentResult: adjustment({ delta: 5, note: null }) })
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getDeltaInput(), { target: { value: "5" } })
      fireEvent.change(getNoteInput(), { target: { value: "batch-7" } })
      fireEvent.submit(getForm(container))
      const successCopy = i18n.t("inventory.adjust.success") as string
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(successCopy),
      )
      // Delta + note both clear after the resolve (UX: ready for the
      // next adjustment without the operator wiping the previous
      // values by hand). Item selection deliberately persists --
      // operators often record multiple adjustments per item.
      expect(getDeltaInput().value).toBe("")
      expect(getNoteInput().value).toBe("")
    })

    it("surfaces the localized forbidden copy when the IPC rejects with a forbidden-shaped error", async () => {
      installIpc({
        adjustmentError: new Error(
          "forbidden: count_correction requires one of [superadmin]",
        ),
      })
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getDeltaInput(), { target: { value: "5" } })
      fireEvent.submit(getForm(container))
      const forbiddenCopy = i18n.t(
        "inventory.adjust.errors.forbidden",
      ) as string
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(forbiddenCopy),
      )
    })

    it("zod refinement: receive with delta=0 blocks the IPC and surfaces delta_must_be_positive copy", async () => {
      installIpc()
      const { wrapper } = makeWrapper()
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      // Delta input is empty (default ""); Number("") = 0 (finite),
      // so the form's parsedDelta is 0. The zod superRefine for
      // reason=receive rejects with "delta_must_be_positive".
      fireEvent.submit(getForm(container))
      const errorCopy = i18n.t(
        "inventory.adjust.errors.delta_must_be_positive",
      ) as string
      await waitFor(() =>
        expect(container.textContent ?? "").toContain(errorCopy),
      )
      // The IPC was NOT invoked: only the items query call appears in
      // the call log; no inventory_create_adjustment entry.
      const submitCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "inventory_create_adjustment")
      expect(submitCalls.length).toBe(0)
    })

    it("invalidates the ['inventory'] query key on a successful submit", async () => {
      installIpc({ adjustmentResult: adjustment() })
      const { wrapper, client } = makeWrapper()
      const spy = vi.spyOn(client, "invalidateQueries")
      const { container } = render(
        <AdjustForm initialItemId={NEEDLES_ID} />,
        { wrapper },
      )
      await waitFor(() => {
        expect(container.querySelectorAll("option").length).toBe(3)
      })
      fireEvent.change(getDeltaInput(), { target: { value: "5" } })
      fireEvent.submit(getForm(container))
      await waitFor(() =>
        expect(spy).toHaveBeenCalledWith({ queryKey: ["inventory"] }),
      )
    })

    // Defensive reference -- silences lint warnings for the screen
    // helper while the local-DOM querySelector pattern owns the
    // assertion surface.
    void screen
  },
)
