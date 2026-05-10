# IDC System — Frontend Summary

**Audience:** anyone touching the React frontend who didn't write the phase that just landed.
**Update cadence:** after EACH phase completion. Never batched. Reviewers block PRs that complete a phase without bumping this file.
**Last updated:** 2026-05-10 (Phase 0 — pre-implementation; this is the seed template).

## 1. Routes

Routes live in `src/routes/index.tsx` using React Router v7's `createBrowserRouter`. The shape after each phase:

| Path | File | Layout | Description | Phase added |
|-|-|-|-|-|
| `/login` | `src/pages/auth/login.tsx` | bare | Email + password; offline-aware fallback | 1 |
| `/lock` | `src/pages/auth/lock.tsx` | bare | Idle re-auth screen (uses cached creds) | 1 |
| `/` | redirect | — | Role-based redirect (superadmin → `/reception`, receptionist → `/reception`, accountant → `/accounting`) | 1 |
| `/no-access` | `src/pages/no-access.tsx` | bare | Fallback for unknown role / inactive user | 1 |
| `/audit` | `src/pages/audit/index.tsx` | app shell | Placeholder; full audit page lands in Phase 9 | 1 (placeholder) |

*Subsequent phases append rows above this line. Phase 5 lands the `/reception/*` group; Phase 7 lands `/accounting/*`; Phase 6 lands `/inventory/*`; Phase 3 lands `/admin/*`.*

## 2. Stores (Zustand)

| Store | Path | Purpose | Persisted | Phase added |
|-|-|-|-|-|
| `useThemeStore` | `src/stores/theme-store.ts` | light/dark/system | yes (`tauri-plugin-store`, per-device) | already exists |
| `useAuthStore` | `src/stores/auth-store.ts` | current user + tokens (in memory only) | no | 1 |
| `useSyncStatusStore` | `src/stores/sync-status-store.ts` | `idle | pushing | pulling | offline | error` + pending count | no | 1 |
| `useLanguageStore` | `src/stores/language-store.ts` | `ar | en` (default `ar`) | yes (per-device) | 1 |

## 3. React Query Keys & Hooks

Convention: keys are tuples; first element is the entity name; subsequent elements scope. Mutation hooks live in the same file as their list/detail hooks.

| Hook | Key | Endpoint / Command | Phase added |
|-|-|-|-|
| `useUser()` | `['user', 'me']` | `auth_get_state` IPC | 1 |
| `useSyncStatus()` | `['sync', 'status']` | Tauri event subscription `sync:status` | 1 |

*Phase 3 onward: per-entity `use<Entity>List`, `use<Entity>Detail`, `useCreate<Entity>`, `useUpdate<Entity>`, `useSoftDelete<Entity>`.*

## 4. Zod Schemas

All schemas live in `src/lib/schemas/` and are re-exported from a barrel file `src/lib/schemas/index.ts`. Domain entities mirror the SQLite shape; UI form schemas may pick a subset.

| Schema | Path | Phase added |
|-|-|-|
| `UserSchema` | `src/lib/schemas/user.ts` | 1 |
| `LoginFormSchema` | `src/lib/schemas/auth.ts` | 1 |

## 5. Shadcn Components Installed

Run `pnpm dlx shadcn@latest add <name>` per component; never bulk-install.

| Component | Phase | Notes |
|-|-|-|
| `button` | 1 | RTL-verified (chevron mirroring via `rtl:rotate-180`). |
| `card` | 1 | |
| `input` | 1 | RTL-verified (`text-start`). |
| `table` | 1 | RTL: column order auto-flips via logical properties. |
| `tabs` | 1 | |
| `dialog` | 1 | RTL-verified. |
| `badge` | 1 | |
| `toast` (sonner) | 1 | RTL: positions on the start side. |
| `skeleton` | 1 | |
| `form` | 1 | |
| `select` | 1 | |
| `radio-group` | 1 | |
| `checkbox` | 1 | |
| `alert` | 1 | |
| `separator` | 1 | |
| `scroll-area` | 1 | |

*Later phases add components per need (e.g., Phase 3 may add `command` for FTS-backed combobox; Phase 5 adds `popover` for operator picker).*

## 6. i18n Namespaces

Bundles at `src/i18n/locales/ar/<namespace>.json` and `src/i18n/locales/en/<namespace>.json`. Default locale is `ar`. RTL applied via `<html dir="rtl">` toggled by `useLanguageStore`.

| Namespace | Phase | Owner module | Notes |
|-|-|-|-|
| `common` | 1 | shared | buttons, status, generic labels. |
| `auth` | 1 | auth | login screen, lock, no-access. |
| `errors` | 1 | shared | domain error keys; consumed by Toast. |
| `reception` | 5 | reception | populated in Phase 5. |
| `accounting` | 7 | accounting | populated in Phase 7. |
| `inventory` | 6 | inventory | populated in Phase 6. |
| `admin` | 3 | admin | populated in Phase 3. |
| `audit` | 9 | audit | populated in Phase 9. |
| `receipts` | 5 | reception | receipt template strings. |

**Hard rule:** no string lives in JSX outside `t('namespace.key')`. The lint rule lands in Phase 1 (research D-?? — to be filed).

## 7. RTL Components Verified

A component is "RTL-verified" when it has been visually inspected in `pnpm tauri dev` with `<html dir="rtl">` toggled, and the following box is checked:

- [ ] Logical-property utilities used (`ps-*`, `pe-*`, `ms-*`, `me-*`, `text-start`, `text-end`).
- [ ] Chevrons / arrows mirror via `rtl:rotate-180` or alt SVG.
- [ ] Table columns flip automatically (no explicit `text-right` / `text-left`).
- [ ] Iconography (pencils, sliders, sort arrows) mirrors.
- [ ] Padding/margin shorthand uses the `start`/`end` axis.

**Tracked components:** see Section 5 above. Each row carries an "RTL-verified" stamp at install time.

## 8. Conventions Summary

- **Path alias:** `@/` → `src/`. Configured in `vite.config.ts` and `tsconfig.app.json`.
- **State sources:** server state via React Query; client state via Zustand; form state via React Hook Form + Zod.
- **Mutations:** every mutation invalidates the relevant query keys on success. No optimistic updates without a documented rollback path.
- **Errors:** `errors.<domain>.<key>` translated via `react-i18next` and toasted via `sonner`. No raw error messages in UI.
- **Sync status:** every page that lists synced rows reads `useSyncStatusStore()` to render the per-row pending dot.
- **Locale switch:** `useLanguageStore.setLanguage('en' | 'ar')` triggers `i18next.changeLanguage` and `<html dir>` flip in one go.
- **Currency:** money is integer IQD. `formatIqd(value, locale)` (in `src/lib/format.ts`, lands in Phase 5) is the only place that formats currency.

## 9. Bilingual Data Display

Domain entities `check_types`, `check_subtypes`, `inventory_items` carry `name_ar` (required) and `name_en` (optional). Resolver:

```ts
const displayName = locale === 'en' && row.name_en ? row.name_en : row.name_ar;
```

People (doctors, patients, operators) have a single `name` column; render it raw in whichever script the staff entered.

## 10. Change Log

| Date | Phase | Change |
|-|-|-|
| 2026-05-10 | 0 (seed) | File created. Phase 1 entries pre-filled to seed the template. |
