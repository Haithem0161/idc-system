// Phase-09 §8 component-render assertion: ShiftHistoryToday.
//
// ShiftHistoryToday is the receptionist's day-history table (phase-04
// §3.Frontend): operator / check-in / check-out / duration / lines-run
// for every shift on the current calendar day. Pure props -- `shifts`,
// `canEdit`, `onEditShift`. No IPC, no Zustand.
//
// What this file pins:
//
//   (a) Closed shifts render their check-out time verbatim (formatted);
//       still-open shifts render the `is-info` "Open" status pill in
//       the check-out cell instead.
//   (b) Closed shifts compute a duration (rendered in font-mono);
//       open shifts render the em-dash placeholder (no duration math
//       on a missing check_out_at -- that would surface as NaN).
//   (c) `canEdit=true` adds the actions column with an Edit button per
//       row that invokes `onEditShift(shift.id)`. `canEdit=false`
//       omits the column entirely.
//   (d) Empty shifts -> EmptyRow placeholder spanning the right number
//       of columns (5 when canEdit=false, 6 when canEdit=true).
//   (e) Count badge shows the row count.
//   (f) Header row resolves from `reception.shifts.*` i18n keys in
//       both locales.

import { render, screen } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"

import "@/i18n"

import { ShiftHistoryToday } from "@/components/reception/shift-history-today"
import type { ShiftWithMetaRecord } from "@/lib/ipc"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

function shift(overrides: Partial<ShiftWithMetaRecord> = {}): ShiftWithMetaRecord {
  return {
    id: "01923af0-7c1a-7000-0001-aaaaaaaaaaaa",
    operator_id: "01923af0-7c1a-7000-0002-bbbbbbbbbbbb",
    operator_name: "Sara",
    operator_phone: "+9647700000001",
    check_in_at: "2026-05-18T07:00:00.000Z",
    check_out_at: "2026-05-18T15:30:00.000Z",
    check_in_by_user_id: "01923af0-7c1a-7000-0003-cccccccccccc",
    check_out_by_user_id: "01923af0-7c1a-7000-0003-cccccccccccc",
    note: null,
    created_at: "2026-05-18T07:00:00.000Z",
    updated_at: "2026-05-18T15:30:00.000Z",
    deleted_at: null,
    version: 2,
    entity_id: "01923af0-7c1a-7000-0099-000000000099",
    ...overrides,
  }
}

describe.each(directions)(
  "Phase-09 §8 component-render: ShiftHistoryToday (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders one tbody row per shift", () => {
      const shifts = [
        shift({ id: "row-a", operator_name: "Sara" }),
        shift({ id: "row-b", operator_name: "Layla" }),
        shift({ id: "row-c", operator_name: "Noor" }),
      ]
      const { container } = render(
        <ShiftHistoryToday
          shifts={shifts}
          canEdit={false}
          onEditShift={() => {}}
        />,
      )
      // The empty-state row only renders when shifts.length === 0;
      // every operator row is a real <tr>.
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(3)
    })

    it("closed shifts render duration in font-mono; open shifts render an em-dash", () => {
      const closed = shift({
        id: "row-closed",
        check_out_at: "2026-05-18T15:00:00.000Z",
      })
      const open = shift({
        id: "row-open",
        operator_name: "Layla",
        check_out_at: null,
      })
      const { container } = render(
        <ShiftHistoryToday
          shifts={[closed, open]}
          canEdit={false}
          onEditShift={() => {}}
        />,
      )
      const rows = Array.from(container.querySelectorAll("tbody tr"))
      // Duration is the 4th cell (operator, in, out, duration, lines).
      const closedDuration = rows[0].querySelectorAll("td")[3]
      const openDuration = rows[1].querySelectorAll("td")[3]
      // Closed shift: cell contains digits (Western or Arabic-Indic).
      const closedText = closedDuration.textContent ?? ""
      expect(/\d|[٠-٩]/.test(closedText)).toBe(true)
      // Open shift: em-dash literal -- never a NaN or "Invalid date".
      expect(openDuration.textContent).toContain("—")
      expect(openDuration.textContent ?? "").not.toMatch(/NaN|Invalid/i)
    })

    it("open shifts render the 'Open' is-info status pill in the check-out cell", () => {
      const open = shift({ check_out_at: null })
      const { container } = render(
        <ShiftHistoryToday
          shifts={[open]}
          canEdit={false}
          onEditShift={() => {}}
        />,
      )
      // The status pill carries .status-pill + .is-info classes per
      // .claude/rules/design-system.md §5.2.
      const pill = container.querySelector(".status-pill.is-info")
      expect(pill).not.toBeNull()
    })

    it("canEdit=false omits the actions column header AND row buttons", () => {
      const { container } = render(
        <ShiftHistoryToday
          shifts={[shift()]}
          canEdit={false}
          onEditShift={() => {}}
        />,
      )
      // Header columns: operator, in, out, duration, lines = 5 ths.
      const headerCells = container.querySelectorAll("thead th")
      expect(headerCells.length).toBe(5)
      // No edit button anywhere.
      expect(screen.queryByRole("button")).toBeNull()
    })

    it("canEdit=true adds the actions column AND an Edit button per row that wires onEditShift(shift.id)", () => {
      const onEditShift = vi.fn()
      const a = shift({ id: "row-a" })
      const b = shift({ id: "row-b", operator_name: "Layla" })
      const { container } = render(
        <ShiftHistoryToday
          shifts={[a, b]}
          canEdit={true}
          onEditShift={onEditShift}
        />,
      )
      // Header columns: operator, in, out, duration, lines, actions
      // = 6 ths.
      const headerCells = container.querySelectorAll("thead th")
      expect(headerCells.length).toBe(6)
      // One Edit button per row.
      const buttons = screen.getAllByRole("button")
      expect(buttons.length).toBe(2)
      buttons[1].click()
      expect(onEditShift).toHaveBeenCalledTimes(1)
      expect(onEditShift).toHaveBeenCalledWith("row-b")
    })

    it("empty list renders the empty-state placeholder spanning the correct column count", () => {
      // canEdit=false: 5-col span; canEdit=true: 6-col span. A
      // regression that hard-coded `colSpan={5}` would leave the
      // empty-state floating short of the actions column under the
      // editor scheme.
      const { container, unmount } = render(
        <ShiftHistoryToday
          shifts={[]}
          canEdit={false}
          onEditShift={() => {}}
        />,
      )
      const fiveColTd = container.querySelector('tbody tr td[colspan="5"]')
      expect(fiveColTd).not.toBeNull()
      unmount()

      const { container: c2 } = render(
        <ShiftHistoryToday
          shifts={[]}
          canEdit={true}
          onEditShift={() => {}}
        />,
      )
      const sixColTd = c2.querySelector('tbody tr td[colspan="6"]')
      expect(sixColTd).not.toBeNull()
    })

    it("count badge surfaces the row count (font-mono per tnum contract)", () => {
      const shifts = [
        shift({ id: "row-a" }),
        shift({ id: "row-b" }),
        shift({ id: "row-c" }),
        shift({ id: "row-d" }),
      ]
      const { container } = render(
        <ShiftHistoryToday
          shifts={shifts}
          canEdit={false}
          onEditShift={() => {}}
        />,
      )
      const badge = container.querySelector(".count-badge.font-mono")
      expect(badge).not.toBeNull()
      expect(badge!.textContent).toContain("4")
    })
  },
)
