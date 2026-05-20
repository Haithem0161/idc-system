// Phase-09 §4 WebdriverIO E2E gate.
//
// The base wdio.conf.ts boots a single tauri-driver against the
// existing debug binary at src-tauri/target/debug/idc-system
// (`pnpm tauri build --no-bundle` rebuilds it). The phase-01 smoke
// spec (`app-shell.spec.ts`) runs unconditionally and is the "the
// binary boots and renders a webview" gate that the rest of the
// suite stands on.
//
// The full §4 surface area (login + clock in/out, new visit create +
// lock + void, inventory adjust + recompute, dashboard + visits
// report + daily close + CSV export, audit query + vacuum +
// diagnostics, conflict resolver + merge editor, plus multi-device
// drills) requires:
//
//   1. A seeded clinical-day SQLite under the app's data directory so
//      every spec starts from a known state. The phase-09 §10 12-month
//      fixture generator covers this off the Rust side.
//   2. A reset/reseed step BETWEEN specs (currently manual; the wdio
//      config has a single tauri-driver lifecycle and the binary
//      mutates the same app-data SQLite across specs).
//   3. For multi-device: TWO binaries running side-by-side against a
//      shared sync-server stack. The wdio config currently spins a
//      single instance; multi-device specs spin a second via a child
//      process inside the spec body. They are gated by MULTI_DEVICE=true
//      so the default CI path stays single-device.
//
// Until (1) and (2) are wired into the wdio onPrepare/beforeSpec
// hooks, the full §4 specs are gated by `RUN_FULL_E2E=true` -- the
// developer flips it locally after a fresh `pnpm tauri build --no-bundle`
// and a one-shot SQLite seed. CI runs the phase-01 smoke spec only.
//
// To enable the full suite locally:
//
//   1. `pnpm tauri build --no-bundle` -- rebuild the debug binary.
//   2. Seed the app-data SQLite from
//      `docs/idc-system/testing/fixtures/clinical-day.sql` (see
//      `fixtures/README.md` for the path resolution per OS).
//   3. `RUN_FULL_E2E=true pnpm test:e2e`.
//
// To enable the multi-device drills additionally:
//
//   4. `RUN_FULL_E2E=true MULTI_DEVICE=true pnpm test:e2e`.

export function gatedDescribe (
  title: string,
  body: (this: Mocha.Suite) => void,
): Mocha.Suite | void {
  if (process.env.RUN_FULL_E2E === "true") {
    return describe(title, body)
  }
  return describe.skip(title, body)
}

export function multiDeviceDescribe (
  title: string,
  body: (this: Mocha.Suite) => void,
): Mocha.Suite | void {
  if (
    process.env.RUN_FULL_E2E === "true" &&
    process.env.MULTI_DEVICE === "true"
  ) {
    return describe(title, body)
  }
  return describe.skip(title, body)
}
