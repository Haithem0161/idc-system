# i18n & RTL Coverage Plan

Cross-cutting plan for English/Arabic locale parity, RTL layout invariants, and Arabic-Indic numeral toggling. See `.claude/rules/testing.md` §4 + §6.2, `.claude/rules/design-system.md` §12, and `I18N.md`.

The IDC system ships in two locales: `en` (LTR, Western numerals) and `ar` (RTL, Western or Arabic-Indic numerals controlled by a setting). Every page MUST render correctly in both. Layout invariants in the design system §12 MUST hold under RTL. Every numeric column MUST honour the `arabic_numerals` setting when locale is `ar`.

## How i18n Tests Run

- Component tests (Vitest + RTL): every page-level component is rendered twice -- once with `i18n.changeLanguage('en')` and once with `'ar'`. Assertions run on both.
- Per-component RTL assertion: the component is rendered with `dir="rtl"` on the wrapper; CSS assertions verify the design-system invariants in §12 (eyebrow on right, numerals right-aligned, status pill dot leads label, side-indicator on right).
- Persona scripts P1 (Asma) and the Arabic-half of P4 (Sara) execute the journey in `ar` with `arabic_numerals: true`.
- Snapshot tests on receipts (A5 PDF + thermal) include an Arabic-locale variant per the §10 snapshot table.

## Page-by-Page Checklist

Every checkbox below MUST be green in CI for the phase that owns the page to flip to `complete`.

### Phase 02 -- Authentication & Users

| Route | en render | ar render | RTL layout | Numerals toggle | Owner |
|-|-|-|-|-|-|
| `/login` | [ ] | [ ] | [ ] | [ ] | phase-02-test §6.2 |
| `/lock` | [ ] | [ ] | [ ] | [ ] | phase-02-test §6.2 |
| `/admin/users` | [ ] | [ ] | [ ] | [ ] | phase-02-test §6.2 |
| `/admin/users/:id` | [ ] | [ ] | [ ] | [ ] | phase-02-test §6.2 |
| `/admin/settings` | [ ] | [ ] | [ ] | [ ] | phase-02-test §6.2 |

### Phase 03 -- Catalog & Reference Data

Eight admin modules. For each: list view + detail view.

| Module | en/ar | RTL | Numerals | Owner |
|-|-|-|-|-|
| Check Types | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Check Subtypes | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Doctors | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Doctor Check Pricing | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Operators | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Operator Specialties | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Inventory Items | [ ] | [ ] | [ ] | phase-03-test §6.2 |
| Inventory Consumption Map | [ ] | [ ] | [ ] | phase-03-test §6.2 |

### Phase 04 -- Operator Shifts

| Route | en/ar | RTL | Numerals | Owner |
|-|-|-|-|-|
| `/reception/shifts` | [ ] | [ ] | [ ] | phase-04-test §6.2 |

### Phase 05 -- Reception & Visit Lock

| Route | en/ar | RTL | Numerals | Owner |
|-|-|-|-|-|
| `/reception` | [ ] | [ ] | [ ] | phase-05-test §6.2 |
| `/reception/checks/:slug` | [ ] | [ ] | [ ] | phase-05-test §6.2 |
| `/reception/checks/:slug/new` | [ ] | [ ] | [ ] | phase-05-test §6.2 |
| `/reception/visits/:id` | [ ] | [ ] | [ ] | phase-05-test §6.2 |

Receipt artifacts:
- A5 receipt PDF (Arabic locale + ar numerals) -- snapshot hash.
- Thermal receipt (Arabic locale + ar numerals) -- byte-exact hash.

### Phase 06 -- Inventory Operations

| Route | en/ar | RTL | Numerals | Owner |
|-|-|-|-|-|
| `/inventory` | [ ] | [ ] | [ ] | phase-06-test §6.2 |
| `/inventory/items/:id` | [ ] | [ ] | [ ] | phase-06-test §6.2 |
| `/inventory/adjust` | [ ] | [ ] | [ ] | phase-06-test §6.2 |

### Phase 07 -- Accounting & Reports

| Route | en/ar | RTL | Numerals | Owner |
|-|-|-|-|-|
| `/accounting` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/visits` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/visits/:id` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/doctors` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/doctors/:id` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/operators` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/operators/:id` | [ ] | [ ] | [ ] | phase-07-test §6.2 |
| `/accounting/daily-close` | [ ] | [ ] | [ ] | phase-07-test §6.2 |

Report artifacts:
- Daily-close PDF (Arabic locale + ar numerals) -- snapshot hash.
- Visits CSV export (Arabic header row) -- byte-exact hash.

### Phase 08 -- Audit & Conflicts

| Route | en/ar | RTL | Numerals | Owner |
|-|-|-|-|-|
| `/audit` | [ ] | [ ] | [ ] | phase-08-test §6.2 |
| `/sync/conflicts` | [ ] | [ ] | [ ] | phase-08-test §6.2 |

## RTL Layout Invariants (from design-system.md §12)

Every page test MUST verify the following when locale is `ar`:

| Invariant | Selector / assertion |
|-|-|
| Eyebrow rule sits on the right of its text | `[data-eyebrow]::before` is on the right edge in `dir=rtl` |
| Numeric columns right-aligned in LTR flip to left-aligned in RTL | Assert `text-align: end` (logical) resolves correctly |
| Toggle thumbs translate negatively | Switch with `dir=rtl` shows thumb on the left when ON |
| Active-nav side-indicator (3px bar) on the right edge | The 3px accent bar is right-side in `dir=rtl` |
| Status pill dot leads the label | Dot is on the right of label in `dir=rtl` |
| Arabic-Indic numerals when `arabic_numerals: true` | Every numeric cell shows `٠١٢٣` style; mono font (Geist Mono) falls back gracefully |
| Mixed-direction input fields | Latin/Arabic mixed phone numbers display with correct bidi behaviour |

## Numerals Toggle Behaviour

`arabic_numerals` is a boolean setting (`settings` table, manual-conflict policy). Toggle behaviour:

| Setting state | Locale `en` | Locale `ar` |
|-|-|-|
| `false` (default) | `0123456789` | `0123456789` |
| `true` | `0123456789` (ignored in en) | `٠١٢٣٤٥٦٧٨٩` |

Tests:
- Toggle setting at runtime; assert all visible numbers re-render to the new digit shape.
- Receipt PDFs in `ar` + `true` render with Arabic-Indic digits.
- CSV exports always use Western numerals regardless of setting (for spreadsheet compatibility) -- assert this is honoured.

## First-Launch Locale Detection (DEF-007 G34)

The phase-02 build spec advertised an `ar`-forcing first-launch flow: when the desktop app boots for the first time AND the OS locale resolves to Arabic (via `tauri-plugin-os::locale()`), the i18next runtime initializes with `ar` BEFORE the first paint so the user never sees an English flash. After the first launch, the user's explicit choice (via `settings::set_locale`) overrides OS detection on every subsequent boot.

This is verified MANUALLY because the test path involves a clean app-data directory + an OS-level locale fixture, neither of which fits cleanly into the automated harness. The manual procedure:

### Procedure (run once per supported OS x locale combination)

| Step | Command | Expected outcome |
|-|-|-|
| 1 | Move or delete the app-data directory (`~/.local/share/com.idc.system/` on Linux, `~/Library/Application Support/com.idc.system/` on macOS, `%APPDATA%\com.idc.system\` on Windows) | Next launch behaves as first-run |
| 2a | Set OS locale to Arabic (Iraq): macOS System Settings -> Language; Linux `LANG=ar_IQ.UTF-8`; Windows Settings -> Time & Language | OS reports `ar` locale |
| 2b | Set OS locale to English (US) for the contrast run | OS reports `en` locale |
| 3 | `pnpm tauri dev` | First paint renders in the locale matching OS (no English flash on Arabic, no Arabic flash on English) |
| 4 | After login, open Settings; flip locale to the OPPOSITE of OS; restart | The settings-stored locale wins -- OS detection only applies on first launch |

### Pass criteria

- **Visual review** (G34 acceptance gate): no English string flashes on `ar` first launch (the loading splash + login form render in Arabic from frame zero).
- **`<html dir="rtl">`** is applied before the React tree paints.
- **Settings persistence wins on second launch**: the OS detection only fires when `settings.locale` is unset.

### Why this isn't automated

The detection runs INSIDE `tauri::run()` before the webview mounts. Driving it via WebdriverIO requires (a) controlling the OS locale per test (only feasible via container fixtures on Linux) and (b) wiping the app-data directory between runs without losing test state. Per phase-09 §10 testing.md anti-pattern row "Skipping the persona script", this is the LAST RESORT manual check and lives in `personas.md` P3 (Mariam superadmin) as step 0 of every first-run-related script.

### Owners

- Acceptance gate: **P3 Mariam the Superadmin** (`personas.md`) -- runs the manual procedure on the canonical OS (Linux x86_64) before flipping `phase-09-test.md` §8 DoD to complete.
- Reinforcement gates: optional macOS x86_64 + Windows runs when the release pipeline ships matrix builds.

## Mixed-Script Input Drills

Real users type mixed scripts. Tests:

- Patient name with Arabic + Latin characters: assert it persists round-trip through SQLite + Postgres without mojibake.
- Patient phone number entered as `+964 7XX XXX XXXX` from an Arabic keyboard: bidi-correct display in lists.
- Search FTS5 with Arabic query against a database of mixed-script names: returns expected matches.

## Coverage Tracker

| Surface | Total checkboxes | Done | Open |
|-|-|-|-|
| Phase 02 routes | 20 | 0 | 20 |
| Phase 03 modules | 32 | 0 | 32 |
| Phase 04 routes | 4 | 0 | 4 |
| Phase 05 routes | 16 | 0 | 16 |
| Phase 06 routes | 12 | 0 | 12 |
| Phase 07 routes | 32 | 0 | 32 |
| Phase 08 routes | 8 | 0 | 8 |
| Receipts / PDFs / CSVs | 5 | 0 | 5 |
| RTL invariants | 7 | 0 | 7 |
| Mixed-script drills | 3 | 0 | 3 |
| **Total** | **139** | **0** | **139** |
