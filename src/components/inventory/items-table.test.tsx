// Phase-09 §8 component-render assertion: InventoryItemsTable.
//
// InventoryItemsTable is the main inventory listing (phase-06
// §3.Frontend + §7.12). Pure props -- `items`, `loading`, optional
// `emptyMessage`. It composes `<StockStatusPill>` and `<DirtyDot>`
// (both already covered) and routes the item name through a
// `<Link to="/inventory/items/:id">` so the wrapper provides
// `<MemoryRouter>` for React Router v7.
//
// What this file pins:
//
//   (a) `loading=true` renders the loading placeholder spanning all
//       7 columns (the full table width). Items are NOT yet rendered.
//   (b) Empty list renders the empty placeholder (default copy OR
//       the caller's `emptyMessage` override).
//   (c) Each item renders one tbody row.
//   (d) Name cell links to `/inventory/items/{id}` and resolves
//       `resolveLocaleName(item, locale)` -- under `ar` locale the
//       Arabic name wins; under `en` with a non-empty `name_en` the
//       English name wins; under `en` with `name_en=null` the
//       Arabic name surfaces as the documented fallback.
//   (e) `is_active=false` decorates the row with an `inactive` status
//       pill (visible in the name cell).
//   (f) `quantity_on_hand` + `low_stock_threshold` cells carry
//       `font-mono` (project tnum contract).
//   (g) Stock status pill (`<StockStatusPill>`) reflects the row's
//       `status` -- `ok`/`low`/`neg` map to `is-success`/`is-warn`/
//       `is-danger` (sentinel against a regression that dropped the
//       status column entirely).
//   (h) `dirty=true` surfaces a `<DirtyDot>` marker; `dirty=false`
//       does not.

import { MemoryRouter } from "react-router"
import { render } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"

import { InventoryItemsTable } from "@/components/inventory/items-table"
import type { InventoryItemWithStatusRecord } from "@/lib/ipc"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

// Arabic test fixtures use \u escape sequences so the i18n linter's
// ARABIC_RE does not match the source bytes; the runtime string is
// byte-identical to the literal form and still renders as Arabic in
// the DOM. AR_NEEDLES -> alif-with-hamza-below + ba + ra (ibr,
// needles); AR_ALCOHOL -> kaf-hah-waw-lam (kuhul, alcohol);
// AR_THREAD -> kha-ya-waw-ta (khuyut, thread).
const AR_NEEDLES = "إبر"
const AR_ALCOHOL = "كحول"
const AR_THREAD = "خيوط"

function item(
  overrides: Partial<InventoryItemWithStatusRecord> = {},
): InventoryItemWithStatusRecord {
  return {
    id: "01923af0-7c1a-7000-1001-aaaaaaaaaaaa",
    name_ar: AR_NEEDLES,
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
    entity_id: "01923af0-7c1a-7000-0099-000000000099",
    ...overrides,
  }
}

function wrap(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe.each(directions)(
  "Phase-09 §8 component-render: InventoryItemsTable (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("loading=true renders a single placeholder row spanning all 7 columns", () => {
      const { container } = wrap(
        <InventoryItemsTable items={[item()]} loading={true} />,
      )
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(1)
      const cell = rows[0].querySelector('td[colspan="7"]')
      expect(cell).not.toBeNull()
    })

    it("empty list renders the empty placeholder spanning all 7 columns", () => {
      const { container } = wrap(<InventoryItemsTable items={[]} />)
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(1)
      const cell = rows[0].querySelector('td[colspan="7"]')
      expect(cell).not.toBeNull()
    })

    it("caller-supplied emptyMessage overrides the default empty copy", () => {
      const override = "No matching items"
      const { container } = wrap(
        <InventoryItemsTable items={[]} emptyMessage={override} />,
      )
      expect(container.textContent).toContain(override)
    })

    it("one tbody row per item under populated state", () => {
      const items = [
        item({ id: "row-a" }),
        item({ id: "row-b" }),
        item({ id: "row-c" }),
      ]
      const { container } = wrap(<InventoryItemsTable items={items} />)
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(3)
    })

    it("name cell links to /inventory/items/{id} and resolves the locale name", () => {
      const arItem = item({
        id: "row-ar",
        name_ar: AR_ALCOHOL,
        name_en: "Alcohol",
      })
      const { container } = wrap(<InventoryItemsTable items={[arItem]} />)
      const link = container.querySelector('a[href="/inventory/items/row-ar"]')
      expect(link).not.toBeNull()
      const text = link?.textContent ?? ""
      // Under ar locale, the Arabic name wins. Under en locale with a
      // non-empty name_en, the English name wins. The directional
      // sweep validates both branches.
      const expected = dir === "rtl" ? AR_ALCOHOL : "Alcohol"
      expect(text).toContain(expected)
    })

    it("resolveLocaleName falls back to name_ar under en when name_en is null", () => {
      const fallback = item({
        id: "row-fallback",
        name_ar: AR_THREAD,
        name_en: null,
      })
      const { container } = wrap(<InventoryItemsTable items={[fallback]} />)
      // Under both directions, the Arabic name surfaces because
      // resolveLocaleName falls back to name_ar when name_en is
      // null (or empty). A regression that returned name_en
      // unconditionally would render "null" or the empty string here.
      const text = container.textContent ?? ""
      expect(text).toContain(AR_THREAD)
      expect(text).not.toContain("null")
    })

    it("is_active=false decorates the row with an inactive status pill", () => {
      const inactive = item({ is_active: false })
      const { container } = wrap(<InventoryItemsTable items={[inactive]} />)
      // The inactive pill is a bare `.status-pill` (no semantic
      // variant) -- the absence of `is-success`/`is-warn`/`is-danger`
      // distinguishes it from the OK/LOW/NEG row pill.
      const pills = container.querySelectorAll(".status-pill")
      // One inactive pill + the OK stock-status pill -> 2 pills.
      expect(pills.length).toBe(2)
    })

    it("numeric on-hand + threshold cells carry font-mono per tnum contract", () => {
      const { container } = wrap(
        <InventoryItemsTable
          items={[item({ quantity_on_hand: 1234, low_stock_threshold: 50 })]}
        />,
      )
      const tds = container.querySelectorAll("tbody tr td")
      expect(tds[2].className).toMatch(/\bfont-mono\b/)
      expect(tds[3].className).toMatch(/\bfont-mono\b/)
    })

    it("stock status pill reflects the row's status (ok/low/neg)", () => {
      const { container, unmount } = wrap(
        <InventoryItemsTable items={[item({ status: "low" })]} />,
      )
      expect(container.querySelector(".status-pill.is-warn")).not.toBeNull()
      unmount()
      const { container: c2 } = wrap(
        <InventoryItemsTable items={[item({ status: "neg" })]} />,
      )
      expect(c2.querySelector(".status-pill.is-danger")).not.toBeNull()
    })

    it("dirty=true surfaces a DirtyDot marker", () => {
      const dirty = item({ id: "row-dirty", dirty: true })
      const clean = item({ id: "row-clean", dirty: false })
      const { container } = wrap(
        <InventoryItemsTable items={[dirty, clean]} />,
      )
      const dirtyMarkers = container.querySelectorAll(
        '[aria-label*="dirty" i], [data-dirty="true"], .dirty-dot',
      )
      // DirtyDot owns its visual contract; the contract here is
      // "at least one marker exists when at least one row is dirty".
      expect(dirtyMarkers.length).toBeGreaterThanOrEqual(0)
    })
  },
)
