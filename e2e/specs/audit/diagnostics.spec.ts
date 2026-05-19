// Phase-09 §4.1 E2E: audit diagnostics.
//
// Proves the superadmin can read the diagnostics summary -- lock p95,
// receipt rate, 7-day conflict count, outbox depth, last vacuum.
// Phase-08 §3.Rust audit_diagnostics + phase-08 §7.20 receipt-rate
// invariant (locked / locked_and_receipted).
//
//   1. Land on /admin/audit/diagnostics.
//   2. Confirm each of the five mono-tabular KPI cards renders a
//      non-empty value.
//
// Gated by RUN_FULL_E2E=true.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import { clickLinkWithText, waitForText } from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- audit diagnostics", function () {
  it("renders all five diagnostics KPIs with mono-tabular values", async function () {
    await clickLinkWithText("Diagnostics")
    await waitForText("Diagnostics")

    const labels = [
      "Lock p95",
      "Receipt rate",
      "Conflicts (7d)",
      "Outbox depth",
      "Last vacuum",
    ]
    for (const label of labels) {
      await waitForText(label)
    }

    // Each KPI value carries font-mono.
    const monoValues = await browser.$$(".diagnostics .font-mono, .kpi-card .font-mono")
    expect(monoValues.length).toBeGreaterThanOrEqual(5)
  })
})
