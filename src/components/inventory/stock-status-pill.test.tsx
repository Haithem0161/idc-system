// Phase-09 §8 component-render assertion: StockStatusPill.
//
// StockStatusPill is the OK / LOW / NEG marker next to inventory rows
// (phase-06 §3.Frontend). Pure props -- a single `status:
// StockStatusLiteral`. It maps to the design-system status-pill
// variants (`.claude/rules/design-system.md` §5.2): `is-success` for
// ok, `is-warn` for low, `is-danger` for neg. The pill leads with a
// dot via the global token styling.
//
// What this file pins:
//
//   (a) status='ok'  -> variant class `is-success`.
//   (b) status='low' -> variant class `is-warn`.
//   (c) status='neg' -> variant class `is-danger`.
//   (d) Each variant resolves to the matching i18n key
//       (`inventory.list.status_pill.ok` / `.low` / `.neg`) -- the
//       en literal in LTR, an Arabic-block-character string in RTL.
//   (e) Variants are mutually exclusive (no danger class on an ok
//       pill, no success class on a neg pill) -- a regression that
//       flipped the ternary would surface a wrong-color pill on the
//       inventory page.

import { render } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"

import { StockStatusPill } from "@/components/inventory/stock-status-pill"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)(
  "Phase-09 §8 component-render: StockStatusPill (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("status='ok' paints the is-success variant", () => {
      const { container } = render(<StockStatusPill status="ok" />)
      const pill = container.firstElementChild
      expect(pill).not.toBeNull()
      expect(pill!.className).toMatch(/is-success/)
      expect(pill!.className).not.toMatch(/is-warn|is-danger/)
    })

    it("status='low' paints the is-warn variant", () => {
      const { container } = render(<StockStatusPill status="low" />)
      const pill = container.firstElementChild
      expect(pill!.className).toMatch(/is-warn/)
      expect(pill!.className).not.toMatch(/is-success|is-danger/)
    })

    it("status='neg' paints the is-danger variant", () => {
      const { container } = render(<StockStatusPill status="neg" />)
      const pill = container.firstElementChild
      expect(pill!.className).toMatch(/is-danger/)
      expect(pill!.className).not.toMatch(/is-success|is-warn/)
    })

    it("every variant resolves to its i18n label (non-empty in both locales)", () => {
      // The status_pill keys live under inventory.list.status_pill in
      // both en + ar locales. A regression that removed one would
      // resolve to the literal key path and fail the locale-character
      // class assertion below.
      const statuses = ["ok", "low", "neg"] as const
      for (const s of statuses) {
        const { container, unmount } = render(<StockStatusPill status={s} />)
        const text = container.textContent ?? ""
        expect(text.length).toBeGreaterThan(0)
        const matches =
          dir === "rtl"
            ? /[؀-ۿ]/.test(text)
            : /^(OK|LOW|NEG)$/.test(text.trim())
        expect(matches).toBe(true)
        unmount()
      }
    })

    it("renders a single <span> root (no wrapper divs)", () => {
      // The pill is the leaf; wrapping in a div would break the
      // inline-flow layout in tables and KPI strips.
      const { container } = render(<StockStatusPill status="ok" />)
      expect(container.children.length).toBe(1)
      expect(container.firstElementChild?.tagName).toBe("SPAN")
    })

    it("base status-pill class is always present (token system surface)", () => {
      // `.status-pill` is the design-system token at index.css that
      // carries the leading dot pseudo-element and the uppercase
      // tracking. Without it, the pill collapses to bare text.
      const { container } = render(<StockStatusPill status="low" />)
      const pill = container.firstElementChild
      expect(pill!.className).toMatch(/\bstatus-pill\b/)
    })
  },
)
