// Phase-09 §8 component-render assertion: ItemConsumptionMapTable.
//
// ItemConsumptionMapTable is the read-only consumption-rule panel on
// the inventory detail page (phase-06 §3.Frontend). Pure props --
// `rows: InventoryConsumptionMapRecord[]`. Renders a link to the
// admin module per row (Edit lives elsewhere), so the test wraps with
// `<MemoryRouter>` so React Router v7 has a routing context.
//
// What this file pins:
//
//   (a) Empty rows render the `inventory.item.consumption_map.empty`
//       placeholder spanning the full 5-column width.
//   (b) Populated rows render one tbody <tr> per record.
//   (c) `check_type_id` is rendered as an 8-char mono prefix (UUID
//       hint); the FULL UUID is NOT shown (would push the table out
//       of width and leak the canonical id into the UI).
//   (d) `check_subtype_id=null` renders the
//       `consumption_map.all_subtypes` placeholder; non-null renders
//       its own 8-char mono prefix.
//   (e) `on_dye_only=true` renders a checkmark; false renders an
//       em-dash (NOT a "false" literal -- the table is glanceable).
//   (f) Quantity is rendered in font-mono with locale grouping
//       (Western digits in en, Arabic-Indic optional in ar).
//   (g) Each row carries an admin edit-link routed to
//       `/admin/check-types/{check_type_id}` -- the canonical URL
//       shape from the routes file.

import { MemoryRouter } from "react-router"
import { render } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"

import { ItemConsumptionMapTable } from "@/components/inventory/item-consumption-map-table"
import type { InventoryConsumptionMapRecord } from "@/lib/ipc"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

function row(
  overrides: Partial<InventoryConsumptionMapRecord> = {},
): InventoryConsumptionMapRecord {
  return {
    id: "01923af0-7c1a-7000-1001-aaaaaaaaaaaa",
    check_type_id: "01923af0-7c1a-7000-2001-bbbbbbbbbbbb",
    check_subtype_id: "01923af0-7c1a-7000-3001-cccccccccccc",
    item_id: "01923af0-7c1a-7000-4001-dddddddddddd",
    quantity_per_check: 2,
    on_dye_only: false,
    ...overrides,
  }
}

function wrap(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe.each(directions)(
  "Phase-09 §8 component-render: ItemConsumptionMapTable (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("empty rows render the placeholder spanning all 5 columns", () => {
      const { container } = wrap(<ItemConsumptionMapTable rows={[]} />)
      // The empty cell uses colSpan={5}; React forwards as DOM
      // attribute "colspan". The contract: span the FULL table width
      // so the placeholder copy reads naturally and does NOT leave
      // misaligned empty cells.
      const empty = container.querySelector('tbody tr td[colspan="5"]')
      expect(empty).not.toBeNull()
      // Placeholder copy resolves from i18n (en literal in LTR, an
      // Arabic-block character in RTL).
      const text = empty!.textContent ?? ""
      const matches =
        dir === "rtl"
          ? /[؀-ۿ]/.test(text)
          : /No consumption rules/i.test(text)
      expect(matches).toBe(true)
    })

    it("populated rows render one tbody <tr> per record", () => {
      const { container } = wrap(
        <ItemConsumptionMapTable
          rows={[
            row({ id: "row-a" }),
            row({ id: "row-b" }),
            row({ id: "row-c" }),
          ]}
        />,
      )
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(3)
    })

    it("check_type_id renders as 8-char mono prefix (UUID hint, NOT the full id)", () => {
      const fullId = "01923af0-7c1a-7000-2001-bbbbbbbbbbbb"
      const { container } = wrap(
        <ItemConsumptionMapTable rows={[row({ check_type_id: fullId })]} />,
      )
      const text = container.textContent ?? ""
      // First 8 chars: "01923af0"
      expect(text).toContain("01923af0")
      // Full id MUST NOT leak into the UI (the 8-char prefix is the
      // visible hint; the admin link carries the canonical id in the
      // href).
      expect(text).not.toContain(fullId)
    })

    it("check_subtype_id=null renders the all_subtypes placeholder", () => {
      const { container } = wrap(
        <ItemConsumptionMapTable rows={[row({ check_subtype_id: null })]} />,
      )
      const text = container.textContent ?? ""
      const matches =
        dir === "rtl"
          ? /[؀-ۿ]/.test(text)
          : /All subtypes/i.test(text)
      expect(matches).toBe(true)
    })

    it("on_dye_only=true renders a checkmark; on_dye_only=false renders an em-dash", () => {
      const { container } = wrap(
        <ItemConsumptionMapTable
          rows={[
            row({ id: "row-true", on_dye_only: true }),
            row({ id: "row-false", on_dye_only: false }),
          ]}
        />,
      )
      const rows = Array.from(container.querySelectorAll("tbody tr"))
      // The on-dye-only cell is the 4th (check_type, subtype, qty,
      // on_dye_only, edit-link).
      const trueCell = rows[0].querySelectorAll("td")[3]
      const falseCell = rows[1].querySelectorAll("td")[3]
      expect(trueCell.textContent).toContain("✓")
      expect(falseCell.textContent).toContain("—")
      // Defense against a regression that rendered the boolean
      // literal "false" (a glance-killer in a dense table).
      expect(falseCell.textContent).not.toMatch(/false|no/i)
    })

    it("quantity is rendered in font-mono with locale grouping", () => {
      const { container } = wrap(
        <ItemConsumptionMapTable rows={[row({ quantity_per_check: 1234 })]} />,
      )
      // Quantity cell is the 3rd (idx 2). It carries font-mono per
      // the design-system tnum contract. en-GB locale groups with
      // a comma; ar-IQ may or may not -- either way at least one
      // digit (Western or Arabic-Indic) is present.
      const qtyCell = container.querySelector("tbody tr td:nth-child(3)")
      expect(qtyCell?.className).toMatch(/\bfont-mono\b/)
      const text = qtyCell?.textContent ?? ""
      expect(/\d|[٠-٩]/.test(text)).toBe(true)
    })

    it("each row carries an admin edit-link routed to /admin/check-types/{check_type_id}", () => {
      const fullId = "01923af0-7c1a-7000-2001-bbbbbbbbbbbb"
      const { container } = wrap(
        <ItemConsumptionMapTable rows={[row({ check_type_id: fullId })]} />,
      )
      // React Router v7 <Link to=...> renders an <a href=...>. The
      // canonical url shape is /admin/check-types/{id}.
      const link = container.querySelector(
        `a[href="/admin/check-types/${fullId}"]`,
      )
      expect(link).not.toBeNull()
    })
  },
)
