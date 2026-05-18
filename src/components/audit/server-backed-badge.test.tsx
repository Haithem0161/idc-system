// Phase-09 §8 component-render assertion: ServerBackedBadge.
//
// The badge is the phase-08 §3 + §7.25 signal that an audit query has
// crossed (or predates) the 90-day local retention cliff. Three modes:
//   - `local`:  badge hidden entirely (local query, no server hit).
//   - `merged`: range spans the cliff; both stores contribute.
//   - `server`: range predates the cliff; only server-side rows.
//
// What this file pins:
//   (a) mode='local' renders nothing (the cliff hiding contract).
//   (b) mode='merged' + mode='server' render a status pill with an
//       accessible role and live region.
//   (c) The pill carries the `is-info` token (calm, not alarming --
//       the server fetch is an enrichment, not a degradation).
//   (d) A Cloud icon precedes the label (visual hint that the data
//       comes from the network).

import { render } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"
import { ServerBackedBadge } from "@/components/audit/server-backed-badge"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)(
  "Phase-09 §8 component-render: ServerBackedBadge (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("mode='local' renders nothing (hidden in local-only queries)", () => {
      const { container } = render(<ServerBackedBadge mode="local" />)
      // The component returns null for local mode; no DOM nodes.
      expect(container.children.length).toBe(0)
    })

    it("mode='merged' renders a status pill with role + aria-live", () => {
      const { container } = render(<ServerBackedBadge mode="merged" />)
      const pill = container.querySelector('[role="status"]')
      expect(pill).not.toBeNull()
      expect(pill!.getAttribute("aria-live")).toBe("polite")
    })

    it("mode='server' renders a status pill with role + aria-live", () => {
      const { container } = render(<ServerBackedBadge mode="server" />)
      const pill = container.querySelector('[role="status"]')
      expect(pill).not.toBeNull()
      expect(pill!.getAttribute("aria-live")).toBe("polite")
    })

    it("non-local modes carry the is-info token (calm, not alarming)", () => {
      const { container, unmount } = render(<ServerBackedBadge mode="merged" />)
      const merged = container.querySelector('[role="status"]')
      expect(merged?.className).toMatch(/is-info/)
      // Not crimson (alarm) or gold (warn) -- the server fetch is an
      // enrichment, not a degradation. A regression that flipped this
      // to is-warn would suggest something is wrong when actually
      // everything is working as designed.
      expect(merged?.className).not.toMatch(/is-warn|is-error|is-danger/)
      unmount()

      const { container: serverContainer } = render(
        <ServerBackedBadge mode="server" />,
      )
      const server = serverContainer.querySelector('[role="status"]')
      expect(server?.className).toMatch(/is-info/)
    })

    it("badge contains a Cloud icon (visual network-data hint)", () => {
      const { container } = render(<ServerBackedBadge mode="merged" />)
      // lucide-react renders SVGs; the icon is the first SVG inside the pill.
      const svg = container.querySelector('[role="status"] svg')
      expect(svg).not.toBeNull()
      // Cloud icon's aria-hidden semantics are explicit on the JSX.
      expect(svg?.getAttribute("aria-hidden")).toBe("true")
    })

    it("badge surfaces a tooltip via title attribute (mouseover affordance)", () => {
      const { container } = render(<ServerBackedBadge mode="merged" />)
      const pill = container.querySelector('[role="status"]')
      const title = pill?.getAttribute("title")
      expect(title).toBeTruthy()
      // Non-empty in every locale -- defense against an i18n key rename
      // that nulls the resolved tooltip.
      expect(title!.length).toBeGreaterThan(0)
    })

    it("merged + server tooltips differ (each mode explains its own behavior)", () => {
      const { container: m } = render(<ServerBackedBadge mode="merged" />)
      const mergedTip = m.querySelector('[role="status"]')?.getAttribute("title")
      const { container: s } = render(<ServerBackedBadge mode="server" />)
      const serverTip = s.querySelector('[role="status"]')?.getAttribute("title")
      expect(mergedTip).toBeTruthy()
      expect(serverTip).toBeTruthy()
      expect(mergedTip).not.toBe(serverTip)
    })

    it("label text renders the mode literal (en) or Arabic translation (ar)", () => {
      const { container } = render(<ServerBackedBadge mode="server" />)
      const text = container.textContent ?? ""
      // Either the literal mode token ("server" in en fallback) OR an
      // Arabic-block character sequence (ar locale resolved key).
      const matches =
        text.toLowerCase().includes("server") || /[؀-ۿ]/.test(text)
      expect(matches).toBe(true)
    })
  },
)
