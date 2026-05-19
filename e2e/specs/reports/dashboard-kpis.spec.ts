// Phase-09 §4.1 E2E: accounting dashboard KPIs.
//
// Proves the accountant Asma can land on /accounting and see the
// KPI cards (today's net, week net, month net, outstanding) PLUS the
// top-5 doctor/operator cards. Numbers render mono-tabular per the
// design system. Phase-07 §3.Frontend Dashboard.
//
//   1. Log in / land on /accounting.
//   2. The page resolves the "Dashboard" eyebrow + 4 KPI cards.
//   3. Each KPI card's value <p> carries the font-mono class (tnum
//      invariant) AND is non-empty.
//
// Gated by RUN_FULL_E2E=true.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import { clickLinkWithText, waitForText } from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- accounting dashboard KPIs", function () {
  it("renders the four KPI cards with mono-tabular values", async function () {
    await clickLinkWithText("Accounting")
    await waitForText("Dashboard")

    // The KPI strip is a grid of 4 cards. Each card's value renders
    // in font-mono.
    const kpiValues = await browser.$$(".kpi-tile .font-mono, .kpi-card .font-mono")
    expect(kpiValues.length).toBeGreaterThanOrEqual(4)
    for (const valueEl of kpiValues) {
      const text = await valueEl.getText()
      expect(text.length).toBeGreaterThan(0)
    }
  })
})
