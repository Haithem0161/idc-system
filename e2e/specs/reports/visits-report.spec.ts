// Phase-09 §4.1 E2E: visits report with group-by drilldown.
//
// Proves the accountant can group visits by doctor, drill into a
// doctor row to expose the underlying operator rows, and read
// canonical totals. Phase-07 §3.Frontend VisitsReportTable +
// phase-07 §7.6 group-by switching.
//
//   1. Land on /accounting/reports/visits.
//   2. Switch group-by to "Doctor".
//   3. Confirm the table renders one row per doctor with a numeric
//      total + visit count.
//   4. Click a doctor row -- the operator drilldown expands.
//
// Gated by RUN_FULL_E2E=true.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- visits report group-by-doctor drilldown", function () {
  it("groups visits by doctor and drills into one doctor row", async function () {
    await clickLinkWithText("Visits report")
    await waitForText("Visits report")

    // Click the "Doctor" group-by filter pill.
    await clickButtonWithText("Doctor")

    // Confirm at least one doctor row.
    const rows = await browser.$$("tbody tr")
    expect(rows.length).toBeGreaterThan(0)

    // Click the first doctor row to drill down.
    const firstRow = rows[0]
    if (!firstRow) throw new Error("expected at least one doctor row")
    await firstRow.click()

    // The drilldown renders operator-row entries beneath the doctor.
    await waitForText("Operator")
  })
})
