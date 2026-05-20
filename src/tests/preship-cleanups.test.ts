// Phase-09 §1.2 + §2.4 -- frontend cleanup pyramid slice.
//
// The phase-09 pre-ship audit landed 3 frontend cleanups:
//
//  1. `admin/inventory/detail.tsx` -- the `consumption_subtype_picker`
//     and `consumption_dye_unsupported` error messages route through
//     i18n keys (with `defaultValue` as the standard i18next fallback
//     pattern, not a stand-in for missing-locale work).
//  2. `setup/first-launch-setup.tsx` -- the subtitle resolves through
//     `t("setup.subtitle")`; both `en` and `ar` locales carry the key.
//  3. `shell/sidebar.tsx` -- the "Coming soon" disabled item is wired
//     with `aria-disabled="true"` + a proper i18n key, not a hardcoded
//     English string.
//
// These tests are static-source assertions: they read the relevant
// source / locale files via Vite's `?raw` and JSON imports, then pin
// the contracts. A regression (replacing the i18n key with an inline
// string, dropping the locale key, or breaking the a11y attribute on
// the disabled nav item) fails the test before it reaches review.

import { describe, expect, it } from "vitest"

// `?raw` is a Vite feature that imports any file's text content as a
// string. It works for .tsx / .ts / .json / .md / etc., bypassing the
// normal module-evaluation path. Perfect for source-file assertions
// because the file never has to be syntactically valid TypeScript
// from vitest's perspective -- it's just bytes.
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

describe("sidebar drops disabled nav items entirely", () => {
  // Phase-09 §1.2 originally rendered disabled nav items as a "Coming
  // soon" greyed span. That contract was reversed: the sidebar now
  // filters out anything the user cannot reach (role-gated or not yet
  // built), so the UI never shows a teaser the user can't act on.

  it("renderer filters items where enabled is false", () => {
    expect(sidebarSource).toMatch(/group\.items\.filter\(\(it\) => it\.enabled\)/)
  })

  it("no longer renders the aria-disabled stub branch", () => {
    expect(sidebarSource).not.toMatch(/aria-disabled="true"/)
    expect(sidebarSource).not.toMatch(/is-disabled/)
  })

  it("no longer references the nav.coming_soon i18n key", () => {
    expect(sidebarSource).not.toMatch(/nav\.coming_soon/)
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
