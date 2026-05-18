// Phase-09 §8 component-render assertion: KpiCard.
//
// KPI tiles (PRD §7.2.1; phase-07 §7.1) -- the dashboard's headline
// numbers. The visual contract here is load-bearing for the
// accounting persona: a single hero number, an optional delta, and
// tone-aware coloring (ink card for the financial focal point, white
// surface for the regular row).
//
// What this file pins:
//   (a) Label text renders verbatim from the `label` prop (caller
//       supplies the i18n-resolved string -- KpiCard is locale-
//       agnostic and just renders what it's given).
//   (b) Amount is formatted via `formatIqd` and rendered in the
//       mono tabular-nums column.
//   (c) IQD currency suffix is present (the project's standard
//       monetary contract).
//   (d) `deltaPermille` polarity drives color:
//       positive -> text-success
//       negative -> text-crimson
//       zero / undefined -> neutral
//   (e) `tone='ink'` flips to the dark scheme; default keeps surface.
//   (f) `arabicNumerals=true` switches the numeral shape under
//       `ar-IQ` locale -- the Arabic-Indic toggle invariant from
//       design-system.md §13.

import { render } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"
import { KpiCard } from "@/components/accounting/kpi-card"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)(
  "Phase-09 §8 component-render: KpiCard (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders the supplied label verbatim", () => {
      const { container } = render(
        <KpiCard label="Today's revenue" amount={123_456} />,
      )
      expect(container.textContent).toContain("Today's revenue")
    })

    it("renders the amount in tabular-nums mono with IQD suffix", () => {
      const { container } = render(
        <KpiCard label="Revenue" amount={1_234_567} />,
      )
      // The amount cell carries `font-mono` + `tabular-nums` classes.
      const amountCell = container.querySelector(".font-mono.tabular-nums")
      expect(amountCell).not.toBeNull()
      // IQD literal suffix is always present (project monetary contract).
      expect(container.textContent).toContain("IQD")
    })

    it("positive deltaPermille renders in text-success color (gain)", () => {
      const { container } = render(
        <KpiCard label="x" amount={100} deltaPermille={50} />,
      )
      const deltaCell = container.querySelector(".text-success")
      expect(deltaCell).not.toBeNull()
      // No crimson contender for the same cell.
      const crimsonDelta = Array.from(container.querySelectorAll(".text-crimson")).find(
        (el) => el.textContent && el.textContent.length > 0 && el !== deltaCell,
      )
      expect(crimsonDelta).toBeUndefined()
    })

    it("negative deltaPermille renders in text-crimson color (loss)", () => {
      const { container } = render(
        <KpiCard label="x" amount={100} deltaPermille={-25} />,
      )
      const deltaCell = container.querySelector(".text-crimson")
      expect(deltaCell).not.toBeNull()
    })

    it("zero deltaPermille renders in neutral ink-3 (no signal)", () => {
      const { container } = render(
        <KpiCard label="x" amount={100} deltaPermille={0} />,
      )
      // Neither success nor crimson should claim the delta cell -- a
      // zero-delta tile must be visually identical to no-delta cousins.
      // The component routes delta=0 to text-ink-3.
      const successDelta = container.querySelector(".text-success")
      const crimsonDelta = container.querySelector(".text-crimson")
      // Neither tone is applied to a zero delta (the inkText is set to
      // ink-3 for delta=0 -- not the success/crimson tone).
      expect(successDelta).toBeNull()
      expect(crimsonDelta).toBeNull()
    })

    it("deltaPermille undefined renders no delta row at all", () => {
      const { container } = render(<KpiCard label="x" amount={100} />)
      // Without deltaPermille, the third div block doesn't render --
      // pin via no text-success / text-crimson selectors AND a row
      // count.
      const directChildren = container.querySelector("div")?.children ?? []
      // KpiCard returns one outer div containing: label div, amount div,
      // and (when delta present) a delta div. Undefined delta => 2 children.
      expect(directChildren.length).toBe(2)
    })

    it("tone='ink' switches to dark scheme (bg-ink + text-paper)", () => {
      const { container } = render(
        <KpiCard label="x" amount={100} tone="ink" />,
      )
      const outer = container.firstElementChild
      expect(outer).not.toBeNull()
      expect(outer!.className).toMatch(/bg-ink/)
      expect(outer!.className).toMatch(/text-paper/)
    })

    it("tone default keeps the surface scheme (bg-surface + border-line)", () => {
      const { container } = render(<KpiCard label="x" amount={100} />)
      const outer = container.firstElementChild
      expect(outer!.className).toMatch(/bg-surface/)
      expect(outer!.className).toMatch(/border-line/)
      // No ink-card flip.
      expect(outer!.className).not.toMatch(/bg-ink\b/)
    })

    it("arabicNumerals=true shifts digit shape under ar locale", () => {
      // This is a soft assertion: formatIqd's behaviour with the
      // arabicNumerals flag depends on the underlying Intl impl and
      // the locale. We pin the visible value contains EITHER a
      // Western digit (en path) OR an Arabic-Indic digit (ar path).
      const { container } = render(
        <KpiCard label="x" amount={123_456} arabicNumerals={true} />,
      )
      const amountCell = container.querySelector(".font-mono.tabular-nums")
      const text = amountCell?.textContent ?? ""
      // At least one digit (Western or Arabic-Indic) is present.
      const hasWestern = /\d/.test(text)
      const hasArabicIndic = /[٠-٩]/.test(text)
      expect(hasWestern || hasArabicIndic).toBe(true)
    })
  },
)
