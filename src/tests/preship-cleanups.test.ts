// Phase-09 §1.2 + §2.4 -- frontend cleanup pyramid slice.
//
// The phase-09 pre-ship audit landed 4 frontend cleanups:
//
//  1. `auth-provider.tsx` -- the chatty `console.log("[AuthProvider]
//     /api/auth not reachable...")` was removed (it fired on every
//     standalone-browser fallback, which is the normal case in dev).
//     Legitimate `console.error` / `console.warn` lines are preserved.
//  2. `admin/inventory/detail.tsx` -- the `consumption_subtype_picker`
//     and `consumption_dye_unsupported` error messages route through
//     i18n keys (with `defaultValue` as the standard i18next fallback
//     pattern, not a stand-in for missing-locale work).
//  3. `setup/first-launch-setup.tsx` -- the subtitle resolves through
//     `t("setup.subtitle")`; both `en` and `ar` locales carry the key.
//  4. `shell/sidebar.tsx` -- the "Coming soon" disabled item is wired
//     with `aria-disabled="true"` + a proper i18n key, not a hardcoded
//     English string.
//
// These tests are static-source assertions: they read the relevant
// source / locale files via Vite's `?raw` and JSON imports, then pin
// the contracts. A regression (re-introducing `console.log`, replacing
// the i18n key with an inline string, dropping the locale key, or
// breaking the a11y attribute on the disabled nav item) fails the
// test before it reaches review.

import { describe, expect, it } from "vitest"

// `?raw` is a Vite feature that imports any file's text content as a
// string. It works for .tsx / .ts / .json / .md / etc., bypassing the
// normal module-evaluation path. Perfect for source-file assertions
// because the file never has to be syntactically valid TypeScript
// from vitest's perspective -- it's just bytes.
import authProviderSource from "@/providers/auth-provider.tsx?raw"
import adminInventoryDetailSource from "@/pages/admin/inventory/detail.tsx?raw"
import firstLaunchSetupSource from "@/components/setup/first-launch-setup.tsx?raw"
import sidebarSource from "@/components/shell/sidebar.tsx?raw"

// Locale JSONs are imported directly. The vite-tsconfig allows JSON
// module imports.
import enAdmin from "@/i18n/locales/en/admin.json"
import arAdmin from "@/i18n/locales/ar/admin.json"
import enCommon from "@/i18n/locales/en/common.json"
import arCommon from "@/i18n/locales/ar/common.json"

function dig(obj: unknown, path: string): unknown {
  return path.split(".").reduce<unknown>((acc, key) => {
    if (acc == null || typeof acc !== "object") return undefined
    return (acc as Record<string, unknown>)[key]
  }, obj)
}

describe("phase-09 §1.2 -- auth-provider chatty console.log removed", () => {
  it("does not emit console.log anywhere in auth-provider", () => {
    // The full ban on `console.log` is the SHIP-CONCERN cleanup. Anything
    // that needs to log to dev-tools must go through `console.error` (for
    // genuine failure paths) or `console.warn` (for noteworthy-but-handled
    // state changes).
    expect(authProviderSource).not.toMatch(/console\.log\(/)
  })

  it("still emits console.error on legitimate failure paths", () => {
    // Regression guard: the cleanup must not have removed the genuine
    // error logging. Two known error paths must still log:
    //   - Embedded auth timeout after 60s
    //   - Auth initialization failure
    expect(authProviderSource).toMatch(/console\.error.*Embedded auth timed out/)
    expect(authProviderSource).toMatch(/console\.error.*Auth initialization failed/)
  })

  it("still emits console.warn on embedded session expiry", () => {
    expect(authProviderSource).toMatch(/console\.warn.*Embedded session expired/)
  })
})

describe("phase-09 §1.2 -- admin/inventory/detail i18n keys present in both locales", () => {
  it("admin.inventory.consumption_subtype_picker resolves in en", () => {
    expect(dig(enAdmin, "admin.inventory.consumption_subtype_picker")).toEqual(
      expect.stringMatching(/[A-Za-z]/),
    )
  })

  it("admin.inventory.consumption_subtype_picker resolves in ar with Arabic characters", () => {
    const value = dig(arAdmin, "admin.inventory.consumption_subtype_picker")
    expect(typeof value).toBe("string")
    expect((value as string).length).toBeGreaterThan(0)
    // Sanity: Arabic locale must NOT have the English fallback bleed
    // through. Arabic strings contain at least one character in the
    // U+0600..U+06FF block.
    expect(value).toMatch(/[؀-ۿ]/)
  })

  it("admin.inventory.consumption_dye_unsupported resolves in both locales", () => {
    expect(dig(enAdmin, "admin.inventory.consumption_dye_unsupported")).toEqual(
      expect.stringMatching(/[A-Za-z]/),
    )
    const arValue = dig(arAdmin, "admin.inventory.consumption_dye_unsupported")
    expect(arValue).toMatch(/[؀-ۿ]/)
  })

  it("detail.tsx routes both error messages through t() with i18n key", () => {
    expect(adminInventoryDetailSource).toMatch(
      /t\("admin\.inventory\.consumption_subtype_picker"/,
    )
    expect(adminInventoryDetailSource).toMatch(
      /t\("admin\.inventory\.consumption_dye_unsupported"/,
    )
  })
})

describe("phase-09 §1.2 -- setup.subtitle resolves in both locales", () => {
  it("setup.subtitle is present in en", () => {
    const value = dig(enCommon, "setup.subtitle")
    expect(typeof value).toBe("string")
    expect((value as string).length).toBeGreaterThan(0)
    expect(value).toMatch(/[A-Za-z]/)
  })

  it("setup.subtitle is present in ar with Arabic characters", () => {
    const value = dig(arCommon, "setup.subtitle")
    expect(typeof value).toBe("string")
    expect((value as string).length).toBeGreaterThan(0)
    expect(value).toMatch(/[؀-ۿ]/)
  })

  it("first-launch-setup.tsx routes subtitle through t('setup.subtitle')", () => {
    expect(firstLaunchSetupSource).toMatch(/t\("setup\.subtitle"/)
  })
})

describe("phase-09 §1.2 -- sidebar 'Coming soon' state consistent", () => {
  it("disabled nav item carries aria-disabled='true' for accessibility", () => {
    // Either the disabled item is rendered (must have proper a11y) or it
    // is removed entirely. Phase-09 §1.2 left the decision open; the
    // current implementation renders it disabled, so pin that contract.
    expect(sidebarSource).toMatch(/aria-disabled="true"/)
  })

  it("disabled nav item uses the nav.coming_soon i18n key, not a hardcoded English string", () => {
    expect(sidebarSource).toMatch(/t\("nav\.coming_soon"/)
  })

  it("nav.coming_soon key is present in both en and ar locales", () => {
    expect(dig(enCommon, "nav.coming_soon")).toBe("Coming soon")
    expect(dig(arCommon, "nav.coming_soon")).toMatch(/[؀-ۿ]/)
  })

  it("disabled nav item carries the is-disabled CSS class for visual consistency", () => {
    expect(sidebarSource).toMatch(/is-disabled/)
  })
})

describe("phase-09 §2.4 -- locale-key parity (no untranslated leftovers)", () => {
  // Defensive: every key referenced by the cleanup tests must exist in
  // both locales. A regression where someone adds a new key to en but
  // forgets ar (or vice-versa) fails the same way the phase-08
  // `pnpm lint:i18n` script would. This is a smaller sanity check
  // scoped to the four cleanups only -- the broader sweep is owned by
  // phase-08 `lint:i18n`.
  const required: ReadonlyArray<readonly [unknown, unknown, string]> = [
    [enAdmin, arAdmin, "admin.inventory.consumption_subtype_picker"],
    [enAdmin, arAdmin, "admin.inventory.consumption_dye_unsupported"],
    [enCommon, arCommon, "setup.subtitle"],
    [enCommon, arCommon, "nav.coming_soon"],
  ] as const

  it.each(required)(
    "key %#: %s exists in both locales with non-empty values",
    (en, ar, keyPath) => {
      const enValue = dig(en, keyPath)
      const arValue = dig(ar, keyPath)
      expect(typeof enValue).toBe("string")
      expect(typeof arValue).toBe("string")
      expect((enValue as string).length).toBeGreaterThan(0)
      expect((arValue as string).length).toBeGreaterThan(0)
    },
  )
})
