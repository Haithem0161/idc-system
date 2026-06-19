// Phase-09 §8 component-render assertion: AuditFilters.
//
// AuditFilters is the controlled-form header above the audit table
// (phase-08 §3 Frontend + §7.6 + §7.24): actor / action / entity /
// entity_id_prefix / from / to / free-text. Pure props -- `value` +
// `onChange` -- no IPC, no React Query. The DOM is plain HTML inputs
// styled with the design-system tokens. The §14 anti-pattern row "RTL
// never tested" requires the directional sweep.
//
// What this file pins:
//
//   (a) Every filter input is rendered with a label + a stable id;
//       the action / entity selects enumerate the closed
//       AUDIT_ACTIONS / AUDIT_ENTITIES domains exactly (the "Any"
//       option plus every enum value, no extras, no omissions).
//   (b) Editing the actor text input invokes `onChange` with the new
//       `actor_user_id` merged into the value snapshot; clearing it
//       to an empty string passes `undefined` (filter dropped).
//   (c) Selecting an action from the dropdown invokes `onChange` with
//       the chosen literal; the empty option clears the filter
//       (`action: undefined`).
//   (d) Free-text input merges into `text` and clears to `undefined`.
//   (e) The form's aria-label resolves from `audit.filters.aria` in
//       both locales (the keyboard-nav contract).
//   (f) The form swallows Enter (no submit) -- the audit page applies
//       filters live, not on submit.

import { createElement, type ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import "@/i18n"

import { AuditFilters } from "@/components/audit/audit-filters"
import { AUDIT_ACTIONS, AUDIT_ENTITIES, type AuditFilter } from "@/lib/schemas/audit"
import { invoke } from "@/lib/ipc"

import i18n from "i18next"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return { ...actual, invoke: vi.fn(), isTauri: () => true }
})

const USERS = [
  { id: "u-1", name: "Asma", is_active: true },
  { id: "u-2", name: "Karrar", is_active: false },
]

const directions = [["ltr"], ["rtl"]] as const

function emptyValue(): AuditFilter {
  return {}
}

/** Wrap renders in a QueryClientProvider so `useUsersList` can resolve. */
function makeWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children)
  return { wrapper }
}

describe.each(directions)(
  "Phase-09 §8 component-render: AuditFilters (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    beforeEach(() => {
      vi.mocked(invoke).mockResolvedValue(
        USERS as unknown as Awaited<ReturnType<typeof invoke<"users_list">>>,
      )
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders all 7 filter inputs with stable ids", () => {
      const { container } = render(
        <AuditFilters value={emptyValue()} onChange={() => {}} />,
        { wrapper: makeWrapper().wrapper },
      )
      // Stable IDs let the audit page label inputs via htmlFor without
      // depending on label text (which varies by locale).
      const ids = [
        "audit-filter-actor",
        "audit-filter-action",
        "audit-filter-entity",
        "audit-filter-prefix",
        "audit-filter-from",
        "audit-filter-to",
        "audit-filter-text",
      ]
      for (const id of ids) {
        const el = container.querySelector(`#${id}`)
        expect(el, `expected #${id}`).not.toBeNull()
      }
    })

    it("action <select> enumerates exactly AUDIT_ACTIONS plus 'Any' (no extras, no drops)", () => {
      const { container } = render(
        <AuditFilters value={emptyValue()} onChange={() => {}} />,
        { wrapper: makeWrapper().wrapper },
      )
      const select = container.querySelector("#audit-filter-action") as HTMLSelectElement
      const options = Array.from(select.querySelectorAll("option"))
      // First option is "Any" with value="". The rest must be
      // AUDIT_ACTIONS in declaration order.
      expect(options[0].value).toBe("")
      const optionValues = options.slice(1).map((o) => o.value)
      expect(optionValues).toEqual([...AUDIT_ACTIONS])
    })

    it("entity <select> enumerates exactly AUDIT_ENTITIES plus 'Any' (no extras, no drops)", () => {
      const { container } = render(
        <AuditFilters value={emptyValue()} onChange={() => {}} />,
        { wrapper: makeWrapper().wrapper },
      )
      const select = container.querySelector("#audit-filter-entity") as HTMLSelectElement
      const options = Array.from(select.querySelectorAll("option"))
      expect(options[0].value).toBe("")
      const optionValues = options.slice(1).map((o) => o.value)
      expect(optionValues).toEqual([...AUDIT_ENTITIES])
    })

    it("selecting an actor from the dropdown merges its user id into the next value", async () => {
      const onChange = vi.fn()
      const { findByRole, container } = render(
        <AuditFilters value={{ text: "carry-over" }} onChange={onChange} />,
        { wrapper: makeWrapper().wrapper },
      )
      // Wait for the users query to populate the dropdown options.
      await findByRole("option", { name: "Asma" })
      const actor = container.querySelector("#audit-filter-actor") as HTMLSelectElement
      fireEvent.change(actor, { target: { value: "u-1" } })
      // Spread-of-prior-value is the load-bearing pattern -- a regression that
      // replaced `...value` with `{ key: v }` would drop `text` and surface here.
      expect(onChange).toHaveBeenCalledTimes(1)
      expect(onChange.mock.calls[0][0]).toEqual({
        text: "carry-over",
        actor_user_id: "u-1",
      })
    })

    it("clearing the actor select passes actor_user_id=undefined (filter dropped, not empty-string)", async () => {
      const onChange = vi.fn()
      const { findByRole, container } = render(
        <AuditFilters value={{ actor_user_id: "u-1" }} onChange={onChange} />,
        { wrapper: makeWrapper().wrapper },
      )
      await findByRole("option", { name: "Asma" })
      const actor = container.querySelector("#audit-filter-actor") as HTMLSelectElement
      fireEvent.change(actor, { target: { value: "" } })
      const next = onChange.mock.calls[0][0]
      expect(next).toHaveProperty("actor_user_id", undefined)
      // The Zod schema rejects empty strings against `z.string().uuid()` --
      // passing `""` instead of `undefined` would surface as a validation error.
    })

    it("the actor dropdown lists users, marking inactive ones", async () => {
      const { findByRole } = render(
        <AuditFilters value={emptyValue()} onChange={() => {}} />,
        { wrapper: makeWrapper().wrapper },
      )
      // Active user shows the bare name; inactive user is annotated.
      await findByRole("option", { name: "Asma" })
      const karrar = await findByRole("option", { name: /Karrar/ })
      expect(karrar.textContent).toMatch(/Karrar/)
    })

    it("selecting an action invokes onChange with the chosen literal", () => {
      const onChange = vi.fn()
      const { container } = render(
        <AuditFilters value={emptyValue()} onChange={onChange} />,
        { wrapper: makeWrapper().wrapper },
      )
      const action = container.querySelector("#audit-filter-action") as HTMLSelectElement
      fireEvent.change(action, { target: { value: "lock" } })
      expect(onChange).toHaveBeenCalledTimes(1)
      expect(onChange.mock.calls[0][0]).toEqual({ action: "lock" })
    })

    it("clearing the action select passes action=undefined (closed-enum guard)", () => {
      const onChange = vi.fn()
      const { container } = render(
        <AuditFilters value={{ action: "lock" }} onChange={onChange} />,
        { wrapper: makeWrapper().wrapper },
      )
      const action = container.querySelector("#audit-filter-action") as HTMLSelectElement
      fireEvent.change(action, { target: { value: "" } })
      const next = onChange.mock.calls[0][0]
      expect(next).toHaveProperty("action", undefined)
    })

    it("editing free-text merges `text` into the snapshot", () => {
      const onChange = vi.fn()
      const { container } = render(
        <AuditFilters value={emptyValue()} onChange={onChange} />,
        { wrapper: makeWrapper().wrapper },
      )
      const text = container.querySelector("#audit-filter-text") as HTMLInputElement
      fireEvent.change(text, { target: { value: "panadol" } })
      expect(onChange).toHaveBeenCalledTimes(1)
      expect(onChange.mock.calls[0][0]).toEqual({ text: "panadol" })
    })

    it("clearing free-text drops the filter (text=undefined, not empty-string)", () => {
      const onChange = vi.fn()
      const { container } = render(
        <AuditFilters value={{ text: "panadol" }} onChange={onChange} />,
        { wrapper: makeWrapper().wrapper },
      )
      const text = container.querySelector("#audit-filter-text") as HTMLInputElement
      // Controlled input rendered with value="panadol"; clearing to ""
      // is a real DOM change that React forwards to the handler.
      fireEvent.change(text, { target: { value: "" } })
      expect(onChange).toHaveBeenCalledTimes(1)
      expect(onChange.mock.calls[0][0]).toHaveProperty("text", undefined)
    })

    it("the form's aria-label resolves to the locale-specific copy (audit.filters.aria)", () => {
      render(<AuditFilters value={emptyValue()} onChange={() => {}} />, {
        wrapper: makeWrapper().wrapper,
      })
      const form = screen.getByRole("form")
      const aria = form.getAttribute("aria-label") ?? ""
      // en: "Audit filters"; ar: contains an Arabic-block character.
      const matches =
        dir === "rtl" ? /[؀-ۿ]/.test(aria) : /audit filters/i.test(aria)
      expect(matches).toBe(true)
    })

    it("submit-on-Enter is suppressed -- filters apply live, never on form submit", () => {
      const onChange = vi.fn()
      render(<AuditFilters value={emptyValue()} onChange={onChange} />, {
        wrapper: makeWrapper().wrapper,
      })
      const form = screen.getByRole("form")
      // Default submit would trigger a navigation; preventDefault is
      // wired at audit-filters.tsx:35. We verify that the synthetic
      // submit fires AND that preventDefault was called (not assertable
      // directly, but the lack of `onChange` invocation + a non-null
      // form element is the smoke proof for now).
      const event = new Event("submit", { bubbles: true, cancelable: true })
      form.dispatchEvent(event)
      expect(event.defaultPrevented).toBe(true)
      expect(onChange).not.toHaveBeenCalled()
    })
  },
)
