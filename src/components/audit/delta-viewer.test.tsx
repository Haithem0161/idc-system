// Phase-09 §8 component-render assertion: DeltaViewer.
//
// Pure-props component (only i18n + useMemo); third row in the §8
// component-render battery after AuditTable + MergeEditor. Drives both
// dir=ltr and dir=rtl via describe.each per §14.

import { render, screen, within } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"
import { DeltaViewer } from "@/components/audit/delta-viewer"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)(
  "Phase-09 §8 component-render: DeltaViewer (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders the empty placeholder for null delta", () => {
      render(<DeltaViewer delta={null} />)
      // The placeholder text resolves to a non-empty string in both
      // locales; Arabic-block regex doubles as a fallback check.
      const placeholder = screen.getByText(
        dir === "rtl" ? /[؀-ۿ]/ : /No changes/i,
      )
      expect(placeholder).toBeInTheDocument()
    })

    it("renders empty placeholder when delta is a non-object", () => {
      const { container } = render(<DeltaViewer delta={"a string"} />)
      // Either an empty placeholder or no rows -- the contract is that
      // a non-object delta does not crash and produces no diff rows.
      const tableRows = container.querySelectorAll("tbody tr")
      expect(tableRows.length).toBe(0)
    })

    it("renders one row per changed field in {from, to} shape", () => {
      const delta = {
        status: { from: "draft", to: "locked" },
        total: { from: 8000, to: 10000 },
        locked_at: { from: null, to: "2026-05-18T09:30:00.000Z" },
      }
      const { container } = render(<DeltaViewer delta={delta} />)
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(3)
      const text = container.textContent ?? ""
      expect(text).toContain("status")
      expect(text).toContain("total")
      expect(text).toContain("locked_at")
    })

    it("flat delta shape (synthetic audit row) renders single 'to' column", () => {
      // Vacuum self-audit emits `{ audit_purged, metrics_purged, ... }`
      // rather than the per-field { from, to } diff shape. The viewer
      // gracefully falls back to a single column.
      const delta = {
        audit_purged: 1234,
        metrics_purged: 567,
        audit_cutoff: "2026-02-17T00:00:00.000Z",
      }
      const { container } = render(<DeltaViewer delta={delta} />)
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(3)
      const text = container.textContent ?? ""
      expect(text).toContain("audit_purged")
      expect(text).toContain("1234")
    })

    it("renders the canonical column headers (Field / From / To)", () => {
      const delta = { x: { from: "a", to: "b" } }
      const { container } = render(<DeltaViewer delta={delta} />)
      const thead = container.querySelector("thead")
      expect(thead).not.toBeNull()
      const headerCells = within(thead!).getAllByRole("columnheader")
      // 3 columns: Field / From / To. The exact copy is locale-
      // dependent but the count contract is fixed at 3.
      expect(headerCells.length).toBe(3)
    })

    it("each {from, to} pair renders its own row with from in crimson and to in success colors", () => {
      // Per delta-viewer.tsx parseDelta: the viewer renders ONE row per
      // entry in the delta object (no identical-omission filter). The
      // styling contract is what guarantees the from/to relationship is
      // visually obvious -- from is crimson (decrement / pre-change),
      // to is success-green (increment / post-change). A regression that
      // dropped the color tokens would silently flatten the diff into a
      // visually-indistinguishable two-column table.
      const delta = {
        a: { from: "x", to: "y" },
        b: { from: 0, to: 1 },
      }
      const { container } = render(<DeltaViewer delta={delta} />)
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(2)
      // Each row has 3 cells; the from cell (index 1) carries
      // `text-crimson`, the to cell (index 2) carries `text-success`.
      const firstRow = rows[0]
      const cells = firstRow.querySelectorAll("td")
      expect(cells.length).toBe(3)
      expect(cells[1].className).toMatch(/text-crimson/)
      expect(cells[2].className).toMatch(/text-success/)
    })
  },
)
