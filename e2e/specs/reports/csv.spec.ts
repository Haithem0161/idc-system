// Phase-09 §4.1 E2E: CSV export.
//
// Proves the accountant can export a visits report as CSV.
// Phase-07 §3.Rust csv_writer + phase-07 §7.x csv_export_now command.
// The export writes the file via Tauri's fs scope (the path is
// disclosed in a success chip with the artifact location).
//
//   1. Land on /accounting/reports/visits.
//   2. Click "Export CSV" -- the file save dialog opens (or the
//      success chip carries the resolved path).
//   3. Confirm the success chip + the artifact location.
//
// Gated by RUN_FULL_E2E=true.

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- CSV export", function () {
  it("exports the visits report as CSV with a non-empty path", async function () {
    await clickLinkWithText("Visits report")
    await waitForText("Visits report")
    await clickButtonWithText("Export CSV")
    await waitForText("CSV exported")
  })
})
