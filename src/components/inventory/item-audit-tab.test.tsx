// Phase-09 §8 component-render assertion: ItemAuditTab.
//
// Pure-shell i18n harness. The component does no IPC and no React Query
// -- it surfaces the audit contract (entity=inventory_items, entity_id=
// itemId) and an empty-state hint while the full audit_query slice is
// not yet wired into the inventory detail tab. This test pins the
// shell's invariants so a future wiring change does not silently drop
// the contract surface that ops teams rely on to locate audit rows in
// the Mariam-persona drilldown.
//
// What this file pins (phase-06 §3.Frontend item-audit-tab seed,
// phase-08 §3.Frontend "audit rows are entity-scoped by
// entity=<table>+entity_id=<row.id>" -- the panel surface tells the
// reader exactly which audit_query parameters reach this row):
//
//   (a) Panel title resolves through `inventory.item.audit.title` in
//       both locales.
//   (b) Panel subtitle resolves through `inventory.item.audit.subtitle`
//       in both locales.
//   (c) Empty placeholder resolves through `inventory.item.audit.empty`
//       in both locales.
//   (d) The contract surface renders `entity=inventory_items` literally
//       (this is the load-bearing string operators read to know which
//       audit table holds these rows).
//   (e) The contract surface renders `entity_id=<itemId>` literally --
//       the itemId prop flows through verbatim, no truncation, no
//       prefix.
//   (f) The contract surface lines use `font-mono` so the IDs render
//       in the IDC mono stack (tabular numerals).

import { render, screen } from "@testing-library/react"
import {
  afterAll,
  beforeAll,
  describe,
  expect,
  it,
} from "vitest"

import "@/i18n"

import i18n from "i18next"

import { ItemAuditTab } from "@/components/inventory/item-audit-tab"

const directions = [["ltr"], ["rtl"]] as const

const ITEM_ID = "01923af0-7c1a-7000-8001-aaaaaaaaaaaa"

describe.each(directions)(
  "Phase-09 §8 component-render: ItemAuditTab (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders the panel title through i18n in the active locale", () => {
      const { container } = render(<ItemAuditTab itemId={ITEM_ID} />)
      expect(container.textContent ?? "").toContain(
        i18n.t("inventory.item.audit.title"),
      )
    })

    it("renders the panel subtitle through i18n in the active locale", () => {
      const { container } = render(<ItemAuditTab itemId={ITEM_ID} />)
      expect(container.textContent ?? "").toContain(
        i18n.t("inventory.item.audit.subtitle"),
      )
    })

    it("renders the empty placeholder through i18n in the active locale", () => {
      const { container } = render(<ItemAuditTab itemId={ITEM_ID} />)
      expect(container.textContent ?? "").toContain(
        i18n.t("inventory.item.audit.empty"),
      )
    })

    it("renders the canonical entity=inventory_items contract literal", () => {
      const { container } = render(<ItemAuditTab itemId={ITEM_ID} />)
      expect(container.textContent ?? "").toContain("entity=inventory_items")
    })

    it("forwards the itemId prop verbatim as entity_id=<itemId> (no truncation, no prefix)", () => {
      const { container } = render(<ItemAuditTab itemId={ITEM_ID} />)
      expect(container.textContent ?? "").toContain(`entity_id=${ITEM_ID}`)
    })

    it("renders the contract surface in the font-mono stack (IDC tnum invariant)", () => {
      const { container } = render(<ItemAuditTab itemId={ITEM_ID} />)
      const monoLines = container.querySelectorAll(".font-mono")
      // 2 mono lines: entity= + entity_id=
      expect(monoLines.length).toBeGreaterThanOrEqual(2)
      const monoText = Array.from(monoLines)
        .map((el) => el.textContent ?? "")
        .join(" ")
      expect(monoText).toContain("entity=inventory_items")
      expect(monoText).toContain(`entity_id=${ITEM_ID}`)
    })

    // Defensive reference -- silences lint warnings for the screen
    // helper while the local-DOM querySelector pattern owns the
    // assertion surface.
    void screen
  },
)
