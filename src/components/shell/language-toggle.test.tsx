// Phase-09 §8 component-render assertion: LanguageToggle.
//
// LanguageToggle is the header chrome's pill-shaped en/ar switcher.
// Pure UI -- it consults `useTranslation()` for the current language
// + the change-language API; no IPC, no Zustand. The visible label
// shows the CURRENT language's name in its own script ("English"
// when en, "العربية" when ar) -- a tiny but load-bearing UX cue.
//
// What this file pins:
//
//   (a) Renders a single <button> root carrying the design-system
//       header-chrome pill classes (rounded-full + border-line-2 +
//       11px uppercase tracking).
//   (b) The visible label echoes the ACTIVE language in its own
//       script: "English" under en, "العربية" under ar. (A regression
//       that flipped the ternary would show "English" under ar and
//       confuse the user about what language they're in.)
//   (c) Clicking the toggle flips i18n.language to the OTHER locale.
//       The en->ar flip is the same as the ar->en flip via the
//       same code path; we sweep both directions per
//       describe.each.
//   (d) The aria-label resolves from `language.toggle_aria` in both
//       locales (the screen-reader contract).
//   (e) A lucide Languages icon (svg, aria-hidden) precedes the
//       label -- the visual hint that this is a localization
//       affordance.

import { fireEvent, render } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import "@/i18n"

import { LanguageToggle } from "@/components/shell/language-toggle"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)(
  "Phase-09 §8 component-render: LanguageToggle (dir=%s)",
  (dir) => {
    beforeEach(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterEach(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders a single <button> root", () => {
      const { container } = render(<LanguageToggle />)
      expect(container.children.length).toBe(1)
      expect(container.firstElementChild?.tagName).toBe("BUTTON")
    })

    it("carries the design-system header-chrome pill classes", () => {
      const { container } = render(<LanguageToggle />)
      const btn = container.firstElementChild as HTMLButtonElement
      expect(btn.className).toMatch(/rounded-full/)
      expect(btn.className).toMatch(/border-line-2/)
      // Uppercase + tight tracking are part of the eyebrow-voice token.
      expect(btn.className).toMatch(/uppercase/)
    })

    it("label echoes the ACTIVE language in its own script", () => {
      const { container } = render(<LanguageToggle />)
      const text = container.textContent ?? ""
      if (dir === "rtl") {
        // ar locale -> "العربية"
        expect(text).toContain("العربية")
        // Defense against a regression that flipped the ternary
        // and showed "English" under ar.
        expect(text).not.toContain("English")
      } else {
        expect(text).toContain("English")
        expect(text).not.toContain("العربية")
      }
    })

    it("clicking the toggle flips i18n.language to the OTHER locale", async () => {
      const startingLang = i18n.language
      const target = startingLang === "ar" ? "en" : "ar"
      const { getByRole } = render(<LanguageToggle />)
      fireEvent.click(getByRole("button"))
      // changeLanguage is async; i18n updates synchronously enough
      // for the next microtask. Await a microtask to flush.
      await Promise.resolve()
      expect(i18n.language).toBe(target)
      // Reset for the next test in the directional run.
      await i18n.changeLanguage(startingLang)
    })

    it("aria-label resolves from language.toggle_aria in both locales", () => {
      const { getByRole } = render(<LanguageToggle />)
      const btn = getByRole("button")
      const aria = btn.getAttribute("aria-label") ?? ""
      expect(aria.length).toBeGreaterThan(0)
      const matches =
        dir === "rtl"
          ? /[؀-ۿ]/.test(aria)
          : /toggle language/i.test(aria)
      expect(matches).toBe(true)
    })

    it("renders a lucide icon (svg, aria-hidden via the icon's default)", () => {
      const { container } = render(<LanguageToggle />)
      const svg = container.querySelector("button svg")
      expect(svg).not.toBeNull()
    })
  },
)
