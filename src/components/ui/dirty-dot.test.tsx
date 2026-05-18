// Phase-09 §8 component-render assertion: DirtyDot.
//
// Smallest pure-props component in the §8 battery -- a single span with
// a boolean prop. Worth pinning because the dirty indicator is the
// phase-05 §7.29 / phase-06 §7.12 "pending sync" sentinel that shows
// up on every syncable row in every list across the app. A regression
// that flipped the crimson/ink-4 token mapping would silently make
// every list look like everything is synced (or vice-versa).

import { render, screen } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"
import { DirtyDot } from "@/components/ui/dirty-dot"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)(
  "Phase-09 §8 component-render: DirtyDot (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders a circular indicator with role=img + accessible label", () => {
      const { container } = render(<DirtyDot dirty={true} />)
      const dot = container.querySelector('[role="img"]')
      expect(dot).not.toBeNull()
      // The aria-label is a non-empty string in every locale.
      const label = dot!.getAttribute("aria-label")
      expect(label).toBeTruthy()
      expect(label!.length).toBeGreaterThan(0)
    })

    it("dirty=true uses crimson background token (pending-sync signal)", () => {
      const { container } = render(<DirtyDot dirty={true} />)
      const dot = container.querySelector('[role="img"]')
      expect(dot).not.toBeNull()
      expect(dot!.className).toMatch(/bg-crimson/)
      // No ink-4 (synced) token on a dirty dot.
      expect(dot!.className).not.toMatch(/bg-ink-4/)
    })

    it("dirty=false uses ink-4 background token (synced signal)", () => {
      const { container } = render(<DirtyDot dirty={false} />)
      const dot = container.querySelector('[role="img"]')
      expect(dot).not.toBeNull()
      expect(dot!.className).toMatch(/bg-ink-4/)
      // No crimson token on a clean dot.
      expect(dot!.className).not.toMatch(/bg-crimson/)
    })

    it("aria-label differs between dirty=true and dirty=false (a11y contract)", () => {
      const { unmount } = render(<DirtyDot dirty={true} />)
      const dirtyLabel = screen.getByRole("img").getAttribute("aria-label")
      unmount()
      render(<DirtyDot dirty={false} />)
      const cleanLabel = screen.getByRole("img").getAttribute("aria-label")
      expect(dirtyLabel).not.toBe(cleanLabel)
      // Neither label is empty -- defense against a future i18n key
      // rename that nulls the resolved string.
      expect(dirtyLabel?.length).toBeGreaterThan(0)
      expect(cleanLabel?.length).toBeGreaterThan(0)
    })

    it("title attribute mirrors aria-label (tooltip + screen-reader parity)", () => {
      const { container } = render(<DirtyDot dirty={true} />)
      const dot = container.querySelector('[role="img"]')
      expect(dot?.getAttribute("title")).toBe(dot?.getAttribute("aria-label"))
    })
  },
)
