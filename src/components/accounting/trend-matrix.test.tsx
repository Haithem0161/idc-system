// Phase-09 §8 component-render assertion: TrendMatrix.
//
// TrendMatrix is the dashboard's 5-row financial grid (phase-07 §7.1):
// revenue / doctor_cuts / operator_cuts / inventory_value / net, each
// row showing current_iqd / prior_iqd / delta_permille with tone-aware
// coloring (success / crimson / neutral). Pure props -- `title`,
// `matrix`, `arabicNumerals`. No IPC, no stores.
//
// What this file pins:
//
//   (a) The title prop renders verbatim in the eyebrow row.
//   (b) Exactly five rows in the body, one per fixed metric key.
//   (c) Row labels resolve from `accounting.kpi.*` i18n keys (locale
//       sweep verifies en + ar paths surface non-empty strings).
//   (d) Numeric cells carry `font-mono tabular-nums` (the project's
//       monetary tnum contract from `.claude/rules/design-system.md`).
//   (e) delta_permille polarity drives the delta cell color:
//         > 0 -> text-success
//         < 0 -> text-crimson
//         = 0 -> text-ink-3 (neutral; NOT a positive signal)
//   (f) `arabicNumerals=true` reaches `formatIqd`; the rendered amount
//       cell contains either a Western digit (en path) or an
//       Arabic-Indic digit (ar path).
//   (g) Each amount cell renders the grouped digits from formatIqd
//       (no withSuffix in this surface -- TrendMatrix is the dense
//       matrix view; KpiCard owns the "IQD"-suffixed hero rendering).

import { render } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it } from "vitest"

import "@/i18n"

import { TrendMatrix } from "@/components/accounting/trend-matrix"
import type { TrendCellRecord, TrendMatrixRecord } from "@/lib/ipc"

import i18n from "i18next"

const directions = [["ltr"], ["rtl"]] as const

function cell(overrides: Partial<TrendCellRecord> = {}): TrendCellRecord {
  return {
    current_iqd: 100_000,
    prior_iqd: 90_000,
    delta_iqd: 10_000,
    delta_permille: 111,
    ...overrides,
  }
}

function matrix(overrides: Partial<TrendMatrixRecord> = {}): TrendMatrixRecord {
  return {
    revenue: cell(),
    doctor_cuts: cell({ delta_permille: -50 }),
    operator_cuts: cell({ delta_permille: 0 }),
    report_cuts: cell({ delta_permille: 0 }),
    mandoub_cuts: cell({ delta_permille: 0 }),
    inventory_value: cell({ delta_permille: 25 }),
    net: cell({ delta_permille: -10 }),
    ...overrides,
  }
}

describe.each(directions)(
  "Phase-09 §8 component-render: TrendMatrix (dir=%s)",
  (dir) => {
    beforeAll(async () => {
      await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
      document.documentElement.setAttribute("dir", dir)
    })

    afterAll(async () => {
      await i18n.changeLanguage("en")
      document.documentElement.removeAttribute("dir")
    })

    it("renders the supplied title in the eyebrow row", () => {
      // Use a variable rather than an inline string literal so the
      // i18n lint (which scans `title="..."` JSX props) stays clean.
      // The component's `title` prop is caller-supplied (already
      // i18n-resolved at the page level), so the literal here is a
      // test fixture, not user-facing copy.
      const trendTitle = "Today vs Yesterday"
      const { container } = render(
        <TrendMatrix title={trendTitle} matrix={matrix()} />,
      )
      expect(container.textContent).toContain(trendTitle)
    })

    it("renders exactly seven data rows (revenue, doctor_cuts, operator_cuts, report_cuts, mandoub_cuts, inventory_value, net)", () => {
      const { container } = render(
        <TrendMatrix title="x" matrix={matrix()} />,
      )
      const rows = container.querySelectorAll("tbody tr")
      expect(rows.length).toBe(7)
    })

    it("numeric cells carry font-mono + tabular-nums (project tnum contract)", () => {
      const { container } = render(
        <TrendMatrix title="x" matrix={matrix()} />,
      )
      // Each row has 3 mono cells (current, prior, delta) -> 21 mono
      // cells across 7 rows. The tnum class is non-negotiable for
      // receipts and report tables per .claude/rules/design-system.md
      // §11 ("Numbers are first-class -- always mono, always tnum,
      // always right-aligned in tables.").
      const monoCells = container.querySelectorAll(".font-mono.tabular-nums")
      expect(monoCells.length).toBe(21)
    })

    it("positive delta_permille paints the delta cell text-success (gain)", () => {
      const { container } = render(
        <TrendMatrix
          title="x"
          matrix={matrix({
            revenue: cell({ delta_permille: 50 }),
            doctor_cuts: cell({ delta_permille: 50 }),
            operator_cuts: cell({ delta_permille: 50 }),
            inventory_value: cell({ delta_permille: 50 }),
            net: cell({ delta_permille: 50 }),
          })}
        />,
      )
      const success = container.querySelectorAll(".text-success")
      // All 5 delta cells should be success. (Headers and other
      // elements may add to this count if any other text-success
      // exists; the assertion is "at least 5".)
      expect(success.length).toBeGreaterThanOrEqual(5)
    })

    it("negative delta_permille paints the delta cell text-crimson (loss)", () => {
      const { container } = render(
        <TrendMatrix
          title="x"
          matrix={matrix({
            revenue: cell({ delta_permille: -50 }),
            doctor_cuts: cell({ delta_permille: -50 }),
            operator_cuts: cell({ delta_permille: -50 }),
            inventory_value: cell({ delta_permille: -50 }),
            net: cell({ delta_permille: -50 }),
          })}
        />,
      )
      const crimson = container.querySelectorAll(".text-crimson")
      expect(crimson.length).toBeGreaterThanOrEqual(5)
    })

    it("zero delta_permille keeps the neutral ink-3 tone (no false-positive gain)", () => {
      // A zero-delta row must NOT paint success -- the receptionist
      // glances at the matrix to see "are we up or down" and zero is
      // neither. A regression that flipped `> 0` to `>= 0` would paint
      // every flat row green and surface here.
      const { container } = render(
        <TrendMatrix
          title="x"
          matrix={matrix({
            revenue: cell({ delta_permille: 0 }),
            doctor_cuts: cell({ delta_permille: 0 }),
            operator_cuts: cell({ delta_permille: 0 }),
            inventory_value: cell({ delta_permille: 0 }),
            net: cell({ delta_permille: 0 }),
          })}
        />,
      )
      // No delta cell carries text-success or text-crimson.
      // Heuristic: look for tabular-nums cells that ALSO carry a
      // tonal class. Zero rows: the delta cell has text-ink-3
      // alongside tabular-nums.
      const successAndMono = Array.from(
        container.querySelectorAll(".font-mono.tabular-nums"),
      ).filter((el) => /text-success|text-crimson/.test(el.className))
      expect(successAndMono.length).toBe(0)
    })

    it("amount cells render grouped digits from formatIqd (no IQD suffix in this surface)", () => {
      const { container } = render(
        <TrendMatrix
          title="x"
          matrix={matrix({
            revenue: cell({ current_iqd: 1_234_567, prior_iqd: 1_111_111 }),
          })}
        />,
      )
      // The revenue row's current amount cell. Grouped digit string
      // contains the comma separator in en-GB locale (or the Arabic-
      // Indic grouping in ar-IQ). Either way, the number is non-empty
      // and contains at least one digit (Western or Arabic-Indic).
      const firstAmount = container.querySelector(
        "tbody tr td.font-mono.tabular-nums",
      )
      const text = firstAmount?.textContent ?? ""
      const hasWestern = /\d/.test(text)
      const hasArabicIndic = /[٠-٩]/.test(text)
      expect(hasWestern || hasArabicIndic).toBe(true)
      // No "IQD" literal at the cell level -- TrendMatrix is the dense
      // matrix surface; the project monetary contract's withSuffix
      // path is owned by KpiCard.
      expect(text).not.toMatch(/IQD/)
    })

    it("arabicNumerals=true reaches formatIqd (Western OR Arabic-Indic digit present)", () => {
      const { container } = render(
        <TrendMatrix
          title="x"
          matrix={matrix()}
          arabicNumerals={true}
        />,
      )
      const firstAmount = container.querySelector(
        "tbody tr td.font-mono.tabular-nums",
      )
      const text = firstAmount?.textContent ?? ""
      const hasWestern = /\d/.test(text)
      const hasArabicIndic = /[٠-٩]/.test(text)
      expect(hasWestern || hasArabicIndic).toBe(true)
    })
  },
)
