// Phase-09 / DEF-007 G10 -- <IdleWatcher> ACTIVITY_EVENTS contract.
//
// The phase-02-test plan §10 (P02-G10) required the IdleWatcher's activity
// listener set to include `touchstart` so the idle-lock timer also resets
// on a tablet poke -- without it, a receptionist working primarily by
// touch would get locked out mid-conversation. The build cycle wired the
// listener but no test pinned the contract, so the gap stayed open as
// part of DEF-007.
//
// This test pins the four-event contract (mouse + keyboard + click + touch)
// at the source level. A regression -- removing `touchstart` from the
// array, or dropping the `{ passive: true }` listener option (which is
// the WCAG / mobile-perf invariant) -- surfaces here before review.

import { describe, expect, it } from "vitest"
import idleWatcherSource from "@/components/auth/idle-watcher.tsx?raw"

describe("phase-09 DEF-007 G10 -- IdleWatcher activity event listeners", () => {
  it("ACTIVITY_EVENTS array contains mousemove, keydown, click, AND touchstart", () => {
    // Pin the closed-set enumeration so a regression dropping touchstart
    // (the most likely cleanup mistake, since it looks like a duplicate of
    // click) fails the test.
    for (const evt of ["mousemove", "keydown", "click", "touchstart"]) {
      expect(idleWatcherSource).toMatch(new RegExp(`"${evt}"`))
    }
  })

  it("ACTIVITY_EVENTS is declared as a closed Array of DocumentEventMap keys", () => {
    // Defensive: the array is typed against DocumentEventMap so a typo in
    // an event name (e.g. 'touchstrat') fails TypeScript before runtime.
    expect(idleWatcherSource).toMatch(
      /const ACTIVITY_EVENTS:\s*Array<keyof DocumentEventMap>/,
    )
  })

  it("listeners register with passive: true to honor mobile-perf invariant", () => {
    // `{ passive: true }` lets the browser scroll without waiting for the
    // listener -- critical on touch devices. WCAG-adjacent contract.
    expect(idleWatcherSource).toMatch(/addEventListener\(\s*evt,\s*onActivity,\s*\{\s*passive:\s*true\s*\}\s*\)/)
  })

  it("listeners are cleaned up in the effect's return function", () => {
    expect(idleWatcherSource).toMatch(/removeEventListener\(\s*evt,\s*onActivity\s*\)/)
  })

  it("bump on activity is wired to the idle store, not a local timer", () => {
    // Cross-tab activity awareness comes through Zustand. Pin the wiring.
    expect(idleWatcherSource).toMatch(/const bump = useIdleStore\(\(s\) => s\.bump\)/)
    expect(idleWatcherSource).toMatch(/const onActivity = \(\) => bump\(\)/)
  })

  it("idle-lock threshold is read from settings.idle_lock_minutes with a 10-minute default", () => {
    // P02-G10 sibling invariant: the threshold must be configurable per
    // tenant via settings, not hardcoded. Default of 10 minutes is the
    // PRD value when the setting is missing or null.
    expect(idleWatcherSource).toMatch(/getSettingByKey\(settings,\s*"idle_lock_minutes"\)/)
    expect(idleWatcherSource).toMatch(/settingValueAsNumber\(setting,\s*10\)/)
  })

  it("auth_lock IPC is dispatched only when running inside Tauri", () => {
    // In a browser preview (dev) `auth_lock` isn't wired, so the IPC
    // call is guarded. Navigation to /lock still happens so the UI test
    // surface is consistent.
    expect(idleWatcherSource).toMatch(/if \(isTauri\(\)\) \{[\s\S]*?invoke\("auth_lock"\)/)
    expect(idleWatcherSource).toMatch(/navigate\("\/lock", \{ replace: true \}\)/)
  })
})
