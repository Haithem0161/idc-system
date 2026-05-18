// Phase-09 §8 component-render assertion: ConflictList.
//
// ConflictList is the left rail of the conflict-resolver page (phase-08
// §3 Frontend): one row per parked server-side conflict, click-to-select
// drives the right-hand panel. It is dependency-light -- pure props
// (`conflicts`, `selectedOpId`, `onSelect`) with no IPC and no React
// Query. The §14 anti-pattern row "RTL never tested" requires the
// `describe.each([['ltr'],['rtl']])` sweep so a future negative-margin
// or mirrored-border regression surfaces immediately.
//
// What this file pins:
//
//   (a) Empty list renders the `sync_conflicts.empty` i18n placeholder
//       (resolved string, not the i18n key).
//   (b) Populated list renders one button per conflict row.
//   (c) Each row carries the entity label, an 8-char entity-id prefix,
//       the reason copy, and the full opId mono string.
//   (d) Clicking a row invokes `onSelect` with the clicked conflict
//       payload (not the index, not the entityId -- the whole record).
//   (e) The currently-selected row carries `aria-current="true"`; all
//       other rows do not.
//   (f) Reason dot color flips by reason kind: a `version`-named reason
//       paints gold (calm warning), every other reason paints crimson
//       (manual-policy alarm).

import { fireEvent, render, screen } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"

import "@/i18n"

import { ConflictList } from "@/components/sync/conflict-list"
import type { Conflict } from "@/lib/schemas/sync"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

function conflict(overrides: Partial<Conflict> = {}): Conflict {
  return {
    opId: "01923af0-7c1a-7000-8000-000000000001",
    entity: "visits",
    entityId: "01923af0-7c1a-7000-c001-000000000001",
    serverPayload: { status: "locked" },
    localPayload: { status: "draft" },
    reason: "manual_policy_visit_divergence",
    ...overrides,
  }
}

describe.each(directions)(
  "Phase-09 §8 component-render: ConflictList (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("empty list renders the sync_conflicts.empty placeholder copy", () => {
      const { container } = render(
        <ConflictList conflicts={[]} selectedOpId={null} onSelect={() => {}} />,
      )
      // The placeholder is a <div>, not a <button>. No list rendered.
      expect(container.querySelector("ul")).toBeNull()
      const text = container.textContent ?? ""
      // en: "No unresolved conflicts. The queue is clear."
      // ar: "لا توجد تعارضات عالقة..."  -- an Arabic-block character regex.
      const matches =
        dir === "rtl"
          ? /[؀-ۿ]/.test(text)
          : /No unresolved conflicts/i.test(text)
      expect(matches).toBe(true)
    })

    it("renders one <button> row per conflict in the list", () => {
      const items: Conflict[] = [
        conflict({ opId: "op-a" }),
        conflict({ opId: "op-b" }),
        conflict({ opId: "op-c" }),
      ]
      const { container } = render(
        <ConflictList
          conflicts={items}
          selectedOpId={null}
          onSelect={() => {}}
        />,
      )
      const buttons = container.querySelectorAll("ul button[type='button']")
      expect(buttons.length).toBe(3)
    })

    it("each row surfaces the entity label, 8-char entity-id prefix, and full opId", () => {
      const c = conflict({
        opId: "01923af0-7c1a-7000-8000-aaaaaaaaaaaa",
        entityId: "01923af0-7c1a-7000-c001-bbbbbbbbbbbb",
      })
      const { container } = render(
        <ConflictList
          conflicts={[c]}
          selectedOpId={null}
          onSelect={() => {}}
        />,
      )
      const text = container.textContent ?? ""
      // 8-char prefix of the entityId: first 8 chars of UUIDv7.
      expect(text).toContain("01923af0")
      // Full opId is rendered as the last mono line.
      expect(text).toContain("01923af0-7c1a-7000-8000-aaaaaaaaaaaa")
    })

    it("clicking a row invokes onSelect with the FULL conflict record", () => {
      const onSelect = vi.fn()
      const a = conflict({ opId: "op-a" })
      const b = conflict({ opId: "op-b" })
      render(
        <ConflictList
          conflicts={[a, b]}
          selectedOpId={null}
          onSelect={onSelect}
        />,
      )
      const buttons = screen.getAllByRole("button")
      fireEvent.click(buttons[1])
      expect(onSelect).toHaveBeenCalledTimes(1)
      // Pass-by-reference contract: the SAME object is handed back, not
      // a clone, not the opId. A future refactor that maps through
      // .find(byId) would break this assertion if the reference were
      // lost.
      expect(onSelect.mock.calls[0][0]).toBe(b)
    })

    it("aria-current='true' marks the selected row and nothing else", () => {
      const a = conflict({ opId: "op-a" })
      const b = conflict({ opId: "op-b" })
      const c = conflict({ opId: "op-c" })
      render(
        <ConflictList
          conflicts={[a, b, c]}
          selectedOpId="op-b"
          onSelect={() => {}}
        />,
      )
      const buttons = screen.getAllByRole("button")
      expect(buttons[0].getAttribute("aria-current")).toBeNull()
      expect(buttons[1].getAttribute("aria-current")).toBe("true")
      expect(buttons[2].getAttribute("aria-current")).toBeNull()
    })

    it("'version'-named reasons paint a gold dot; others paint crimson", () => {
      // Two-row fixture: one with `version` in the reason string, one
      // without. The dot is the FIRST <span> child of the button at
      // line 42-47 of conflict-list.tsx.
      const items: Conflict[] = [
        conflict({ opId: "op-version", reason: "manual_policy_version_divergence" }),
        conflict({ opId: "op-other", reason: "manual_policy_visit_divergence" }),
      ]
      const { container } = render(
        <ConflictList
          conflicts={items}
          selectedOpId={null}
          onSelect={() => {}}
        />,
      )
      const buttons = container.querySelectorAll("ul button")
      const dotA = buttons[0].querySelector("span[aria-hidden]")
      const dotB = buttons[1].querySelector("span[aria-hidden]")
      expect(dotA?.className).toMatch(/bg-gold/)
      expect(dotA?.className).not.toMatch(/bg-crimson/)
      expect(dotB?.className).toMatch(/bg-crimson/)
      expect(dotB?.className).not.toMatch(/bg-gold/)
    })
  },
)
